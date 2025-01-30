use std::collections::HashSet;
use std::ops::Deref;
use std::path::Path;
use std::rc::Rc;

use futures_util::StreamExt;
use indicatif::MultiProgress;
use regex::Regex;
use reqwest::header::{HeaderMap, HeaderValue};
use reqwest::{Url, Response};
use tokio_dl_stream_to_disk::{AsyncDownload, error::ErrorKind as TDSTDErrorKind};
use tokio::time::{sleep, Duration as TokioDuration};

use crate::progress_bar::progress_wrapper;

fn http_headers() -> HeaderMap {
    let mut headers = HeaderMap::new();
    headers.insert("x-cv", HeaderValue::from_static("3172501"));
    headers.insert("x-sv", HeaderValue::from_static("29"));
    headers.insert(
        "x-abis",
        HeaderValue::from_static("arm64-v8a,armeabi-v7a,armeabi"),
    );
    headers.insert("x-gp", HeaderValue::from_static("1"));
    headers
}

pub async fn download_apps(
    apps: Vec<(String, Option<String>)>,
    parallel: usize,
    sleep_duration: u64,
    outpath: &Path,
    xapk_bundle: bool
) {
    let mp = Rc::new(MultiProgress::new());
    let http_client = Rc::new(reqwest::Client::new());
    let headers = http_headers();

    futures_util::stream::iter(
        apps.into_iter().map(|app| {
            let (app_id, app_version) = app;
            let http_client = Rc::clone(&http_client);
            let headers = headers.clone();
            let mp = Rc::clone(&mp);
            let mp_log = Rc::clone(&mp);
            async move {
                let app_string = match app_version {
                    Some(ref version) => {
                        mp_log.println(format!("Downloading {} version {}...", app_id, version)).unwrap();
                        format!("{}@{}", app_id, version)
                    },
                    None => {
                        mp_log.println(format!("Downloading {}...", app_id)).unwrap();
                        app_id.to_string()
                    },
                };
                if sleep_duration > 0 {
                    sleep(TokioDuration::from_millis(sleep_duration)).await;
                }
                let versions_url = Url::parse(&format!("{}{}", crate::consts::APKPURE_VERSIONS_URL_FORMAT, app_id)).unwrap();
                let versions_response = http_client
                    .get(versions_url)
                    .headers(headers)
                    .send().await.unwrap();

                let download_url_regex = match xapk_bundle {
                    true => format!("(XAPKJ)..{}", crate::consts::APKPURE_DOWNLOAD_URL_REGEX),
                    false => format!("[^X](APKJ)..{}", crate::consts::APKPURE_DOWNLOAD_URL_REGEX)
                };

                let regex_string = if let Some(app_version) = app_version {
                    format!("[[:^digit:]]{}:(?s:.)+?{}", regex::escape(&app_version) , download_url_regex) }
                else {
                    format!("[[:^digit:]]:(?s:.)+?{}" , download_url_regex)};

                let re = Regex::new(&regex_string).unwrap();
                download_from_response(versions_response, Box::new(Box::new(re)), app_string, outpath, mp).await;
            }
        })
    ).buffer_unordered(parallel).collect::<Vec<()>>().await;
}

async fn download_from_response(response: Response, re: Box<dyn Deref<Target=Regex>>, app_string: String, outpath: &Path, mp: Rc<MultiProgress>) {
    let mp_log = Rc::clone(&mp);
    let mp = Rc::clone(&mp);
    match response.status() {
        reqwest::StatusCode::OK => {
            let body = response.text().await.unwrap();
            match re.captures(&body) {
                Some(caps) if caps.len() >= 2 => {
                    let apk_xapk = caps.get(1).unwrap().as_str();
                    let download_url = caps.get(2).unwrap().as_str();
                    let fname = match apk_xapk {
                        "XAPKJ" => format!("{}.xapk", app_string),
                        _ => format!("{}.apk", app_string),
                    };

                    match AsyncDownload::new(download_url, Path::new(outpath), &fname).get().await {
                        Ok(mut dl) => {
                            let length = dl.length();
                            let cb = match length {
                                Some(length) => Some(progress_wrapper(mp)(fname.clone(), length)),
                                None => None,
                            };

                            match dl.download(&cb).await {
                                Ok(_) => mp_log.println(format!("{} downloaded successfully!", app_string)).unwrap(),
                                Err(err) if matches!(err.kind(), TDSTDErrorKind::FileExists) => {
                                    mp_log.println(format!("File already exists for {}. Skipping...", app_string)).unwrap();
                                },
                                Err(err) if matches!(err.kind(), TDSTDErrorKind::PermissionDenied) => {
                                    mp_log.println(format!("Permission denied when attempting to write file for {}. Skipping...", app_string)).unwrap();
                                },
                                Err(_) => {
                                    mp_log.println(format!("An error has occurred attempting to download {}.  Retry #1...", app_string)).unwrap();
                                    match AsyncDownload::new(download_url, Path::new(outpath), &fname).download(&cb).await {
                                        Ok(_) => mp_log.println(format!("{} downloaded successfully!", app_string)).unwrap(),
                                        Err(_) => {
                                            mp_log.println(format!("An error has occurred attempting to download {}.  Retry #2...", app_string)).unwrap();
                                            match AsyncDownload::new(download_url, Path::new(outpath), &fname).download(&cb).await {
                                                Ok(_) => mp_log.println(format!("{} downloaded successfully!", app_string)).unwrap(),
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
                },
                _ => {
                    mp_log.println(format!("Could not get download URL for {}. Skipping...", app_string)).unwrap();
                }
            }

        },
        _ => {
            mp_log.println(format!("Invalid app response for {}. Skipping...", app_string)).unwrap();
        }
    }
}

pub async fn list_versions(apps: Vec<(String, Option<String>)>) {
    let http_client = Rc::new(reqwest::Client::new());
    let re = Rc::new(Regex::new(r"([[:alnum:]\.-]+):\([[:xdigit:]]{40,}").unwrap());
    let headers = http_headers();
    for app in apps {
        let (app_id, _) = app;
        let http_client = Rc::clone(&http_client);
        let re = Rc::clone(&re);
        let headers = headers.clone();
        async move {
            println!("Versions available for {} on APKPure:", app_id);
            let versions_url = Url::parse(&format!("{}{}", crate::consts::APKPURE_VERSIONS_URL_FORMAT, app_id)).unwrap();
            let versions_response = http_client
                .get(versions_url)
                .headers(headers)
                .send().await.unwrap();

            match versions_response.status() {
                reqwest::StatusCode::OK => {
                    let body = versions_response.text().await.unwrap();
                    let mut versions = HashSet::new();
                    for caps in re.captures_iter(&body) {
                        if caps.len() >= 2 {
                            versions.insert(caps.get(1).unwrap().as_str().to_string());
                        }
                    }
                    let mut versions = versions.drain().collect::<Vec<String>>();
                    versions.sort();
                    println!("| {}", versions.join(", "));
                }
                _ => {
                    println!("| Invalid app response for {}. Skipping...", app_id);
                }
            }
        }.await;
    }
}
