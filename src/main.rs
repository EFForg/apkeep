//! # Installation
//!
//! Precompiled binaries for `apkeep` on various platforms can be downloaded
//! [here](https://github.com/EFForg/apkeep/releases).
//!
//! To install from `crates.io`, simply [install rust](https://www.rust-lang.org/tools/install) and
//! run
//!
//! ```shell
//! cargo install apkeep
//! ```
//!
//! Or to install from the latest commit in our repository, run
//!
//! ```shell
//! cargo install --git https://github.com/EFForg/apkeep.git
//! ```
//!
//! # Usage
//!
//! See [`USAGE`](https://github.com/EFForg/apkeep/blob/master/USAGE).
//!
//! # Examples
//!
//! The simplest example is to download a single APK to the current directory:
//!
//! ```shell
//! apkeep -a com.instagram.android .
//! ```
//!
//! This downloads from the default source, APKPure, which does not require credentials.  To
//! download directly from the google play store:
//!
//! ```shell
//! apkeep -a com.instagram.android -d GooglePlay -u 'someone@gmail.com' -p somepass .
//! ```
//!
//! Or, to download from the F-Droid open source repository:
//!
//! ```shell
//! apkeep -a org.mozilla.fennec_fdroid -d FDroid .
//! ```
//!
//! To download a specific version of an APK (possible for APKPure or F-Droid), use the `@version` convention:
//!
//! ```shell
//! apkeep -a com.instagram.android@1.2.3 .
//! ```
//!
//! Or, to list what versions are available, use `-l`:
//!
//! ```shell
//! apkeep -l -a org.mozilla.fennec_fdroid -d FDroid
//! ```
//!
//! Refer to [`USAGE`](https://github.com/EFForg/apkeep/blob/master/USAGE) to download multiple
//! APKs in a single run.
//!
//! # Specify a CSV file or individual app ID
//!
//! You can either specify a CSV file which lists the apps to download, or an individual app ID.
//! If you specify a CSV file and the app ID is not specified by the first column, you'll have to
//! use the --field option as well.  If you have a simple file with one app ID per line, you can
//! just treat it as a CSV with a single field.
//!
//! # Download Sources
//!
//! You can use this tool to download from a few distinct sources.
//!
//! * The Google Play Store, given a username and password
//! * APKPure, a third-party site hosting APKs available on the Play Store
//! * F-Droid, a repository for free and open-source Android apps. `apkeep` verifies that these
//! APKs are signed by the F-Droid maintainers, and alerts the user if an APK was downloaded but
//! could not be verified
//!
//! # Usage Note
//!
//! Users should not use app lists or choose so many parallel APK fetches as to place unreasonable
//! or disproportionately large load on the infrastructure of the app distributor.
//!
//! When using with the Google Play Store as the download source, a few considerations should be
//! made:
//!
//! * Google may terminate your Google account based on Terms of Service violations.  Read their
//! [Terms of Service](https://play.google.com/about/play-terms/index.html), avoid violating it,
//! and choose an account where this outcome is acceptable.
//! * The session works with a specific "device profile," so only APKs available for that device,
//! location, language, etc. will be available.  In time we hope to make this profile configurable.
//! * Paid and DRM apps will not be available.
//! * Using Tor will make it a lot more likely that the download will fail.

#[macro_use]
extern crate clap;

use std::collections::HashSet;
use std::error::Error;
use std::fs;
use std::ops::Deref;
use std::path::{Path, PathBuf};
use std::rc::Rc;

use futures_util::StreamExt;
use gpapi::error::ErrorKind as GpapiErrorKind;
use gpapi::Gpapi;
use regex::Regex;
use reqwest::header::{HeaderMap, HeaderValue};
use reqwest::{Url, Response};
use tokio::time::{sleep, Duration as TokioDuration};
use tokio_dl_stream_to_disk::error::ErrorKind as TDSTDErrorKind;

mod cli;
use cli::DownloadSource;

mod consts;
mod fdroid;

fn fetch_csv_list(csv: &str, field: usize, version_field: Option<usize>) -> Result<Vec<(String, Option<String>)>, Box<dyn Error>> {
    Ok(parse_csv_text(fs::read_to_string(csv)?, field, version_field))
}

fn parse_csv_text(text: String, field: usize, version_field: Option<usize>) -> Vec<(String, Option<String>)> {
    let field = field - 1;
    let version_field = match version_field {
        None => None,
        Some(version_field) => Some(version_field - 1),
    };
    text.split("\n")
        .filter_map(|l| {
            let entry = l.trim();
            let mut entry_vec = entry.split(",").collect::<Vec<&str>>();
            if entry_vec.len() > field && !(entry_vec.len() == 1 && entry_vec[0].len() == 0) {
                match version_field {
                    Some(mut version_field) if entry_vec.len() > version_field => {
                        if version_field > field {
                            version_field = version_field - 1;
                        }
                        let app_id = String::from(entry_vec.remove(field));
                        let app_version = String::from(entry_vec.remove(version_field));
                        if app_version.len() > 0 {
                            Some((app_id, Some(app_version)))
                        } else {
                            Some((app_id, None))
                        }
                    },
                    _ => Some((String::from(entry_vec.remove(field)), None)),
                }
            } else {
                None
            }
        })
        .collect()
}

async fn download_apps_from_google_play(
    apps: Vec<(String, Option<String>)>,
    parallel: usize,
    sleep_duration: u64,
    username: &str,
    password: &str,
    outpath: &PathBuf,
) {
    let mut gpa = Gpapi::new("en_US", "UTC", "hero2lte");
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
                    match gpa.download(&app_id, None, &Path::new(outpath)).await {
                        Ok(_) => println!("{} downloaded successfully!", app_id),
                        Err(err) if matches!(err.kind(), GpapiErrorKind::FileExists) => {
                            println!("File already exists for {}. Skipping...", app_id);
                        }
                        Err(err) if matches!(err.kind(), GpapiErrorKind::InvalidApp) => {
                            println!("Invalid app response for {}. Skipping...", app_id);
                        }
                        Err(err) if matches!(err.kind(), GpapiErrorKind::PermissionDenied) => {
                            println!("Permission denied when attempting to write file for {}. Skipping...", app_id);
                        }
                        Err(_) => {
                            println!("An error has occurred attempting to download {}.  Retry #1...", app_id);
                            match gpa.download(&app_id, None, &Path::new(outpath)).await {
                                Ok(_) => println!("{} downloaded successfully!", app_id),
                                Err(_) => {
                                    println!("An error has occurred attempting to download {}.  Retry #2...", app_id);
                                    match gpa.download(&app_id, None, &Path::new(outpath)).await {
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

fn list_versions_from_google_play(apps: Vec<(String, Option<String>)>) {
    for app in apps {
        let (app_id, _) = app;
        println!("Versions available for {} on Google Play:", app_id);
        println!("| Google Play does not make old versions of apps available.");
    }
}

fn apkpure_http_headers() -> HeaderMap {
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
async fn download_apps_from_apkpure(
    apps: Vec<(String, Option<String>)>,
    parallel: usize,
    sleep_duration: u64,
    outpath: &PathBuf,
) {
    let http_client = Rc::new(reqwest::Client::new());
    let headers = apkpure_http_headers();
    let re = Rc::new(Regex::new(consts::APKPURE_DOWNLOAD_URL_REGEX).unwrap());

    futures_util::stream::iter(
        apps.into_iter().map(|app| {
            let (app_id, app_version) = app;
            let http_client = Rc::clone(&http_client);
            let re = Rc::clone(&re);
            let headers = headers.clone();
            async move {
                let app_string = match app_version {
                    Some(ref version) => {
                        println!("Downloading {} version {}...", app_id, version);
                        format!("{}@{}", app_id, version)
                    },
                    None => {
                        println!("Downloading {}...", app_id);
                        format!("{}", app_id)
                    },
                };
                if sleep_duration > 0 {
                    sleep(TokioDuration::from_millis(sleep_duration)).await;
                }
                if app_version.is_none() {
                    let detail_url = Url::parse(&format!("{}{}", consts::APKPURE_DETAILS_URL_FORMAT, app_id)).unwrap();
                    let detail_response = http_client
                        .get(detail_url)
                        .headers(headers)
                        .send().await.unwrap();
                    download_from_response(detail_response, Box::new(re), app_string, &outpath).await;
                } else {
                    let versions_url = Url::parse(&format!("{}{}", consts::APKPURE_VERSIONS_URL_FORMAT, app_id)).unwrap();
                    let versions_response = http_client
                        .get(versions_url)
                        .headers(headers)
                        .send().await.unwrap();
                    let app_version = app_version.unwrap();
                    let regex_string = format!("[[:^digit:]]{}:(?s:.)+?{}", regex::escape(&app_version), consts::APKPURE_DOWNLOAD_URL_REGEX);
                    let re = Regex::new(&regex_string).unwrap();
                    download_from_response(versions_response, Box::new(Box::new(re)), app_string, &outpath).await;
                }
            }
        })
    ).buffer_unordered(parallel).collect::<Vec<()>>().await;
}

async fn download_from_response(response: Response, re: Box<dyn Deref<Target=Regex>>, app_string: String, outpath: &PathBuf) {
    let fname = format!("{}.apk", app_string);
    match response.status() {
        reqwest::StatusCode::OK => {
            let body = response.text().await.unwrap();
            match re.captures(&body) {
                Some(caps) if caps.len() >= 2 => {
                    let download_url = caps.get(1).unwrap().as_str();
                    match tokio_dl_stream_to_disk::download(download_url, &Path::new(outpath), &fname).await {
                        Ok(_) => println!("{} downloaded successfully!", app_string),
                        Err(err) if matches!(err.kind(), TDSTDErrorKind::FileExists) => {
                            println!("File already exists for {}. Skipping...", app_string);
                        },
                        Err(err) if matches!(err.kind(), TDSTDErrorKind::PermissionDenied) => {
                            println!("Permission denied when attempting to write file for {}. Skipping...", app_string);
                        },
                        Err(_) => {
                            println!("An error has occurred attempting to download {}.  Retry #1...", app_string);
                            match tokio_dl_stream_to_disk::download(download_url, &Path::new(outpath), &fname).await {
                                Ok(_) => println!("{} downloaded successfully!", app_string),
                                Err(_) => {
                                    println!("An error has occurred attempting to download {}.  Retry #2...", app_string);
                                    match tokio_dl_stream_to_disk::download(download_url, &Path::new(outpath), &fname).await {
                                        Ok(_) => println!("{} downloaded successfully!", app_string),
                                        Err(_) => {
                                            println!("An error has occurred attempting to download {}. Skipping...", app_string);
                                        }
                                    }
                                }
                            }
                        }
                    }
                },
                _ => {
                    println!("Could not get download URL for {}. Skipping...", app_string);
                }
            }

        },
        _ => {
            println!("Invalid app response for {}. Skipping...", app_string);
        }
    }
}

async fn list_versions_from_apkpure(apps: Vec<(String, Option<String>)>) {
    let http_client = Rc::new(reqwest::Client::new());
    let re = Rc::new(Regex::new(r"([[:alnum:]\.-]+):\([[:xdigit:]]{40,}").unwrap());
    let headers = apkpure_http_headers();
    for app in apps {
        let (app_id, _) = app;
        let http_client = Rc::clone(&http_client);
        let re = Rc::clone(&re);
        let headers = headers.clone();
        async move {
            println!("Versions available for {} on APKPure:", app_id);
            let versions_url = Url::parse(&format!("{}{}", consts::APKPURE_VERSIONS_URL_FORMAT, app_id)).unwrap();
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


#[tokio::main]
async fn main() {
    let matches = cli::app().get_matches();

    let download_source = value_t!(matches.value_of("download_source"), DownloadSource).unwrap();
    let list = match matches.value_of("app") {
        Some(app) => {
            let mut app_vec: Vec<String> = app.splitn(2, "@").map(|s| String::from(s)).collect();
            let app_id = app_vec.remove(0);
            let app_version = match app_vec.len() {
                1 => Some(app_vec.remove(0)),
                _ => None,
            };
            vec![(app_id, app_version)]
        },
        None => {
            let csv = matches.value_of("csv").unwrap();
            let field = value_t!(matches, "field", usize).unwrap();
            let version_field: Option<usize> = value_t!(matches, "version_field", usize).ok();
            if field < 1 {
                println!("{}\n\nApp ID field must be 1 or greater", matches.usage());
                std::process::exit(1);
            }
            if let Some(version_field) = version_field {
                if version_field < 1 {
                    println!("{}\n\nVersion field must be 1 or greater", matches.usage());
                    std::process::exit(1);
                }
                if field == version_field {
                    println!("{}\n\nApp ID and Version fields must be different", matches.usage());
                    std::process::exit(1);
                }
            }
            match fetch_csv_list(csv, field, version_field) {
                Ok(csv_list) => csv_list,
                Err(err) => {
                    println!("{}\n\n{:?}", matches.usage(), err);
                    std::process::exit(1);
                }
            }
        }
    };

    if matches.is_present("list_versions") {
        match download_source {
            DownloadSource::APKPure => {
                list_versions_from_apkpure(list).await;
            }
            DownloadSource::GooglePlay => {
                list_versions_from_google_play(list);
            }
            DownloadSource::FDroid => {
                fdroid::list_versions(list).await;
            }
        }
    } else {
        let parallel = value_t!(matches, "parallel", usize).unwrap();
        let sleep_duration = value_t!(matches, "sleep_duration", u64).unwrap();
        let outpath = matches.value_of("OUTPATH");
        if outpath.is_none() {
            println!("{}\n\nOUTPATH must be specified when downloading files", matches.usage());
            std::process::exit(1);
        }
        let outpath = fs::canonicalize(outpath.unwrap()).unwrap();
        if !Path::new(&outpath).is_dir() {
            println!("{}\n\nOUTPATH is not a valid directory", matches.usage());
            std::process::exit(1);
        };

        match download_source {
            DownloadSource::APKPure => {
                download_apps_from_apkpure(list, parallel, sleep_duration, &outpath).await;
            }
            DownloadSource::GooglePlay => {
                let username = matches.value_of("google_username").unwrap();
                let password = matches.value_of("google_password").unwrap();
                download_apps_from_google_play(
                    list,
                    parallel,
                    sleep_duration,
                    username,
                    password,
                    &outpath,
                )
                .await;
            }
            DownloadSource::FDroid => {
                fdroid::download_apps(list, parallel, sleep_duration, &outpath).await;
            }
        }
    }
}
