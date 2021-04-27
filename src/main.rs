use futures_util::StreamExt;
use regex::Regex;
use serde_json::json;
use std::env;
use std::fs;
use std::path::Path;
use std::time::Duration;
use thirtyfour::prelude::*;

fn help_message_and_exit(program_name: &str, status_code: i32) -> String {
    println!("Usage: {} OUTPATH", program_name);
    std::process::exit(status_code);
}

async fn fetch_ar_list() -> Result<Vec<String>, Box<dyn std::error::Error>> {
    let resp = reqwest::get("https://www.androidrank.org/applist.csv")
        .await?
        .error_for_status()?
        .text()
        .await?;

    Ok(resp.split("\n").filter_map(|l| {
        let entry = l.trim();
        let mut entry_vec = entry.split(",").collect::<Vec<&str>>();
        if entry_vec.len() > 1 {
            Some(String::from(entry_vec.remove(0)))
        } else {
            None
        }
    }).collect())
}

async fn download_ar_apps(app_ids: Vec<String>, outpath: &str) -> WebDriverResult<()> {
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
    //).buffer_unordered(4).filter_map(|i| i).collect::<Vec<(String, String, String)>>();
    ).buffer_unordered(4).filter_map(|i| i).collect::<Vec<(String, String, String)>>();
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
    let args: Vec<String> = env::args().collect();
    if args.len() != 2 {
        help_message_and_exit(&args[0], 1);
    }
    let outpath = &args[1];

    let ar_list = fetch_ar_list().await.unwrap();
    download_ar_apps(ar_list, outpath).await.unwrap();
    Ok(())
}
