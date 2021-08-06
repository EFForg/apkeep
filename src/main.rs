//! # Usage
//!
//! See [`USAGE`](https://github.com/EFForg/apk-downloader/blob/master/USAGE).
//!
//! # Usage Note
//!
//! Users should not use app lists or choose so many parallel APK fetches as to place unreasonable
//! or disproportionately large load on the infrastructure of the app distributor.
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
//! * The Google Play Store, given a username and password.
//! * APKPure, a third-party site hosting APKs available on the Play Store.  You must be running
//! an instance of the ChromeDriver for this to work, since a headless browser is used.

#[macro_use]
extern crate clap;

use futures_util::StreamExt;
use gpapi::error::{Error as GpapiError, ErrorKind};
use gpapi::Gpapi;
use regex::Regex;
use serde_json::json;
use std::error::Error;
use std::fs;
use std::path::Path;
use std::rc::Rc;
use std::time::Duration;
use thirtyfour::prelude::*;
use tokio::time::{sleep, Duration as TokioDuration};

mod cli;
use cli::DownloadSource;

fn fetch_csv_list(csv: &str, field: usize) -> Result<Vec<String>, Box<dyn Error>> {
    Ok(parse_csv_text(fs::read_to_string(csv)?, field))
}

fn parse_csv_text(text: String, field: usize) -> Vec<String> {
    let field = field - 1;
    text.split("\n").filter_map(|l| {
        let entry = l.trim();
        let mut entry_vec = entry.split(",").collect::<Vec<&str>>();
        if entry_vec.len() > field && !(entry_vec.len() == 1 && entry_vec[0].len() == 0) {
            Some(String::from(entry_vec.remove(field)))
        } else {
            None
        }
    }).collect()
}

async fn download_apps_from_google_play(app_ids: Vec<String>, parallel: usize, sleep_duration: u64, username: &str, password: &str, outpath: &str) {
    let mut gpa = Gpapi::new("en_US", "UTC", "hero2lte");
    if let Err(_) = gpa.login(username, password).await {
        println!("Could not log in to Google Play.  Please check your credentials and try again later.");
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
                    Ok(_) => Ok(()),
                    Err(err) if matches!(err.kind(), ErrorKind::FileExists) => {
                        println!("File already exists for {}.  Aborting.", app_id);
                        Ok(())
                    }
                    Err(err) if matches!(err.kind(), ErrorKind::InvalidApp) => {
                        println!("Invalid app response for {}.  Aborting.", app_id);
                        Err(err)
                    }
                    Err(_) => {
                        println!("An error has occurred attempting to download {}.  Retry #1...", app_id);
                        match gpa.download(&app_id, None, &Path::new(outpath)).await {
                            Ok(_) => Ok(()),
                            Err(_) => {
                                println!("An error has occurred attempting to download {}.  Retry #2...", app_id);
                                match gpa.download(&app_id, None, &Path::new(outpath)).await {
                                    Ok(_) => Ok(()),
                                    Err(err) => {
                                        println!("An error has occurred attempting to download {}.  Aborting.", app_id);
                                        Err(err)
                                    }
                                }
                            }
                        }
                    }
                }
            }
        })
    ).buffer_unordered(parallel).collect::<Vec<Result<(), GpapiError>>>().await;
}

async fn download_apps_from_apkpure(app_ids: Vec<String>, parallel: usize, sleep_duration: u64, outpath: &str) -> WebDriverResult<()> {
    let fetches = futures_util::stream::iter(
        app_ids.into_iter().map(|app_id| {
            async move {
                match download_single_app(&app_id, sleep_duration, outpath).await {
                    Ok(res_tuple) => futures_util::future::ready(Some(res_tuple)),
                    Err(_) => {
                        println!("An error has occurred attempting to download {}.  Retry #1...", app_id);
                        match download_single_app(&app_id, sleep_duration, outpath).await {
                            Ok(res_tuple) => futures_util::future::ready(Some(res_tuple)),
                            Err(_) => {
                                println!("An error has occurred attempting to download {}.  Retry #2...", app_id);
                                match download_single_app(&app_id, sleep_duration, outpath).await {
                                    Ok(res_tuple) => futures_util::future::ready(Some(res_tuple)),
                                    Err(_) => {
                                        println!("An error has occurred attempting to download {}.  Aborting.", app_id);
                                        futures_util::future::ready(None)
                                    }
                                }
                            }
                        }
                    }
                }
            }
        })
    ).buffer_unordered(parallel).filter_map(|i| i).collect::<Vec<(String, String, String)>>();
    println!("Waiting...");
    let results = fetches.await;
    for move_file in results {
        if let Ok(paths) = fs::read_dir(&move_file.0) {
            let dir_list = paths.filter_map(|path| path.ok()).collect::<Vec<fs::DirEntry>>();
            if dir_list.len() > 0 {
                println!("Saving {}...", move_file.2);
                let old_filename = dir_list[0].file_name();
                fs::rename(Path::new(&move_file.0).join(old_filename), Path::new(&move_file.0).join(move_file.1)).unwrap();
            } else {
                println!("Could not save {}...", move_file.2);
            }
        } else {
            println!("Could not save {}...", move_file.2);
        }
    }
    Ok(())
}

async fn download_single_app(app_id: &str, sleep_duration: u64, outpath: &str) -> WebDriverResult<(String, String, String)> {
    println!("Downloading {}...", app_id);
    if sleep_duration > 0 {
        sleep(TokioDuration::from_millis(sleep_duration)).await;
    }
    let app_url = format!("https://apkpure.com/a/{}/download?from=details", app_id);
    let mut caps = DesiredCapabilities::chrome();
    let filepath = format!("{}", Path::new(outpath).join(app_id.clone()).to_str().unwrap());
    let prefs = json!({
        "download.default_directory": filepath
    });
    caps.add_chrome_option("prefs", prefs).unwrap();

    let driver = match WebDriver::new("http://localhost:9515", &caps).await {
        Ok(driver) => driver,
        Err(_) => panic!("chromedriver must be running on port 9515")
    };
    let sleep_duration = Duration::new(10, 0);
    driver.set_implicit_wait_timeout(sleep_duration).await?;
    driver.get(app_url).await?;
    let elem_result = driver.find_element(By::Css("span.file")).await?;
    let re = Regex::new(r" \([0-9.]+ MB\)$").unwrap();

    let new_filename = elem_result.text().await?;
    let new_filename = re.replace(&new_filename, "").into_owned();
    Ok((filepath, new_filename, String::from(app_id)))
}

#[tokio::main]
async fn main() -> WebDriverResult<()> {
    let matches = cli::app().get_matches();

    let download_source = value_t!(matches.value_of("download_source"), DownloadSource).unwrap();
    let parallel = value_t!(matches, "parallel", usize).unwrap();
    let sleep_duration = value_t!(matches, "sleep_duration", u64).unwrap();
    let outpath = matches.value_of("OUTPATH").unwrap();
    if !Path::new(&outpath).is_dir() {
        println!("{}\n\nOUTPATH is not a valid directory", matches.usage());
        std::process::exit(1);
    };
    let list = match matches.value_of("app_name") {
        Some(app_name) => vec![app_name.to_string()],
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
            download_apps_from_apkpure(list, parallel, sleep_duration, outpath).await.unwrap();
        },
        DownloadSource::GooglePlay => {
            let username = matches.value_of("google_username").unwrap();
            let password = matches.value_of("google_password").unwrap();
            download_apps_from_google_play(list, parallel, sleep_duration, username, password, outpath).await;
        },
    }
    Ok(())
}
