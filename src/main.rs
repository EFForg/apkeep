#[macro_use]
extern crate clap;

use clap::{Arg, App};
use futures_util::StreamExt;
use gpapi::Gpapi;
use regex::Regex;
use serde_json::json;
use std::error::Error;
use std::fs;
use std::path::Path;
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

async fn download_apps_from_google_play(app_ids: Vec<String>, processes: usize, username: &str, password: &str, outpath: &str) {
    let mut gpa = Gpapi::new("en_US", "UTC", "hero2lte");
    gpa.login(username, password).await.expect("Could not log in to google play");

    futures_util::stream::iter(
        app_ids.into_iter().map(|app_id| {
            println!("Downloading {}...", app_id);
            gpa.download(app_id, None, &Path::new(outpath))
        })
    ).buffer_unordered(processes).collect::<Vec<Result<(), Box<dyn Error>>>>().await;
}

async fn download_apps_from_apkpure(app_ids: Vec<String>, processes: usize, outpath: &str) -> WebDriverResult<()> {
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
    ).buffer_unordered(processes).filter_map(|i| i).collect::<Vec<(String, String, String)>>();
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
    let matches = App::new("Batch APK Downloader")
        .author("William Budington <bill@eff.org>")
        .about("Downloads APKs from various sources")
        .usage("batch-apk-downloader <OUTPUT> --download-source <download-source> --list-source <list-source> --processes <processes>")
        .arg(
            Arg::with_name("list-source")
                .help("Source of the apps list")
                .short("l")
                .long("list-source")
                .default_value("AndroidRank")
                .takes_value(true)
                .possible_values(&ListSource::variants())
                .required(false))
        .arg(
            Arg::with_name("download-source")
                .help("Where to download the APKs from")
                .short("d")
                .long("download-source")
                .default_value("APKPure")
                .takes_value(true)
                .possible_values(&DownloadSource::variants())
                .required(false))
        .arg(
            Arg::with_name("google-username")
                .help("Google Username (required if download source is Google Play)")
                .short("u")
                .long("username")
                .takes_value(true)
                .required_if("download-source", "GooglePlay"))
        .arg(
            Arg::with_name("google-password")
                .help("Google App Password (required if download source is Google Play)")
                .short("p")
                .long("password")
                .takes_value(true)
                .required_if("download-source", "GooglePlay"))
        .arg(
            Arg::with_name("processes")
                .help("The number of parallel APK fetches to run at a time")
                .short("r")
                .long("processes")
                .takes_value(true)
                .default_value("4")
                .required(false))
        .arg(Arg::with_name("OUTPUT")
            .help("An absolute path to store output files")
            .required(true)
            .index(1))
        .get_matches();

    let list_source = value_t!(matches.value_of("list-source"), ListSource).unwrap();
    let download_source = value_t!(matches.value_of("download-source"), DownloadSource).unwrap();
    let processes = value_t!(matches, "processes", usize).unwrap();
    let outpath = matches.value_of("OUTPUT").unwrap();

    let list = fetch_list(&list_source).await.unwrap();
    match download_source {
        DownloadSource::APKPure => {
            download_apps_from_apkpure(list, processes, outpath).await.unwrap();
        },
        DownloadSource::GooglePlay => {
            let username = matches.value_of("google-username").unwrap();
            let password = matches.value_of("google-password").unwrap();
            download_apps_from_google_play(list, processes, username, password, outpath).await;
        },
    }
    Ok(())
}
