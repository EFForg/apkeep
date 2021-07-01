#[macro_use]
extern crate clap;

use clap::{App, Arg};
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

arg_enum! {
    #[derive(Debug)]
    pub enum ListSource {
        AndroidRank,
        ISDi,
    }
}
arg_enum! {
    pub enum DownloadSource {
        APKPure,
        GooglePlay,
    }
}

async fn fetch_list(source: &ListSource) -> Result<Vec<String>, Box<dyn Error>> {
    let resp = match source {
        &ListSource::AndroidRank => reqwest::get("https://www.androidrank.org/applist.csv"),
        &ListSource::ISDi => reqwest::get("https://raw.githubusercontent.com/stopipv/isdi/master/static_data/app-flags.csv"),
    }.await?
     .error_for_status()?
     .text()
     .await?;

    Ok(resp.split("\n").filter_map(|l| {
        let entry = l.trim();
        let mut entry_vec = entry.split(",").collect::<Vec<&str>>();
        if entry_vec.len() > 2 {
            match source {
                &ListSource::AndroidRank => Some(String::from(entry_vec.remove(0))),
                &ListSource::ISDi => {
                    let app_id = entry_vec.remove(0);
                    match entry_vec.remove(0) {
                        "playstore" => Some(String::from(app_id)),
                        _ => None,
                    }
                },
            }
        } else {
            None
        }
    }).collect())
}

async fn download_apps_from_google_play(app_ids: Vec<String>, parallel: usize, username: &str, password: &str, outpath: &str) {
    let mut gpa = Gpapi::new("en_US", "UTC", "hero2lte");
    gpa.login(username, password).await.expect("Could not log in to google play");
    let gpa = Rc::new(gpa);

    futures_util::stream::iter(
        app_ids.into_iter().map(|app_id| {
            let gpa = Rc::clone(&gpa);
            async move {
                println!("Downloading {}...", app_id);
                match gpa.download(&app_id, None, &Path::new(outpath)).await {
                    Ok(_) => Ok(()),
                    Err(err) if matches!(err.kind(), ErrorKind::FileExists) => {
                        println!("File alredy exists for {}.  Aborting.", app_id);
                        Ok(())
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

async fn download_apps_from_apkpure(app_ids: Vec<String>, parallel: usize, outpath: &str) -> WebDriverResult<()> {
    let fetches = futures_util::stream::iter(
        app_ids.into_iter().map(|app_id| {
            async move {
                match download_single_app(&app_id, outpath).await {
                    Ok(res_tuple) => futures_util::future::ready(Some(res_tuple)),
                    Err(_) => {
                        println!("An error has occurred attempting to download {}.  Retry #1...", app_id);
                        match download_single_app(&app_id, outpath).await {
                            Ok(res_tuple) => futures_util::future::ready(Some(res_tuple)),
                            Err(_) => {
                                println!("An error has occurred attempting to download {}.  Retry #2...", app_id);
                                match download_single_app(&app_id, outpath).await {
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

async fn download_single_app(app_id: &str, outpath: &str) -> WebDriverResult<(String, String, String)> {
    println!("Downloading {}...", app_id);
    let app_url = format!("https://apkpure.com/a/{}/download?from=details", app_id);
    let mut caps = DesiredCapabilities::chrome();
    let filepath = format!("{}", Path::new(outpath).join(app_id.clone()).to_str().unwrap());
    let prefs = json!({
        "download.default_directory": filepath
    });
    caps.add_chrome_option("prefs", prefs).unwrap();

    let driver = match WebDriver::new("http://localhost:4444", &caps).await {
        Ok(driver) => driver,
        Err(_) => panic!("chromedriver must be running on port 4444")
    };
    let delay = Duration::new(10, 0);
    driver.set_implicit_wait_timeout(delay).await?;
    driver.get(app_url).await?;
    let elem_result = driver.find_element(By::Css("span.file")).await?;
    let re = Regex::new(r" \([0-9.]+ MB\)$").unwrap();

    let new_filename = elem_result.text().await?;
    let new_filename = re.replace(&new_filename, "").into_owned();
    Ok((filepath, new_filename, String::from(app_id)))
}

#[tokio::main]
async fn main() -> WebDriverResult<()> {
    let matches = App::new("APK Downloader")
        .author("William Budington <bill@eff.org>")
        .about("Downloads APKs from various sources")
        .usage("apk-downloader <-a app_name | -l list_source> [--download-source download_source] [--parallel parallel] OUTPUT ")
        .arg(
            Arg::with_name("list_source")
                .help("Source of the apps list")
                .short("l")
                .long("list-source")
                .takes_value(true)
                .possible_values(&ListSource::variants()))
        .arg(
            Arg::with_name("app_name")
                .help("Provide the name of an app directly")
                .short("a")
                .long("app-name")
                .takes_value(true)
                .conflicts_with("list_source")
                .required_unless("list_source"))
        .arg(
            Arg::with_name("download_source")
                .help("Where to download the APKs from")
                .short("d")
                .long("download-source")
                .default_value("APKPure")
                .takes_value(true)
                .possible_values(&DownloadSource::variants())
                .required(false))
        .arg(
            Arg::with_name("google_username")
                .help("Google Username (required if download source is Google Play)")
                .short("u")
                .long("username")
                .takes_value(true)
                .required_if("download_source", "GooglePlay"))
        .arg(
            Arg::with_name("google_password")
                .help("Google App Password (required if download source is Google Play)")
                .short("p")
                .long("password")
                .takes_value(true)
                .required_if("download_source", "GooglePlay"))
        .arg(
            Arg::with_name("parallel")
                .help("The number of parallel APK fetches to run at a time")
                .short("r")
                .long("parallel")
                .takes_value(true)
                .default_value("4")
                .required(false))
        .arg(Arg::with_name("OUTPUT")
            .help("An absolute path to store output files")
            .required(true)
            .index(1))
        .get_matches();

    let download_source = value_t!(matches.value_of("download_source"), DownloadSource).unwrap();
    let parallel = value_t!(matches, "parallel", usize).unwrap();
    let outpath = matches.value_of("OUTPUT").unwrap();
    let list = match matches.value_of("app_name") {
        Some(app_name) => vec![app_name.to_string()],
        None => {
            let list_source = value_t!(matches.value_of("list_source"), ListSource).unwrap();
            fetch_list(&list_source).await.unwrap()
        }
    };

    match download_source {
        DownloadSource::APKPure => {
            download_apps_from_apkpure(list, parallel, outpath).await.unwrap();
        },
        DownloadSource::GooglePlay => {
            let username = matches.value_of("google_username").unwrap();
            let password = matches.value_of("google_password").unwrap();
            download_apps_from_google_play(list, parallel, username, password, outpath).await;
        },
    }
    Ok(())
}
