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
//! Docker images are also available through the GitHub Container Registry. Aside from using a
//! specific release version, the following floating tags are available:
//!
//! - stable: tracks the latest stable release (recommended)
//! - latest: tracks the latest release, including pre-releases
//! - edge: tracks the latest commit
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
//! apkeep -a com.instagram.android -d google-play -u 'someone@gmail.com' -p somepass .
//! ```
//!
//! For more google play usage examples, such as specifying a device configuration, timezone or
//! locale, refer to the [`USAGE-google-play.md`](USAGE-google-play.md) document.
//!
//! Or, to download from the F-Droid open source repository:
//!
//! ```shell
//! apkeep -a org.mozilla.fennec_fdroid -d f-droid .
//! ```
//!
//! For more F-Droid usage examples, such as downloading from F-Droid mirrors or other F-Droid
//! repositories, refer to the [`USAGE-fdroid.md`](USAGE-fdroid.md) document.
//!
//! To download a specific version of an APK (possible for APKPure or F-Droid), use the `@version`
//! convention:
//!
//! ```shell
//! apkeep -a com.instagram.android@1.2.3 .
//! ```
//!
//! Or, to list what versions are available, use `-l`:
//!
//! ```shell
//! apkeep -l -a org.mozilla.fennec_fdroid -d f-droid
//! ```
//!
//! Refer to [`USAGE`](https://github.com/EFForg/apkeep/blob/master/USAGE) to download multiple
//! APKs in a single run.
//!
//! All the above examples can also be used in Docker with minimal changes. For example, to
//! download a single APK to your chosen output directory:
//!
//! ```shell
//! docker run --rm -v output_path:/output ghcr.io/efforg/apkeep:stable -a com.instagram.android
//! /output
//! ```
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
//! * The Google Play Store (`-d google-play`), given a username and password
//! * APKPure (`-d apk-pure`), a third-party site hosting APKs available on the Play Store
//! * F-Droid (`-d f-droid`), a repository for free and open-source Android apps. `apkeep`
//! verifies that these APKs are signed by the F-Droid maintainers, and alerts the user if an APK
//! was downloaded but could not be verified
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
//! * Paid and DRM apps will not be available.
//! * Using Tor will make it a lot more likely that the download will fail.

use std::collections::HashMap;
use std::error::Error;
use std::fs;
use std::path::Path;

mod cli;
use cli::DownloadSource;

mod consts;

mod apkpure;
mod fdroid;
mod google_play;

type CSVList = Vec<(String, Option<String>)>;
fn fetch_csv_list(csv: &str, field: usize, version_field: Option<usize>) -> Result<CSVList, Box<dyn Error>> {
    Ok(parse_csv_text(fs::read_to_string(csv)?, field, version_field))
}

fn parse_csv_text(text: String, field: usize, version_field: Option<usize>) -> Vec<(String, Option<String>)> {
    let field = field - 1;
    let version_field = version_field.map(|version_field| version_field - 1);
    text.split('\n')
        .filter_map(|l| {
            let entry = l.trim();
            let mut entry_vec = entry.split(',').collect::<Vec<&str>>();
            if entry_vec.len() > field && !(entry_vec.len() == 1 && entry_vec[0].is_empty()) {
                match version_field {
                    Some(mut version_field) if entry_vec.len() > version_field => {
                        if version_field > field {
                            version_field -= 1;
                        }
                        let app_id = String::from(entry_vec.remove(field));
                        let app_version = String::from(entry_vec.remove(version_field));
                        if !app_version.is_empty() {
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

#[tokio::main]
async fn main() {
    let usage = {
        cli::app().render_usage()
    };
    let matches = cli::app().get_matches();

    let download_source: DownloadSource = matches.value_of_t("download_source").unwrap();
    let options: HashMap<&str, &str> = match matches.value_of("options") {
        Some(options) => {
            let mut options_map = HashMap::new();
            for option in options.split(",") {
                match option.split_once("=") {
                    Some((key, value)) => {
                        options_map.insert(key, value);
                    },
                    None => {}
                }
            }
            options_map
        },
        None => HashMap::new()
    };
    let list = match matches.value_of("app") {
        Some(app) => {
            let mut app_vec: Vec<String> = app.splitn(2, '@').map(String::from).collect();
            let app_id = app_vec.remove(0);
            let app_version = match app_vec.len() {
                1 => Some(app_vec.remove(0)),
                _ => None,
            };
            vec![(app_id, app_version)]
        },
        None => {
            let csv = matches.value_of("csv").unwrap();
            let field: usize = matches.value_of_t("field").unwrap();
            let version_field: Option<usize> = matches.value_of_t("version_field").ok();
            if field < 1 {
                println!("{}\n\nApp ID field must be 1 or greater", usage);
                std::process::exit(1);
            }
            if let Some(version_field) = version_field {
                if version_field < 1 {
                    println!("{}\n\nVersion field must be 1 or greater", usage);
                    std::process::exit(1);
                }
                if field == version_field {
                    println!("{}\n\nApp ID and Version fields must be different", usage);
                    std::process::exit(1);
                }
            }
            match fetch_csv_list(csv, field, version_field) {
                Ok(csv_list) => csv_list,
                Err(err) => {
                    println!("{}\n\n{:?}", usage, err);
                    std::process::exit(1);
                }
            }
        }
    };

    if matches.is_present("list_versions") {
        match download_source {
            DownloadSource::APKPure => {
                apkpure::list_versions(list).await;
            }
            DownloadSource::GooglePlay => {
                google_play::list_versions(list);
            }
            DownloadSource::FDroid => {
                fdroid::list_versions(list, options).await;
            }
        }
    } else {
        let parallel: usize = matches.value_of_t("parallel").unwrap();
        let sleep_duration: u64 = matches.value_of_t("sleep_duration").unwrap();
        let outpath = matches.value_of("OUTPATH");
        if outpath.is_none() {
            println!("{}\n\nOUTPATH must be specified when downloading files", usage);
            std::process::exit(1);
        }
        let outpath = match fs::canonicalize(outpath.unwrap()) {
            Ok(outpath) if Path::new(&outpath).is_dir() => {
                outpath
            },
            _ => {
                println!("{}\n\nOUTPATH is not a valid directory", usage);
                std::process::exit(1);
            }
        };

        match download_source {
            DownloadSource::APKPure => {
                apkpure::download_apps(list, parallel, sleep_duration, &outpath).await;
            }
            DownloadSource::GooglePlay => {
                let username = matches.value_of("google_username").unwrap();
                let password = matches.value_of("google_password").unwrap();
                google_play::download_apps(
                    list,
                    parallel,
                    sleep_duration,
                    username,
                    password,
                    &outpath,
                    options,
                )
                .await;
            }
            DownloadSource::FDroid => {
                fdroid::download_apps(list,
                    parallel,
                    sleep_duration,
                    &outpath,
                    options,
                ).await;
            }
        }
    }
}
