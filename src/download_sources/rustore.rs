use std::path::Path;
use std::rc::Rc;

use futures_util::StreamExt;
use indicatif::MultiProgress;
use reqwest::header::{HeaderMap, HeaderValue};
use serde_json::{Value, json};
use tokio_dl_stream_to_disk::{AsyncDownload, error::ErrorKind as TDSTDErrorKind};
use tokio::time::{sleep, Duration as TokioDuration};

use crate::util::progress_bar::progress_wrapper;

fn http_headers() -> HeaderMap {
    let mut headers = HeaderMap::new();
    headers.insert("User-Agent", HeaderValue::from_static("RuStore/1.78.0.1 (Android 11; SDK 30; arm64-v8a; samsung SM-N935F; en)"));
    headers.insert("Content-Type", HeaderValue::from_static("application/json"));
    headers
}

fn download_request_body(app_id: u64) -> String {
    serde_json::to_string(&json!({
        "appId": app_id,
        "firstInstall": true,
        "mobileServices": ["GMS", "HMS"],
        "supportedAbis": ["arm64-v8a"],
        "screenDensity": 420,
        "supportedLocales": ["en_US"],
        "sdkVersion": 30,
        "withoutSplits": true,
        "signatureFingerprint": null
    })).unwrap()
}

async fn get_app_id(http_client: &reqwest::Client, headers: &HeaderMap, package_name: &str) -> Option<u64> {
    let lookup_url = format!("https://backapi.rustore.ru/applicationData/store-app?packageNames={}", package_name);
    
    match http_client.get(&lookup_url).headers(headers.clone()).send().await {
        Ok(response) if response.status().is_success() => {
            match response.text().await {
                Ok(body) => {
                    match serde_json::from_str::<Value>(&body) {
                        Ok(json_response) => {
                            if let Some(array) = json_response.as_array() {
                                if let Some(first_app) = array.first() {
                                    if let Some(id) = first_app.get("id") {
                                        return id.as_u64();
                                    }
                                }
                            }
                            None
                        }
                        Err(_) => None
                    }
                }
                Err(_) => None
            }
        }
        _ => None
    }
}

async fn get_download_url(http_client: &reqwest::Client, headers: &HeaderMap, app_id: u64) -> Option<String> {
    let download_url = "https://backapi.rustore.ru/applicationData/v2/download-link";
    
    match http_client
        .post(download_url)
        .headers(headers.clone())
        .body(download_request_body(app_id))
        .send().await 
    {
        Ok(response) if response.status().is_success() => {
            match response.text().await {
                Ok(body) => {
                    match serde_json::from_str::<Value>(&body) {
                        Ok(json_response) => {
                            if let Some(code) = json_response.get("code") {
                                if code.as_str() == Some("OK") {
                                    if let Some(body) = json_response.get("body") {
                                        if let Some(download_urls) = body.get("downloadUrls") {
                                            if let Some(array) = download_urls.as_array() {
                                                if let Some(first_url) = array.first() {
                                                    if let Some(url) = first_url.get("url") {
                                                        return url.as_str().map(|s| s.to_string());
                                                    }
                                                }
                                            }
                                        }
                                    }
                                }
                            }
                            None
                        }
                        Err(_) => None
                    }
                }
                Err(_) => None
            }
        }
        _ => None
    }
}

pub async fn download_apps(
    apps: Vec<(String, Option<String>)>,
    parallel: usize,
    sleep_duration: u64,
    outpath: &Path,
) {
    let http_client = Rc::new(reqwest::Client::new());
    let headers = http_headers();

    let mp = Rc::new(MultiProgress::new());
    futures_util::stream::iter(
        apps.into_iter().map(|app| {
            let (app_id, app_version) = app;
            let http_client = Rc::clone(&http_client);
            let headers = headers.clone();
            let mp = Rc::clone(&mp);
            let mp_log = Rc::clone(&mp);
            async move {
                if app_version.is_none() {
                    mp_log.suspend(|| println!("Downloading {}...", app_id));
                    if sleep_duration > 0 {
                        sleep(TokioDuration::from_millis(sleep_duration)).await;
                    }

                    match get_app_id(&http_client, &headers, &app_id).await {
                        Some(rustore_app_id) => {
                            match get_download_url(&http_client, &headers, rustore_app_id).await {
                                Some(download_url) => {
                                    download_apk(download_url, app_id.to_string(), outpath, mp).await;
                                }
                                None => {
                                    mp_log.println(format!("Could not get download URL for {}. Skipping...", app_id)).unwrap();
                                }
                            }
                        }
                        None => {
                            mp_log.println(format!("App not found on RuStore: {}. Skipping...", app_id)).unwrap();
                        }
                    }
                } else {
                    mp_log.println(format!("Specific versions can not be downloaded from RuStore ({}@{}). Skipping...", app_id, app_version.unwrap())).unwrap();
                }
            }
        })
    ).buffer_unordered(parallel).collect::<Vec<()>>().await;
}

async fn download_apk(download_url: String, app_string: String, outpath: &Path, mp: Rc<MultiProgress>) {
    let mp_log = Rc::clone(&mp);
    let mp = Rc::clone(&mp);
    let fname = format!("{}.apk", app_string);
    
    match AsyncDownload::new(&download_url, Path::new(outpath), &fname).get().await {
        Ok(mut dl) => {
            let length = dl.length();
            let cb = match length {
                Some(length) => Some(progress_wrapper(mp)(fname.clone(), length)),
                None => None,
            };

            match dl.download(&cb).await {
                Ok(_) => mp_log.suspend(|| println!("{} downloaded successfully!", app_string)),
                Err(err) if matches!(err.kind(), TDSTDErrorKind::FileExists) => {
                    mp_log.println(format!("File already exists for {}. Skipping...", app_string)).unwrap();
                },
                Err(err) if matches!(err.kind(), TDSTDErrorKind::PermissionDenied) => {
                    mp_log.println(format!("Permission denied when attempting to write file for {}. Skipping...", app_string)).unwrap();
                },
                Err(_) => {
                    mp_log.println(format!("An error has occurred attempting to download {}.  Retry #1...", app_string)).unwrap();
                    match AsyncDownload::new(&download_url, Path::new(outpath), &fname).download(&cb).await {
                        Ok(_) => mp_log.suspend(|| println!("{} downloaded successfully!", app_string)),
                        Err(_) => {
                            mp_log.println(format!("An error has occurred attempting to download {}.  Retry #2...", app_string)).unwrap();
                            match AsyncDownload::new(&download_url, Path::new(outpath), &fname).download(&cb).await {
                                Ok(_) => mp_log.suspend(|| println!("{} downloaded successfully!", app_string)),
                                Err(_) => {
                                    mp_log.println(format!("An error has occurred attempting to download {}. Skipping...", app_string)).unwrap();
                                }
                            }
                        }
                    }
                }
            }
        },
        Err(_) => {
            mp_log.println(format!("Invalid response for {}. Skipping...", app_string)).unwrap();
        }
    }
}

pub async fn list_versions(apps: Vec<(String, Option<String>)>) {
    for app in apps {
        let (app_id, _) = app;
        println!("Versions available for {} on RuStore:", app_id);
        println!("| RuStore does not make old versions of apps available.");
    }
}