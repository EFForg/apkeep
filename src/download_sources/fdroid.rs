use std::collections::{hash_map::DefaultHasher, HashSet, HashMap};
use std::error::Error;
use std::fs::{self, File};
use std::hash::{Hash, Hasher};
use std::io;
use std::io::prelude::*;
use std::path::{Path, PathBuf};
use std::rc::Rc;

use base64::{Engine as _, engine::general_purpose as b64_general_purpose};
use cryptographic_message_syntax::{SignedData, SignerInfo};
use futures_util::StreamExt;
use indicatif::MultiProgress;
use regex::Regex;
use ring::digest::{Context, SHA256};
use serde_json::{json, Value};
use sha1::{Sha1, Digest as Sha1Digest};
use sha2::Sha256;
use simple_error::SimpleError;
use tempfile::{tempdir, TempDir};
use tokio::time::{sleep, Duration};
use tokio_dl_stream_to_disk::{AsyncDownload, error::ErrorKind as TDSTDErrorKind};
use x509_certificate::certificate::CapturedX509Certificate;

use crate::consts;
use crate::config::{self, ConfigDirError};
use crate::util::{OutputFormat, progress_bar::progress_wrapper};
mod error;
use error::Error as FDroidError;

async fn retrieve_index_or_exit(options: &HashMap<&str, &str>, mp: Rc<MultiProgress>, output_format: OutputFormat) -> Value {
    let temp_dir = match tempdir() {
        Ok(temp_dir) => temp_dir,
        Err(_) => {
            print_error("Could not create temporary directory for F-Droid package index. Exiting.", output_format);
            std::process::exit(1);
        }
    };
    let mut custom_repo = false;
    let mut repo = consts::FDROID_REPO.to_string();
    let mut fingerprint = Vec::from(consts::FDROID_INDEX_FINGERPRINT);
    let use_entry = match options.get("use_entry") {
        Some(val) if val == &"0" || val.to_lowercase() == "false" => false,
        _ => true,
    };
    if let Some(full_repo_option) = options.get("repo") {
        custom_repo = true;
        if let Some((repo_option, fingerprint_option)) = full_repo_option.split_once("?fingerprint=") {
            fingerprint = match hex::decode(fingerprint_option) {
                Ok(hex_fingerprint) => hex_fingerprint,
                Err(_) => {
                    print_error("Fingerprint must be specified as valid hex. Exiting.", output_format);
                    std::process::exit(1);
                }
            };
            repo = repo_option.to_string();
        } else {
            repo = full_repo_option.to_string();
        }
    }

    let display_error_and_exit = |err: ConfigDirError| {
        match err {
            ConfigDirError::NotFound => {
                print_error("Could not find a config directory for apkeep to store F-Droid package index. Exiting.", output_format.clone());
            },
            ConfigDirError::CouldNotCreate => {
                print_error("Could not create a config directory for apkeep to store F-Droid package index. Exiting.", output_format.clone());
            },
        }
        std::process::exit(1);
    };
    let mut config_dir = config::config_dir().map_err(display_error_and_exit).unwrap();
    if custom_repo {
        config_dir.push("fdroid-custom-repos");
        config::create_dir(&config_dir).map_err(display_error_and_exit).unwrap();
        let mut s = DefaultHasher::new();
        repo.hash(&mut s);
        config_dir.push(format!("{}", s.finish()));
        config::create_dir(&config_dir).map_err(display_error_and_exit).unwrap();
    }

    let mut latest_etag_file = PathBuf::from(&config_dir);
    if use_entry {
        latest_etag_file.push("latest_entry_etag");
    } else {
        latest_etag_file.push("latest_etag");
    }
    let latest_etag = match File::open(&latest_etag_file) {
        Ok(mut file) => {
            let mut contents = String::new();
            if file.read_to_string(&mut contents).is_err() {
                print_error("Could not read etag file for F-Droid package index. Exiting.", output_format);
                std::process::exit(1);
            }
            Some(contents)
        },
        Err(_) => None,
    };

    let http_client = reqwest::Client::new();
    let fdroid_jar_url = if use_entry {
        format!("{}/entry.jar", repo)
    } else {
        format!("{}/index-v1.jar", repo)
    };
    let jar_response = http_client
        .head(fdroid_jar_url)
        .send().await.unwrap();

    let etag = if jar_response.headers().contains_key("ETag") {
        jar_response.headers()["ETag"].to_str().unwrap()
    } else {
        print_error("Could not receive etag for F-Droid package index. Exiting.", output_format);
        std::process::exit(1);
    };

    let mut index_file = PathBuf::from(&config_dir);
    if use_entry {
        index_file.push("index.json");
    } else {
        index_file.push("index_v1.json");
    }
    if latest_etag.is_some() && latest_etag.unwrap() == etag {
        let index = read_file_to_string(index_file);
        serde_json::from_str(&index).unwrap()
    } else {
        let files = download_and_extract_to_tempdir(&temp_dir, &repo, Rc::clone(&mp), use_entry, output_format.clone()).await;
        let verify_index = match options.get("verify-index") {
            Some(&"false") => false,
            _ => true,
        };
        match verify_and_return_json(&temp_dir, &files, &fingerprint, verify_index, use_entry, Rc::clone(&mp)) {
            Ok(json) => {
                let index = if use_entry {
                    match verify_and_return_index_from_entry(&temp_dir, &repo, &json, verify_index, mp, output_format.clone()).await {
                        Ok(index_from_entry) => {
                            index_from_entry
                        }
                        Err(_) => {
                            print_error("Could verify and return package index from entry JSON. Exiting.", output_format);
                            std::process::exit(1);
                        }
                    }
                } else {
                    json
                };

                match serde_json::from_str(&index) {
                    Ok(index_value) => {
                        if fs::write(index_file, index).is_err() {
                            print_error("Could not write F-Droid package index to config file. Exiting.", output_format);
                            std::process::exit(1);
                        }
                        if fs::write(latest_etag_file, etag).is_err() {
                            print_error("Could not write F-Droid etag to config file. Exiting.", output_format);
                            std::process::exit(1);
                        }
                        index_value
                    }
                    Err(_) => {
                        print_error("Could not decode JSON for F-Droid package index. Exiting.", output_format);
                        std::process::exit(1);
                    }
                }
            },
            Err(_) => {
                print_error("Could not verify F-Droid package index. Exiting.", output_format);
                std::process::exit(1);
            },
        }
    }
}

fn print_error(err_msg: &str, output_format: OutputFormat) {
    match output_format {
        OutputFormat::Plaintext => eprintln!("{}", err_msg),
        OutputFormat::Json => println!("{{\"error\":\"{}\"}}", err_msg),
    }
}

fn read_file_to_string(file: PathBuf ) -> String {
    let mut file = File::open(&file).unwrap();
    let mut contents = String::new();
    file.read_to_string(&mut contents).unwrap();
    contents
}

pub async fn download_apps(
    apps: Vec<(String, Option<String>)>,
    parallel: usize,
    sleep_duration: u64,
    outpath: &Path,
    options: HashMap<&str, &str>,
) {
    let mp = Rc::new(MultiProgress::new());
    let index = retrieve_index_or_exit(&options, Rc::clone(&mp), OutputFormat::Plaintext).await;

    let app_arch = options.get("arch").map(|x| x.to_string());
    let (fdroid_apps, repo_address) = match parse_json_for_download_information(index, apps, app_arch.clone(), Rc::clone(&mp)) {
        Ok((fdroid_apps, repo_address)) => (fdroid_apps, repo_address),
        Err(_) => {
            println!("Could not parse JSON of F-Droid package index. Exiting.");
            std::process::exit(1);
        },
    };

    let repo_address = Rc::new(repo_address);
    futures_util::stream::iter(
        fdroid_apps.into_iter().map(|fdroid_app| {
            let (app_id, app_version, url_filename, hash) = fdroid_app;
            let repo_address = Rc::clone(&repo_address);
            let mp_log = Rc::clone(&mp);
            let mp = Rc::clone(&mp);
            let app_arch = app_arch.clone();
            async move {
                let app_string = match (app_version, app_arch) {
                    (None, None) => {
                        mp_log.suspend(|| println!("Downloading {}...", app_id));
                        app_id.to_string()
                    },
                    (None, Some(arch)) => {
                        mp_log.suspend(|| println!("Downloading {} arch {}...", app_id, arch));
                        format!("{}@{}", app_id, arch)
                    },
                    (Some(version), None) => {
                        mp_log.suspend(|| println!("Downloading {} version {}...", app_id, version));
                        format!("{}@{}", app_id, version)
                    },
                    (Some(version), Some(arch)) => {
                        mp_log.suspend(|| println!("Downloading {} version {} arch {}...", app_id, version, arch));
                        format!("{}@{}@{}", app_id, version, arch)
                    },
                };
                let fname = format!("{}.apk", app_string);
                if sleep_duration > 0 {
                    sleep(Duration::from_millis(sleep_duration)).await;
                }
                let download_url = format!("{}/{}", repo_address, url_filename);
                match AsyncDownload::new(&download_url, Path::new(outpath), &fname).get().await {
                    Ok(mut dl) => {
                        let length = dl.length();
                        let cb = match length {
                            Some(length) => Some(progress_wrapper(mp)(fname.clone(), length)),
                            None => None,
                        };

                        let sha256sum = match dl.download_and_return_sha256sum(&cb).await {
                            Ok(sha256sum) => Some(sha256sum),
                            Err(err) if matches!(err.kind(), TDSTDErrorKind::FileExists) => {
                                mp_log.println(format!("File already exists for {}. Skipping...", app_string)).unwrap();
                                None
                            },
                            Err(err) if matches!(err.kind(), TDSTDErrorKind::PermissionDenied) => {
                                mp_log.println(format!("Permission denied when attempting to write file for {}. Skipping...", app_string)).unwrap();
                                None
                            },
                            Err(_) => {
                                mp_log.println(format!("An error has occurred attempting to download {}.  Retry #1...", app_string)).unwrap();
                                match AsyncDownload::new(&download_url, Path::new(outpath), &fname).download_and_return_sha256sum(&cb).await {
                                    Ok(sha256sum) => Some(sha256sum),
                                    Err(_) => {
                                        mp_log.println(format!("An error has occurred attempting to download {}.  Retry #2...", app_string)).unwrap();
                                        match AsyncDownload::new(&download_url, Path::new(outpath), &fname).download_and_return_sha256sum(&cb).await {
                                            Ok(sha256sum) => Some(sha256sum),
                                            Err(_) => {
                                                mp_log.println(format!("An error has occurred attempting to download {}. Skipping...", app_string)).unwrap();
                                                None
                                            }
                                        }
                                    }
                                }
                            }
                        };
                        if let Some(sha256sum) = sha256sum {
                            if sha256sum == hash {
                                mp_log.suspend(|| println!("{} downloaded successfully!", app_string));
                            } else {
                                mp_log.suspend(|| println!("{} downloaded, but the sha256sum does not match the one signed by F-Droid. Proceed with caution.", app_string));
                            }
                        }
                    },
                    Err(_) => {
                        mp_log.println(format!("Invalid response for {}. Skipping...", app_string)).unwrap();
                    },
                }
            }
        })
    ).buffer_unordered(parallel).collect::<Vec<()>>().await;
}

type DownloadInformation = (Vec<(String, Option<String>, String, Vec<u8>)>, String);
/// This currently works for `index-v1.json` as well as an index with version `20002`.  It is
/// flexible enough to parse either, and may work on future index versions as well.  Since `sha256`
/// digests are checked before proceeding, I don't foresee this having an insecure failure mode, so
/// checking the index version and making the parsing overly brittle has no substantive advantage.
fn parse_json_for_download_information(index: Value, apps: Vec<(String, Option<String>)>, app_arch: Option<String>, mp_log: Rc<MultiProgress>) -> Result<DownloadInformation, FDroidError> {
    let index_map = index.as_object().ok_or(FDroidError::Dummy)?;
    let repo_address = index_map
        .get("repo").ok_or(FDroidError::Dummy)?
        .get("address").ok_or(FDroidError::Dummy)?
        .as_str().ok_or(FDroidError::Dummy)?;

    let packages = index_map
        .get("packages").ok_or(FDroidError::Dummy)?
        .as_object().ok_or(FDroidError::Dummy)?;

    let fdroid_apps: Vec<(String, Option<String>, String, Vec<u8>)> = apps.into_iter().map(|app| {
        let (app_id, app_version) = app;
        match packages.get(&app_id) {
            Some(Value::Array(app_array)) => {
                for single_app in app_array {
                    if let Value::Object(fdroid_app) = single_app {
                        if let Some(Value::String(version_name)) = fdroid_app.get("versionName") {
                            if app_version.is_none() || version_name == app_version.as_ref().unwrap() {
                                if let (Some(Value::String(filename)), Some(Value::String(hash))) = (fdroid_app.get("apkName"), fdroid_app.get("hash")) {
                                    if let Ok(hash) = hex::decode(hash.to_string()) {
                                        if let Some(arch) = &app_arch {
                                            if let Some(Value::Array(nativecode_array)) = fdroid_app.get("nativecode") {
                                                if nativecode_array.iter().any(|value| {
                                                    if let Value::String(value_str) = value{
                                                        value_str == arch
                                                    } else {
                                                        false
                                                    }
                                                }) {
                                                    return Some((app_id, app_version, filename.to_string(), hash));
                                                }
                                            }
                                        } else {
                                            return Some((app_id, app_version, filename.to_string(), hash));
                                        }
                                    }
                                }
                            }
                        }
                    }
                }
                let arch_str = app_arch.as_ref().map_or("".to_string(), |x| format!(" {}", x));
                mp_log.println(format!("Could not find version {}{} of {}. Skipping...", app_version.unwrap(), arch_str, app_id)).unwrap();
                return None;
            },
            Some(Value::Object(app_object)) => {
                if let Some(Value::Object(versions)) = app_object.get("versions") {
                    let mut latest_version = 0;
                    let mut filename = String::new();
                    let mut hash = String::new();
                    for (_, version_value) in versions {
                        if let Value::Object(version) = version_value {
                            if let (Some(Value::Object(manifest)), Some(Value::Object(file))) = (version.get("manifest"), version.get("file")) {
                                if let (Some(Value::String(name)), Some(Value::String(sha256))) = (file.get("name"), file.get("sha256")) {
                                    if app_version.is_some() {
                                        if let Some(Value::String(version_name)) = manifest.get("versionName") {
                                            if version_name == app_version.as_ref().unwrap() {
                                                if let Ok(sha256) = hex::decode(sha256.to_string()) {
                                                    return Some((app_id, app_version, name.to_string(), sha256));
                                                }
                                            }
                                        }
                                    } else {
                                        if let Some(Value::Number(version_code_number)) = manifest.get("versionCode") {
                                            if let Some(version_code) = version_code_number.as_u64() {
                                                if version_code > latest_version {
                                                    latest_version = version_code;
                                                    filename = name.to_string();
                                                    hash = sha256.to_string();
                                                }
                                            }
                                        }
                                    }
                                }
                            }
                        }
                    }
                    if app_version.is_none() {
                        if let Ok(hash) = hex::decode(hash) {
                            return Some((app_id, app_version, filename, hash));
                        }
                    }
                }
            },
            _ => mp_log.println(format!("Could not find {} in package list. Skipping...", app_id)).unwrap(),
        }
        None
    }).flatten().collect();

    Ok((fdroid_apps, repo_address.to_string()))
}

pub async fn list_versions(apps: Vec<(String, Option<String>)>, options: HashMap<&str, &str>) {
    let mp = Rc::new(MultiProgress::new());
    let output_format = match options.get("output_format") {
        Some(val) if val.to_lowercase() == "json" => OutputFormat::Json,
        _ => OutputFormat::Plaintext,
    };
    let index = retrieve_index_or_exit(&options, mp, output_format.clone()).await;

    if parse_json_display_versions(index, apps, output_format).is_err() {
        eprintln!("Could not parse JSON of F-Droid package index. Exiting.");
        std::process::exit(1);
    };
}

/// The comments for `parse_json_for_download_information` apply here, too.
fn parse_json_display_versions(index: Value, apps: Vec<(String, Option<String>)>, output_format: OutputFormat) -> Result<(), FDroidError> {
    let index_map = index.as_object().ok_or(FDroidError::Dummy)?;

    let packages = index_map
        .get("packages").ok_or(FDroidError::Dummy)?
        .as_object().ok_or(FDroidError::Dummy)?;

    let mut json_root = match output_format {
        OutputFormat::Json => Some(HashMap::new()),
        _ => None,
    };

    for app in apps {
        let (app_id, _) = app;
        if output_format.is_plaintext() {
            println!("Versions available for {} on F-Droid:", app_id);
        }
        let mut versions_set = HashSet::new();
        match packages.get(&app_id) {
            Some(Value::Array(app_array)) => {
                for single_app in app_array {
                    if let Value::Object(fdroid_app) = single_app {
                        if let Some(Value::String(version_name)) = fdroid_app.get("versionName") {
                            versions_set.insert(version_name.to_string());
                        }
                    }
                }
            },
            Some(Value::Object(app_object)) => {
                if let Some(Value::Object(versions)) = app_object.get("versions") {
                    for (_, version_value) in versions {
                        if let Value::Object(version) = version_value {
                            if let Some(Value::Object(manifest)) = version.get("manifest") {
                                if let Some(Value::String(version_name)) = manifest.get("versionName") {
                                    versions_set.insert(version_name.to_string());
                                }
                            }
                        }
                    }
                }
            },
            _ => {
                match output_format {
                    OutputFormat::Plaintext => {
                        eprintln!("| Could not find {} in package list. Skipping...", app_id);
                    },
                    OutputFormat::Json => {
                        let mut app_root = HashMap::new();
                        app_root.insert("error".to_string(), "Not found in package list.".to_string());
                        json_root.as_mut().unwrap().insert(app_id.to_string(), json!(app_root));
                    }
                }
                continue;
            }
        }
        let mut versions_set = versions_set.drain().collect::<Vec<String>>();
        versions_set.sort();
        match output_format {
            OutputFormat::Plaintext => {
                println!("| {}", versions_set.join(", "));
            },
            OutputFormat::Json => {
                let mut app_root: HashMap<String, Vec<HashMap<String, String>>> = HashMap::new();
                app_root.insert("available_versions".to_string(), versions_set.into_iter().map(|v| {
                    let mut version_map = HashMap::new();
                    version_map.insert("version".to_string(), v);
                    version_map
                }).collect());
                json_root.as_mut().unwrap().insert(app_id.to_string(), json!(app_root));
            }
        }
    }
    if output_format.is_json() {
        println!("{{\"source\":\"F-Droid\",\"apps\":{}}}", json!(json_root.unwrap()));
    };
    Ok(())
}

fn verify_and_return_json(dir: &TempDir, files: &[String], fingerprint: &[u8], verify_index: bool, use_entry: bool, mp: Rc<MultiProgress>) -> Result<String, Box<dyn Error>> {
    let re = Regex::new(consts::FDROID_SIGNATURE_BLOCK_FILE_REGEX).unwrap();
    let cert_file = {
        let mut cert_files = vec![];
        for file in files {
            if re.is_match(file) {
                cert_files.push(file.clone());
            }
        }
        if cert_files.is_empty() {
            return Err(Box::new(SimpleError::new("Found no certificate file for F-Droid repository.")));
        }
        if cert_files.len() > 1 {
            return Err(Box::new(SimpleError::new("Found multiple certificate files for F-Droid repository.")));
        }
        dir.path().join(cert_files[0].clone())
    };
    let signed_file = {
        let mut signed_file = cert_file.clone();
        signed_file.set_extension("SF");
        signed_file
    };

    let signed_content = fs::read(signed_file)?;

    if verify_index {
        mp.println("Verifying...").unwrap();
        let signed_data = get_signed_data_from_cert_file(cert_file)?;
        let signer_info = signed_data.signers().next().unwrap();
        signer_info.verify_signature_with_signed_data_and_content(
            &signed_data,
            &signed_content)?;
        let cert = signed_data.certificates().next().unwrap();
        let mut context = Context::new(&SHA256);
        context.update(&cert.encode_ber()?);
        let cert_fingerprint = context.finish();
        if cert_fingerprint.as_ref() != fingerprint {
            return Err(Box::new(SimpleError::new("Fingerprint of the key contained in the F-Droid repository does not match the expected fingerprint.")))
        };
    }

    let signed_file_string = std::str::from_utf8(&signed_content)?;
    let manifest_file = dir.path().join("META-INF").join("MANIFEST.MF");
    let manifest_file_data = fs::read(manifest_file)?;
    if verify_index {
        let (signed_file_regex, sha_algorithm_name) = if use_entry {
            (Regex::new(r"\r\nSHA-256-Digest-Manifest: (.*)\r\n").unwrap(), "sha256sum")
        } else {
            (Regex::new(r"\r\nSHA1-Digest-Manifest: (.*)\r\n").unwrap(), "sha1sum")
        };
        let signed_file_manifest_shasum = b64_general_purpose::STANDARD.decode(match signed_file_regex.captures(signed_file_string) {
            Some(caps) if caps.len() >= 2 => caps.get(1).unwrap().as_str(),
            _ => {
                return Err(Box::new(SimpleError::new(format!("Could not retrieve the manifest {} from the signed file.", sha_algorithm_name))));
            }
        })?;
        let actual_manifest_shasum = if use_entry {
            let mut hasher = Sha256::new();
            hasher.update(manifest_file_data.clone());
            Vec::from(hasher.finalize().as_slice())
        } else {
            let mut hasher = Sha1::new();
            hasher.update(manifest_file_data.clone());
            Vec::from(hasher.finalize().as_slice())
        };
        if signed_file_manifest_shasum != actual_manifest_shasum[..] {
            return Err(Box::new(SimpleError::new(format!("The manifest {} from the signed file does not match the actual manifest {}.", sha_algorithm_name, sha_algorithm_name))));
        }
    }

    let manifest_file_string = std::str::from_utf8(&manifest_file_data)?;
    let json_file = if use_entry {
        dir.path().join("entry.json")
    } else {
        dir.path().join("index-v1.json")
    };
    let json_file_data = fs::read(json_file)?;
    if verify_index {
        let (manifest_file_regex, file_algo) = if use_entry {
            (Regex::new(r"\r\nName: entry\.json\r\nSHA-256-Digest: (.*)\r\n").unwrap(), "entry sha256sum")
        } else {
            (Regex::new(r"\r\nName: index-v1\.json\r\nSHA1-Digest: (.*)\r\n").unwrap(), "index sha1sum")
        };
        let manifest_file_shasum = b64_general_purpose::STANDARD.decode(match manifest_file_regex.captures(manifest_file_string) {
            Some(caps) if caps.len() >= 2 => caps.get(1).unwrap().as_str(),
            _ => {
                return Err(Box::new(SimpleError::new(format!("Could not retrieve the {} from the manifest file.", file_algo))));
            }
        })?;
        let actual_shasum = if use_entry {
            let mut hasher = Sha256::new();
            hasher.update(json_file_data.clone());
            Vec::from(hasher.finalize().as_slice())
        } else {
            let mut hasher = Sha1::new();
            hasher.update(json_file_data.clone());
            Vec::from(hasher.finalize().as_slice())
        };
        if manifest_file_shasum != actual_shasum[..] {
            return Err(Box::new(SimpleError::new(format!("The {} from the manifest file does not match the actual {}.", file_algo, file_algo))));
        }
    }

    Ok(String::from(std::str::from_utf8(&json_file_data)?))
}

async fn verify_and_return_index_from_entry(dir: &TempDir, repo: &str, json: &str, verify_index: bool, mp: Rc<MultiProgress>, output_format: OutputFormat) -> Result<String, Box<dyn Error>> {
    let mp_log = Rc::clone(&mp);
    let (index_name, index_sha256) = match serde_json::from_str::<Value>(json) {
        Ok(entry) => {
            let entry_map = entry.as_object().ok_or(FDroidError::Dummy)?;
            let index_map = entry_map
                .get("index").ok_or(FDroidError::Dummy)?;
            (index_map.get("name").ok_or(FDroidError::Dummy)?
                .as_str().ok_or(FDroidError::Dummy)?.trim_start_matches("/").to_string(),
            index_map.get("sha256").ok_or(FDroidError::Dummy)?
                .as_str().ok_or(FDroidError::Dummy)?.to_string())
        },
        Err(_) => {
            print_error("Could not decode JSON for F-Droid entry file. Exiting.", output_format);
            std::process::exit(1);
        }
    };
    let index_url = format!("{}/{}", repo, index_name);
    let mut dl = AsyncDownload::new(&index_url, dir.path(), &index_name).get().await.unwrap();
    let length = dl.length();
    let cb = match length {
        Some(length) => Some(progress_wrapper(mp)(index_name.to_string(), length)),
        None => None,
    };
    match dl.download(&cb).await {
        Ok(_) => {
            mp_log.println(format!("Package index downloaded successfully!")).unwrap();
            let index_file = dir.path().join(index_name);
            let index_file_data = fs::read(index_file)?;

            if verify_index {
                mp_log.println("Verifying...").unwrap();
                let actual_index_shasum = {
                    let mut hasher = Sha256::new();
                    hasher.update(index_file_data.clone());
                    Vec::from(hasher.finalize().as_slice())
                };
                let index_sha256 = match hex::decode(index_sha256) {
                    Ok(index_sha256) => index_sha256,
                    Err(_) => {
                        print_error("Index sha256sum did not specify valid hex. Exiting.", output_format);
                        std::process::exit(1);
                    }
                };
                if index_sha256 != actual_index_shasum {
                    return Err(Box::new(SimpleError::new("The index sha256sum from the entry file does not match the actual index sha256sum.")));
                }
            }

            Ok(String::from(std::str::from_utf8(&index_file_data)?))
        }
        Err(_) => {
            print_error("Could not download F-Droid package index. Exiting.", output_format);
            std::process::exit(1);
        }
    }
}

fn get_signed_data_from_cert_file(signature_block_file: PathBuf) -> Result<SignedData, Box<dyn Error>> {
    let bytes = fs::read(signature_block_file).unwrap();
    match SignedData::parse_ber(&bytes) {
        Ok(signed_data) => {
            let certificates: Vec<&CapturedX509Certificate> = signed_data.certificates().collect();
            if certificates.len() > 1 {
                return Err(Box::new(SimpleError::new("Too many certificates provided.")));
            }
            if certificates.is_empty() {
                return Err(Box::new(SimpleError::new("No certificate provided.")));
            }
            let signatories: Vec<&SignerInfo> = signed_data.signers().collect();
            if signatories.len() > 1 {
                return Err(Box::new(SimpleError::new("Too many signatories provided.")));
            }
            if signatories.is_empty() {
                return Err(Box::new(SimpleError::new("No signatories provided.")));
            }
            Ok(signed_data)
        },
        Err(err) => {
            Err(Box::new(err))
        }
    }
}

async fn download_and_extract_to_tempdir(dir: &TempDir, repo: &str, mp: Rc<MultiProgress>, use_entry: bool, output_format: OutputFormat) -> Vec<String> {
    let mp_log = Rc::clone(&mp);
    mp_log.suspend(|| println!("Downloading F-Droid package repository..."));
    let mut files = vec![];
    let fdroid_jar_url  = if use_entry {
        format!("{}/entry.jar", repo)
    } else {
        format!("{}/index-v1.jar", repo)
    };
    let jar_local_file = "jar.zip";
    let mut dl = AsyncDownload::new(&fdroid_jar_url, dir.path(), jar_local_file).get().await.unwrap();
    let length = dl.length();
    let cb = match length {
        Some(length) => Some(progress_wrapper(mp)(jar_local_file.to_string(), length)),
        None => None,
    };
    match dl.download(&cb).await {
        Ok(_) => {
            mp_log.suspend(|| println!("Package repository downloaded successfully!\nExtracting..."));
            let file = fs::File::open(dir.path().join(jar_local_file)).unwrap();
            match zip::ZipArchive::new(file) {
                Ok(mut archive) => {
                    for i in 0..archive.len() {
                        let mut file = archive.by_index(i).unwrap();
                        let outpath = match file.enclosed_name() {
                            Some(path) => dir.path().join(path.to_owned()),
                            None => continue,
                        };
                        if (&*file.name()).ends_with('/') {
                            fs::create_dir_all(&outpath).unwrap();
                        } else {
                            if let Some(p) = outpath.parent() {
                                if !p.exists() {
                                    fs::create_dir_all(&p).unwrap();
                                }
                            }
                            files.push(file.enclosed_name().unwrap().to_owned().into_os_string().into_string().unwrap());
                            let mut outfile = fs::File::create(&outpath).unwrap();
                            io::copy(&mut file, &mut outfile).unwrap();
                        }

                        // Get and Set permissions
                        #[cfg(unix)]
                        {
                            use std::os::unix::fs::PermissionsExt;

                            if let Some(mode) = file.unix_mode() {
                                fs::set_permissions(&outpath, fs::Permissions::from_mode(mode)).unwrap();
                            }
                        }
                    }
                },
                Err(_) => {
                    print_error("F-Droid package repository could not be extracted. Please try again.", output_format);
                    std::process::exit(1);
                }
            }
        }
        Err(_) => {
            print_error("Could not download F-Droid package repository.", output_format);
            std::process::exit(1);
        }
    }
    files
}
