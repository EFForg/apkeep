use std::collections::HashMap;
use std::path::Path;
use std::rc::Rc;

use futures_util::StreamExt;
use gpapi::error::ErrorKind as GpapiErrorKind;
use gpapi::Gpapi;
use indicatif::MultiProgress;
use tokio::time::{sleep, Duration as TokioDuration};

use crate::util::progress_bar::progress_wrapper;

pub async fn download_apps(
    apps: Vec<(String, Option<String>)>,
    parallel: usize,
    sleep_duration: u64,
    email: &str,
    aas_token: Option<&str>,
    auth_token: Option<&str>,
    outpath: &Path,
    accept_tos: bool,
    mut options: HashMap<&str, &str>,
) -> usize {
    let device = options.remove("device").unwrap_or("px_9a");
    let split_apk = match options.remove("split_apk") {
        Some(val) if val == "1" || val.to_lowercase() == "true" => true,
        _ => false,
    };
    let include_additional_files = match options.remove("include_additional_files") {
        Some(val) if val == "1" || val.to_lowercase() == "true" => true,
        _ => false,
    };
    let include_dex_metadata = match options.remove("include_dex_metadata") {
        Some(val) if val == "1" || val.to_lowercase() == "true" => true,
        _ => false,
    };
    let mut gpa = match options.remove("device_properties_file") {
        None => Gpapi::new(device, email),
        Some(file) => Gpapi::from_device_properties_file(device, email, file)
    };
    if let Some(locale) = options.remove("locale") {
        gpa.set_locale(locale);
    }
    if let Some(timezone) = options.remove("timezone") {
        gpa.set_timezone(timezone);
    }

    // Set the appropriate token type
    if let Some(aas) = aas_token {
        gpa.set_aas_token(aas);
    } else if let Some(auth) = auth_token {
        gpa.set_auth_token(auth);
    } else {
        eprintln!("Either AAS token or AUTH token must be provided");
        std::process::exit(1);
    }
    if let Err(err) = gpa.login().await {
        match err.kind() {
            GpapiErrorKind::TermsOfService => {
                if accept_tos {
                    match gpa.accept_tos().await {
                        Ok(_) => {
                            println!("Google Play Terms of Service accepted.");
                        },
                        Err(e) => {
                            eprintln!("Could not accept Google Play Terms of Service: {}", e);
                            std::process::exit(1);
                        },
                    }
                } else {
                    println!("{}\nPlease read the ToS here: https://play.google.com/about/play-terms/index.html\nIf you accept, please pass the --accept-tos flag.", err);
                    std::process::exit(1);
                }
            },
            _ => {
                eprintln!("Could not log in to Google Play.  Please check your credentials and try again later. {}", err);
                std::process::exit(1);
            }
        }
    }

    let mp = Rc::new(MultiProgress::new());
    let gpa = Rc::new(gpa);
    let results = futures_util::stream::iter(
        apps.into_iter().map(|app| {
            let (app_id, app_version) = app;
            let gpa = Rc::clone(&gpa);
            let mp_dl1 = Rc::clone(&mp);
            let mp_dl2 = Rc::clone(&mp);
            let mp_dl3 = Rc::clone(&mp);
            let mp_log = Rc::clone(&mp);

            async move {
                if let Some(app_version) = app_version {
                    mp_log.suspend(|| eprintln!("Specific versions can not be downloaded from Google Play ({}@{}). Skipping...", app_id, app_version));
                    return false;
                }
                mp_log.suspend(|| println!("Downloading {}...", app_id));
                if sleep_duration > 0 {
                    sleep(TokioDuration::from_millis(sleep_duration)).await;
                }
                match gpa.download(&app_id, None, split_apk, include_dex_metadata, include_additional_files, Path::new(outpath), Some(&progress_wrapper(mp_dl1))).await {
                    Ok(_) => {
                        mp_log.suspend(|| println!("{} downloaded successfully!", app_id));
                        true
                    }
                    Err(err) if matches!(err.kind(), GpapiErrorKind::FileExists) => {
                        mp_log.suspend(|| eprintln!("File already exists for {}. Skipping...", app_id));
                        true
                    }
                    Err(err) if matches!(err.kind(), GpapiErrorKind::DirectoryExists) => {
                        mp_log.suspend(|| eprintln!("Split APK directory already exists for {}. Skipping...", app_id));
                        true
                    }
                    Err(err) if matches!(err.kind(), GpapiErrorKind::InvalidApp) => {
                        mp_log.suspend(|| eprintln!("Could not download {}. The app may be paid, nonexistent, restricted to certain accounts, unavailable in this region, or incompatible with the selected device. Skipping...", app_id));
                        false
                    }
                    Err(err) if matches!(err.kind(), GpapiErrorKind::PermissionDenied) => {
                        mp_log.suspend(|| eprintln!("Permission denied when attempting to write file for {}. Skipping...", app_id));
                        false
                    }
                    Err(_) => {
                        mp_log.suspend(|| eprintln!("An error has occurred attempting to download {}.  Retry #1...", app_id));
                        match gpa.download(&app_id, None, split_apk, include_dex_metadata, include_additional_files, Path::new(outpath), Some(&progress_wrapper(mp_dl2))).await {
                            Ok(_) => {
                                mp_log.suspend(|| println!("{} downloaded successfully!", app_id));
                                true
                            }
                            Err(_) => {
                                mp_log.suspend(|| eprintln!("An error has occurred attempting to download {}.  Retry #2...", app_id));
                                match gpa.download(&app_id, None, split_apk, include_dex_metadata, include_additional_files, Path::new(outpath), Some(&progress_wrapper(mp_dl3))).await {
                                    Ok(_) => {
                                        mp_log.suspend(|| println!("{} downloaded successfully!", app_id));
                                        true
                                    }
                                    Err(_) => {
                                        mp_log.suspend(|| eprintln!("An error has occurred attempting to download {}. Skipping...", app_id));
                                        false
                                    }
                                }
                            }
                        }
                    }
                }
            }
        })
    ).buffer_unordered(parallel).collect::<Vec<bool>>().await;
    results.into_iter().filter(|&success| !success).count()
}

pub async fn request_aas_token(
    email: &str,
    oauth_token: &str,
    mut options: HashMap<&str, &str>,
) {
    let device = options.remove("device").unwrap_or("px_9a");
    let mut api = match options.remove("device_properties_file") {
        None => Gpapi::new(device, email),
        Some(file) => Gpapi::from_device_properties_file(device, email, file)
    };
    match api.request_aas_token(oauth_token).await {
        Ok(()) => {
            let aas_token = api.get_aas_token().unwrap();
            println!("AAS Token: {}", aas_token);
        },
        Err(_) => {
            println!("Error: was not able to retrieve AAS token with the provided OAuth token. Please provide new OAuth token and try again.");
        }
    }
}

pub fn list_versions(apps: Vec<(String, Option<String>)>) {
    for app in apps {
        let (app_id, _) = app;
        println!("Versions available for {} on Google Play:", app_id);
        println!("| Google Play does not make old versions of apps available.");
    }
}
