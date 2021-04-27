use futures_util::StreamExt;
use regex::Regex;
use serde_json::json;
use std::env;
use std::fs;
use std::path::Path;
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
                println!("Downloading {}...", app_id);
                let app_url = format!("https://apkpure.com/a/{}/download?from=details", app_id);
                let mut caps = DesiredCapabilities::chrome();
                let filepath = format!("{}", Path::new(outpath).join(app_id).to_str().unwrap());
                let prefs = json!({
                    "download.default_directory": filepath
                });
                caps.add_chrome_option("prefs", prefs).unwrap();

                let driver = WebDriver::new("http://localhost:4444", &caps).await.unwrap();
                driver.get(app_url).await.unwrap();
                let elem_result = driver.find_element(By::Css("span.file")).await.unwrap();
                let re = Regex::new(r" \([0-9.]+ MB\)$").unwrap();
                let new_filename = elem_result.text().await.unwrap();
                let new_filename = re.replace(&new_filename, "").into_owned();
                let paths = fs::read_dir(&filepath).unwrap();
                let old_filename = paths.filter_map(|path| path.ok()).collect::<Vec<fs::DirEntry>>()[0].file_name();
                fs::rename(Path::new(&filepath).join(old_filename), Path::new(&filepath).join(new_filename)).unwrap();
            }
        })
    ).buffer_unordered(4).collect::<Vec<()>>();
    println!("Waiting...");
    fetches.await;
    Ok(())
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
