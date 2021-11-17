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
//! This downloads from the default source, `APKPure`, which does not require credentials.  To
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

use std::error::Error;
use std::fs;
use std::path::{Path, PathBuf};
use std::rc::Rc;

use futures_util::StreamExt;
use gpapi::error::ErrorKind as GpapiErrorKind;
use gpapi::Gpapi;
use regex::Regex;
use reqwest::header::{HeaderMap, HeaderValue};
use reqwest::Url;
use tokio::time::{sleep, Duration as TokioDuration};
use tokio_dl_stream_to_disk::error::ErrorKind as TDSTDErrorKind;

mod cli;
use cli::DownloadSource;

mod consts;
mod fdroid;

fn fetch_csv_list(csv: &str, field: usize) -> Result<Vec<String>, Box<dyn Error>> {
    Ok(parse_csv_text(fs::read_to_string(csv)?, field))
}

fn parse_csv_text(text: String, field: usize) -> Vec<String> {
    let field = field - 1;
    text.split("\n")
        .filter_map(|l| {
            let entry = l.trim();
            let mut entry_vec = entry.split(",").collect::<Vec<&str>>();
            if entry_vec.len() > field && !(entry_vec.len() == 1 && entry_vec[0].len() == 0) {
                Some(String::from(entry_vec.remove(field)))
            } else {
                None
            }
        })
        .collect()
}

async fn download_apps_from_google_play(
    app_ids: Vec<String>,
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
        app_ids.into_iter().map(|app_id| {
            let gpa = Rc::clone(&gpa);
            async move {
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
            }
        })
    ).buffer_unordered(parallel).collect::<Vec<()>>().await;
}

async fn download_apps_from_apkpure(
    app_ids: Vec<String>,
    parallel: usize,
    sleep_duration: u64,
    outpath: &PathBuf,
) {
    let http_client = Rc::new(reqwest::Client::new());
    let mut headers = HeaderMap::new();
    headers.insert("x-cv", HeaderValue::from_static("3172501"));
    headers.insert("x-sv", HeaderValue::from_static("29"));
    headers.insert(
        "x-abis",
        HeaderValue::from_static("arm64-v8a,armeabi-v7a,armeabi"),
    );
    headers.insert("x-gp", HeaderValue::from_static("1"));
    let re = Rc::new(Regex::new(consts::APKPURE_DOWNLOAD_URL_REGEX).unwrap());

    futures_util::stream::iter(
        app_ids.into_iter().map(|app_id| {
            let http_client = Rc::clone(&http_client);
            let re = Rc::clone(&re);
            let headers = headers.clone();
            async move {
                println!("Downloading {}...", app_id);
                if sleep_duration > 0 {
                    sleep(TokioDuration::from_millis(sleep_duration)).await;
                }
                let detail_url = Url::parse(&format!("{}{}", consts::APKPURE_DETAILS_URL_FORMAT, app_id)).unwrap();
                let detail_response = http_client
                    .get(detail_url)
                    .headers(headers)
                    .send().await.unwrap();
                match detail_response.status() {
                    reqwest::StatusCode::OK => {
                        let body = detail_response.text().await.unwrap();
                        match re.captures(&body) {
                            Some(caps) if caps.len() >= 2 => {
                                let download_url = caps.get(1).unwrap().as_str();
                                let fname = format!("{}.apk", app_id);
                                match tokio_dl_stream_to_disk::download(download_url, &Path::new(outpath), &fname).await {
                                    Ok(_) => println!("{} downloaded successfully!", app_id),
                                    Err(err) if matches!(err.kind(), TDSTDErrorKind::FileExists) => {
                                        println!("File already exists for {}. Skipping...", app_id);
                                    },
                                    Err(err) if matches!(err.kind(), TDSTDErrorKind::PermissionDenied) => {
                                        println!("Permission denied when attempting to write file for {}. Skipping...", app_id);
                                    },
                                    Err(_) => {
                                        println!("An error has occurred attempting to download {}.  Retry #1...", app_id);
                                        match tokio_dl_stream_to_disk::download(download_url, &Path::new(outpath), &fname).await {
                                            Ok(_) => println!("{} downloaded successfully!", app_id),
                                            Err(_) => {
                                                println!("An error has occurred attempting to download {}.  Retry #2...", app_id);
                                                match tokio_dl_stream_to_disk::download(download_url, &Path::new(outpath), &fname).await {
                                                    Ok(_) => println!("{} downloaded successfully!", app_id),
                                                    Err(_) => {
                                                        println!("An error has occurred attempting to download {}. Skipping...", app_id);
                                                    }
                                                }
                                            }
                                        }
                                    }
                                }
                            },
                            _ => {
                                println!("Could not get download URL for {}. Skipping...", app_id);
                            }
                        }
                    },
                    _ => {
                        println!("Invalid app response for {}. Skipping...", app_id);
                    }
                }
            }
        })
    ).buffer_unordered(parallel).collect::<Vec<()>>().await;
}

#[tokio::main]
async fn main() {
    let matches = cli::app().get_matches();

    let download_source = value_t!(matches.value_of("download_source"), DownloadSource).unwrap();
    let parallel = value_t!(matches, "parallel", usize).unwrap();
    let sleep_duration = value_t!(matches, "sleep_duration", u64).unwrap();
    let outpath = fs::canonicalize(matches.value_of("OUTPATH").unwrap()).unwrap();
    if !Path::new(&outpath).is_dir() {
        println!("{}\n\nOUTPATH is not a valid directory", matches.usage());
        std::process::exit(1);
    };
    let list = match matches.value_of("app_id") {
        Some(app_id) => vec![app_id.to_string()],
        None => {
            let csv = matches.value_of("csv").unwrap();
            let field = value_t!(matches, "field", usize).unwrap();
            if field < 1 {
                println!("{}\n\nField must be 1 or greater", matches.usage());
                std::process::exit(1);
            }
            match fetch_csv_list(csv, field) {
                Ok(csv_list) => csv_list,
                Err(err) => {
                    println!("{}\n\n{:?}", matches.usage(), err);
                    std::process::exit(1);
                }
            }
        }
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
