use std::collections::HashMap;
use std::path::Path;
use std::rc::Rc;

use futures_util::StreamExt;
use gpapi::error::ErrorKind as GpapiErrorKind;
use gpapi::Gpapi;
use indicatif::MultiProgress;
use tokio::time::{sleep, Duration as TokioDuration};

use crate::progress_bar::progress_wrapper;

pub async fn download_apps(
    apps: Vec<(String, Option<String>)>,
    parallel: usize,
    sleep_duration: u64,
    username: &str,
    password: &str,
    outpath: &Path,
    mut options: HashMap<&str, &str>,
) {
    let locale = options.remove("locale").unwrap_or("en_US");
    let timezone = options.remove("timezone").unwrap_or("UTC");
    let device = options.remove("device").unwrap_or("hero2lte");
    let split_apk = match options.remove("split_apk") {
        Some(val) if val == "1" || val.to_lowercase() == "true" => true,
        _ => false,
    };
    let include_additional_files = match options.remove("include_additional_files") {
        Some(val) if val == "1" || val.to_lowercase() == "true" => true,
        _ => false,
    };
    let mut gpa = Gpapi::new(locale, timezone, device);

    if let Err(err) = gpa.login(username, password).await {
        match err.kind() {
            GpapiErrorKind::SecurityCheck | GpapiErrorKind::EncryptLogin => println!("{}", err),
            _ => println!("Could not log in to Google Play.  Please check your credentials and try again later."),
        }
        std::process::exit(1);
    }

    let mp = Rc::new(MultiProgress::new());
    let gpa = Rc::new(gpa);
    futures_util::stream::iter(
        apps.into_iter().map(|app| {
            let (app_id, app_version) = app;
            let gpa = Rc::clone(&gpa);
            let mp_dl1 = Rc::clone(&mp);
            let mp_dl2 = Rc::clone(&mp);
            let mp_dl3 = Rc::clone(&mp);
            let mp_log = Rc::clone(&mp);

            async move {
                if app_version.is_none() {
                    mp_log.println(format!("Downloading {}...", app_id)).unwrap();
                    if sleep_duration > 0 {
                        sleep(TokioDuration::from_millis(sleep_duration)).await;
                    }
                    match gpa.download(&app_id, None, split_apk, include_additional_files, Path::new(outpath), Some(&progress_wrapper(mp_dl1))).await {
                        Ok(_) => mp_log.println(format!("{} downloaded successfully!", app_id)).unwrap(),
                        Err(err) if matches!(err.kind(), GpapiErrorKind::FileExists) => {
                            mp_log.println(format!("File already exists for {}. Skipping...", app_id)).unwrap();
                        }
                        Err(err) if matches!(err.kind(), GpapiErrorKind::DirectoryExists) => {
                            mp_log.println(format!("Split APK directory already exists for {}. Skipping...", app_id)).unwrap();
                        }
                        Err(err) if matches!(err.kind(), GpapiErrorKind::InvalidApp) => {
                            mp_log.println(format!("Invalid app response for {}. Skipping...", app_id)).unwrap();
                        }
                        Err(err) if matches!(err.kind(), GpapiErrorKind::PermissionDenied) => {
                            mp_log.println(format!("Permission denied when attempting to write file for {}. Skipping...", app_id)).unwrap();
                        }
                        Err(_) => {
                            mp_log.println(format!("An error has occurred attempting to download {}.  Retry #1...", app_id)).unwrap();
                            match gpa.download(&app_id, None, split_apk, include_additional_files, Path::new(outpath), Some(&progress_wrapper(mp_dl2))).await {
                                Ok(_) => mp_log.println(format!("{} downloaded successfully!", app_id)).unwrap(),
                                Err(_) => {
                                    mp_log.println(format!("An error has occurred attempting to download {}.  Retry #2...", app_id)).unwrap();
                                    match gpa.download(&app_id, None, split_apk, include_additional_files, Path::new(outpath), Some(&progress_wrapper(mp_dl3))).await {
                                        Ok(_) => mp_log.println(format!("{} downloaded successfully!", app_id)).unwrap(),
                                        Err(_) => {
                                            mp_log.println(format!("An error has occurred attempting to download {}. Skipping...", app_id)).unwrap();
                                        }
                                    }
                                }
                            }
                        }
                    }
                } else {
                    mp_log.println(format!("Specific versions can not be downloaded from Google Play ({}@{}). Skipping...", app_id, app_version.unwrap())).unwrap();
                }
            }
        })
    ).buffer_unordered(parallel).collect::<Vec<()>>().await;
}

pub fn list_versions(apps: Vec<(String, Option<String>)>) {
    for app in apps {
        let (app_id, _) = app;
        println!("Versions available for {} on Google Play:", app_id);
        println!("| Google Play does not make old versions of apps available.");
    }
}
