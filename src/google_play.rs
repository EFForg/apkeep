use std::collections::HashMap;
use std::path::Path;
use std::rc::Rc;

use futures_util::StreamExt;
use gpapi::error::ErrorKind as GpapiErrorKind;
use gpapi::Gpapi;
use tokio::time::{sleep, Duration as TokioDuration};

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
    let gpa = Rc::new(gpa);

    futures_util::stream::iter(
        apps.into_iter().map(|app| {
            let (app_id, app_version) = app;
            let gpa = Rc::clone(&gpa);
            async move {
                if app_version.is_none() {
                    println!("Downloading {}...", app_id);
                    if sleep_duration > 0 {
                        sleep(TokioDuration::from_millis(sleep_duration)).await;
                    }
                    match gpa.download(&app_id, None, split_apk, include_additional_files, Path::new(outpath)).await {
                        Ok(_) => println!("{} downloaded successfully!", app_id),
                        Err(err) if matches!(err.kind(), GpapiErrorKind::FileExists) => {
                            println!("File already exists for {}. Skipping...", app_id);
                        }
                        Err(err) if matches!(err.kind(), GpapiErrorKind::DirectoryExists) => {
                            println!("Split APK directory already exists for {}. Skipping...", app_id);
                        }
                        Err(err) if matches!(err.kind(), GpapiErrorKind::InvalidApp) => {
                            println!("Invalid app response for {}. Skipping...", app_id);
                        }
                        Err(err) if matches!(err.kind(), GpapiErrorKind::PermissionDenied) => {
                            println!("Permission denied when attempting to write file for {}. Skipping...", app_id);
                        }
                        Err(_) => {
                            println!("An error has occurred attempting to download {}.  Retry #1...", app_id);
                            match gpa.download(&app_id, None, split_apk, include_additional_files, Path::new(outpath)).await {
                                Ok(_) => println!("{} downloaded successfully!", app_id),
                                Err(_) => {
                                    println!("An error has occurred attempting to download {}.  Retry #2...", app_id);
                                    match gpa.download(&app_id, None, split_apk, include_additional_files, Path::new(outpath)).await {
                                        Ok(_) => println!("{} downloaded successfully!", app_id),
                                        Err(_) => {
                                            println!("An error has occurred attempting to download {}. Skipping...", app_id);
                                        }
                                    }
                                }
                            }
                        }
                    }
                } else {
                    println!("Specific versions can not be downloaded from Google Play ({}@{}). Skipping...", app_id, app_version.unwrap());
                }
            }
        })
    ).buffer_unordered(parallel).collect::<Vec<()>>().await;
}

pub async fn show_app_detail(
    apps: Vec<(String, Option<String>)>,
    username: &str,
    password: &str,
    mut options: HashMap<&str, &str>,
) {
    for app in apps {
        let (app_id, _) = app;
        let locale = options.remove("locale").unwrap_or("en_US");
        let timezone = options.remove("timezone").unwrap_or("UTC");
        let device = options.remove("device").unwrap_or("hero2lte");

        let mut gpa = Gpapi::new(locale, timezone, device);

        if let Err(err) = gpa.login(username, password).await {
            match err.kind() {
                GpapiErrorKind::SecurityCheck | GpapiErrorKind::EncryptLogin => println!("{}", err),
                _ => println!("Could not log in to Google Play.  Please check your credentials and try again later."),
            }
            std::process::exit(1);
        }

        let gpa = Rc::new(gpa);
        let details = gpa.details(&app_id).await.unwrap().unwrap();

        println!("{}", serde_json::to_string_pretty(&details).unwrap());
    }
}
pub fn list_versions(apps: Vec<(String, Option<String>)>) {
    for app in apps {
        let (app_id, _) = app;
        println!("Versions available for {} on Google Play:", app_id);
        println!("| Google Play does not make old versions of apps available.");
    }
}
