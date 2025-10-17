use std::cell::RefCell;
use std::collections::{HashMap, HashSet};
use std::error::Error;
use std::path::Path;
use std::rc::Rc;

use futures_util::StreamExt;
use indicatif::MultiProgress;
use reqwest::header::{HeaderMap, HeaderValue};
use reqwest::{Url, Response};
use serde_json::{json, value::Value};
use simple_error::SimpleError;
use tokio_dl_stream_to_disk::{AsyncDownload, error::ErrorKind as TDSTDErrorKind};
use tokio::time::{sleep, Duration as TokioDuration};

use crate::util::{OutputFormat, progress_bar::progress_wrapper};

fn http_headers(options: &HashMap<&str, &str>) -> HeaderMap {
    let mut headers = HeaderMap::new();
    headers.insert(
        "user-agent",
        HeaderValue::from_static("Dalvik/2.1.0 (Linux; U; Android 15; Pixel 4a (5G) Build/BP1A.250505.005); APKPure/3.20.53 (Aegon)")
    );
    headers.insert("ual-access-businessid", HeaderValue::from_static("projecta"));
    let abis = match options.get("arch"){
        Some(arch) => {
            let arch_vec: Vec<&str> = arch.split(";").collect();
            json!(arch_vec).to_string()
        },
        None => "[\"arm64-v8a\",\"armeabi-v7a\",\"armeabi\",\"x86\",\"x86_64\"]".to_string()
    };
    let language = match options.get("language") {
        Some(language) => json!(language).to_string(),
        None => "\"en-US\"".to_string()
    };
    let os_ver = match options.get("os_ver") {
        Some(os_ver) => json!(os_ver).to_string(),
        None => "\"35\"".to_string()
    };
    let device_info = format!("{{\"device_info\":{{\"abis\":{},\"language\":{},\"os_ver\":{}}}", abis, language, os_ver);
    match HeaderValue::from_str(&device_info) {
        Ok(device_info_header) => {
            headers.insert(
                "ual-access-projecta",
                device_info_header
            );
        },
        Err(_) => {
            println!("Invalid options specified, excluding device arch specification.");
        }
    }
    headers
}

pub async fn download_apps(
    apps: Vec<(String, Option<String>)>,
    parallel: usize,
    sleep_duration: u64,
    outpath: &Path,
    options: HashMap<&str, &str>,
) {
    let mp = Rc::new(MultiProgress::new());
    let http_client = Rc::new(reqwest::Client::new());
    let app_arch = options.get("arch").cloned();
    let headers = http_headers(&options);

    futures_util::stream::iter(
        apps.into_iter().map(|app| {
            let (app_id, app_version) = app;
            let http_client = Rc::clone(&http_client);
            let headers = headers.clone();
            let mp = Rc::clone(&mp);
            let mp_log = Rc::clone(&mp);
            async move {
                let app_string = match (&app_version, app_arch) {
                    (None, None) => {
                        mp_log.suspend(|| println!("Downloading {}...", app_id));
                        app_id.to_string()
                    },
                    (None, Some(ref arch)) => {
                        mp_log.suspend(|| println!("Downloading {} arch {}...", app_id, arch));
                        format!("{}@{}", app_id, arch)
                    },
                    (Some(ref version), None) => {
                        mp_log.suspend(|| println!("Downloading {} version {}...", app_id, version));
                        format!("{}@{}", app_id, version)
                    },
                    (Some(ref version), Some(ref arch)) => {
                        mp_log.suspend(|| println!("Downloading {} version {} arch {}...", app_id, version, arch));
                        format!("{}@{}@{}", app_id, version, arch)
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
                if let Err(err) = download_from_response(versions_response, app_string, app_version, outpath, mp).await {
                    mp_log.println(format!("{}", err)).unwrap();
                }
            }
        })
    ).buffer_unordered(parallel).collect::<Vec<()>>().await;
}

async fn download_from_response(response: Response, app_string: String, app_version: Option<String>, outpath: &Path, mp: Rc<MultiProgress>) -> Result<(), Box<dyn Error>> {
    let mp_log = Rc::clone(&mp);
    let mp = Rc::clone(&mp);
    match response.status() {
        reqwest::StatusCode::OK => {
            let body_json = response.text().await?;
            let mut download_url = String::new();
            let mut fname = String::new();
            match serde_json::from_str::<Value>(&body_json){
                Ok(body) => {
                    if let Some(Value::Array(version_list)) = body.get("version_list"){
                        for version in version_list {
                            let app_version = app_version.clone();
                            if app_version.is_some() && app_version != version.get("version_name").map(|v| v.as_str().unwrap().to_string()) {
                                continue
                            }
                            if let Some(Value::Object(asset)) = version.get("asset"){
                                if let (Some(Value::String(url)), Some(Value::String(apk_type))) = (asset.get("url"), asset.get("type")) {
                                    download_url = url.to_string();
                                    if apk_type == "XAPK" {
                                        fname = format!("{}.xapk", app_string);
                                    } else {
                                        fname = format!("{}.apk", app_string);
                                    }
                                    break
                                }
                            }
                        }
                    }
                },
                Err(_) => {
                    return Err(Box::new(SimpleError::new(format!("Invalid app JSON response for {}. Skipping...", app_string))));
                }
            }
            if download_url.is_empty(){
                return Err(Box::new(SimpleError::new(format!("No valid versions for {}. Skipping...", app_string))));
            }
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
                    Ok(())
                },
                Err(_) => {
                    Err(Box::new(SimpleError::new(format!("Invalid response for {}. Skipping...", app_string))))
                }
            }
        },
        _ => {
            return Err(Box::new(SimpleError::new(format!("Invalid app response for {}. Skipping...", app_string))));
        }
    }
}

pub async fn list_versions(apps: Vec<(String, Option<String>)>, options: HashMap<&str, &str>) {
    let http_client = Rc::new(reqwest::Client::new());
    let headers = http_headers(&options);
    let output_format = match options.get("output_format") {
        Some(val) if val.to_lowercase() == "json" => OutputFormat::Json,
        _ => OutputFormat::Plaintext,
    };
    let json_root = Rc::new(RefCell::new(match output_format {
        OutputFormat::Json => Some(HashMap::new()),
        _ => None,
    }));

    for app in apps {
        let (app_id, _) = app;
        let http_client = Rc::clone(&http_client);
        let json_root = Rc::clone(&json_root);
        let output_format = output_format.clone();
        let headers = headers.clone();
        async move {
            if output_format.is_plaintext() {
                println!("Versions available for {} on APKPure:", app_id);
            }
            let versions_url = Url::parse(&format!("{}{}", crate::consts::APKPURE_VERSIONS_URL_FORMAT, app_id)).unwrap();
            let versions_response = http_client
                .get(versions_url)
                .headers(headers)
                .send().await.unwrap();

            match versions_response.status() {
                reqwest::StatusCode::OK => {
                    let body_json = versions_response.text().await.unwrap();

                    match serde_json::from_str::<Value>(&body_json){
                        Ok(body) => {
                            let mut versions = HashSet::new();
                            if let Some(Value::Array(version_list)) = body.get("version_list"){
                                for version in version_list {
                                    if let Some(Value::String(version_name)) = version.get("version_name") {
                                        versions.insert(version_name.to_string());
                                    }
                                }
                            }
                            let mut versions = versions.drain().collect::<Vec<String>>();
                            versions.sort();
                            versions.reverse();
                            match output_format {
                                OutputFormat::Plaintext => {
                                    println!("| {}", versions.join(", "));
                                },
                                OutputFormat::Json => {
                                    let mut app_root: HashMap<String, Vec<HashMap<String, String>>> = HashMap::new();
                                    app_root.insert("available_versions".to_string(), versions.into_iter().map(|v| {
                                        let mut version_map = HashMap::new();
                                        version_map.insert("version".to_string(), v);
                                        version_map
                                    }).collect());
                                    json_root.borrow_mut().as_mut().unwrap().insert(app_id.to_string(), json!(app_root));
                                },
                            }
                        },
                        Err(_) => {
                            match output_format {
                                OutputFormat::Plaintext => {
                                    eprintln!("| Invalid app JSON response for {}. Skipping...", app_id);
                                },
                                OutputFormat::Json => {
                                    let mut app_root = HashMap::new();
                                    app_root.insert("error".to_string(), "Invalid app JSON response.".to_string());
                                    json_root.borrow_mut().as_mut().unwrap().insert(app_id.to_string(), json!(app_root));
                                }
                            }
                        }
                    }
                }
                _ => {
                    match output_format {
                        OutputFormat::Plaintext => {
                            eprintln!("| Invalid app response for {}. Skipping...", app_id);
                        },
                        OutputFormat::Json => {
                            let mut app_root = HashMap::new();
                            app_root.insert("error".to_string(), "Invalid app response.".to_string());
                            json_root.borrow_mut().as_mut().unwrap().insert(app_id.to_string(), json!(app_root));
                        },
                    }
                }
            }
        }.await;
    }
    if output_format.is_json() {
        println!("{{\"source\":\"APKPure\",\"apps\":{}}}", json!(*json_root));
    };
}
