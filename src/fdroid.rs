use std::collections::HashSet;
use std::error::Error;
use std::fs::{self, File};
use std::io;
use std::io::prelude::*;
use std::path::{Path, PathBuf};
use std::rc::Rc;

use cryptographic_message_syntax::{SignedData, SignerInfo};
use futures_util::StreamExt;
use openssl::x509::X509;
use openssl::hash::MessageDigest;
use regex::Regex;
use serde_json::Value;
use sha1::{Sha1, Digest};
use simple_error::SimpleError;
use tempfile::{tempdir, TempDir};
use tokio::time::{sleep, Duration};
use tokio_dl_stream_to_disk::error::ErrorKind as TDSTDErrorKind;
use x509_certificate::certificate::CapturedX509Certificate;

use crate::consts;
mod error;
use error::Error as FDroidError;

async fn retrieve_index_or_exit() -> Value {
    let temp_dir = match tempdir() {
        Ok(temp_dir) => temp_dir,
        Err(_) => {
            println!("Could not create temporary directory for F-Droid package index. Exiting.");
            std::process::exit(1);
        }
    };
    let config_dir = match dirs::config_dir() {
        Some(mut config_dir) => {
            let create_dir = |config_dir: &PathBuf| {
                if !config_dir.is_dir() &&  fs::create_dir(config_dir).is_err() {
                    println!("Could not create a config directory for apkeep to store F-Droid package index. Exiting.");
                    std::process::exit(1);
                }
            };
            create_dir(&config_dir);
            config_dir.push("apkeep");
            create_dir(&config_dir);
            config_dir
        },
        None => {
            println!("Could not find a config directory for apkeep to store F-Droid package index. Exiting.");
            std::process::exit(1);
        },
    };
    let mut latest_etag_file = PathBuf::from(&config_dir);
    latest_etag_file.push("latest_etag");
    let latest_etag = match File::open(&latest_etag_file) {
        Ok(mut file) => {
            let mut contents = String::new();
            if file.read_to_string(&mut contents).is_err() {
                println!("Could not read latest_etag file for F-Droid package index. Exiting.");
                std::process::exit(1);
            }
            Some(contents)
        },
        Err(_) => None,
    };

    let http_client = reqwest::Client::new();
    let index_response = http_client
        .head(consts::FDROID_INDEX_URL)
        .send().await.unwrap();

    let etag = if index_response.headers().contains_key("ETag") {
        index_response.headers()["ETag"].to_str().unwrap()
    } else {
        println!("Could not receive etag for F-Droid package index. Exiting.");
        std::process::exit(1);
    };

    let mut index_file = PathBuf::from(&config_dir);
    index_file.push("index_v1.json");
    if latest_etag.is_some() && latest_etag.unwrap() == etag {
        let index = {
            let mut file = File::open(&index_file).unwrap();
            let mut contents = String::new();
            file.read_to_string(&mut contents).unwrap();
            contents
        };
        serde_json::from_str(&index).unwrap()
    } else {
        let files = download_and_extract_index_to_tempdir(&temp_dir).await;
        match verify_and_return_index(&temp_dir, &files) {
            Ok(index) => {
                match serde_json::from_str(&index) {
                    Ok(index_value) => {
                        if fs::write(index_file, index).is_err() {
                            println!("Could not write F-Droid package index to config file. Exiting.");
                            std::process::exit(1);
                        }
                        if fs::write(latest_etag_file, etag).is_err() {
                            println!("Could not write F-Droid etag to config file. Exiting.");
                            std::process::exit(1);
                        }
                        index_value
                    }
                    Err(_) => {
                        println!("Could not decode JSON for F-Droid package index. Exiting.");
                        std::process::exit(1);
                    }
                }
            },
            Err(_) => {
                println!("Could not verify F-Droid package index. Exiting.");
                std::process::exit(1);
            },
        }
    }
}

pub async fn download_apps(
    apps: Vec<(String, Option<String>)>,
    parallel: usize,
    sleep_duration: u64,
    outpath: &Path,
) {
    let index = retrieve_index_or_exit().await;

    let (fdroid_apps, repo_address) = match parse_json_for_download_information(index, apps) {
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
            async move {
                let app_string = match app_version {
                    Some(ref version) => {
                        println!("Downloading {} version {}...", app_id, version);
                        format!("{}@{}", app_id, version)
                    },
                    None => {
                        println!("Downloading {}...", app_id);
                        app_id.to_string()
                    },
                };
                let fname = format!("{}.apk", app_string);
                if sleep_duration > 0 {
                    sleep(Duration::from_millis(sleep_duration)).await;
                }
                let download_url = format!("{}/{}", repo_address, url_filename);
                let sha256sum = match tokio_dl_stream_to_disk::download_and_return_sha256sum(&download_url, Path::new(outpath), &fname).await {
                    Ok(sha256sum) => Some(sha256sum),
                    Err(err) if matches!(err.kind(), TDSTDErrorKind::FileExists) => {
                        println!("File already exists for {}. Skipping...", app_string);
                        None
                    },
                    Err(err) if matches!(err.kind(), TDSTDErrorKind::PermissionDenied) => {
                        println!("Permission denied when attempting to write file for {}. Skipping...", app_string);
                        None
                    },
                    Err(_) => {
                        println!("An error has occurred attempting to download {}.  Retry #1...", app_string);
                        match tokio_dl_stream_to_disk::download_and_return_sha256sum(&download_url, Path::new(outpath), &fname).await {
                            Ok(sha256sum) => Some(sha256sum),
                            Err(_) => {
                                println!("An error has occurred attempting to download {}.  Retry #2...", app_string);
                                match tokio_dl_stream_to_disk::download_and_return_sha256sum(&download_url, Path::new(outpath), &fname).await {
                                    Ok(sha256sum) => Some(sha256sum),
                                    Err(_) => {
                                        println!("An error has occurred attempting to download {}. Skipping...", app_string);
                                        None
                                    }
                                }
                            }
                        }
                    }
                };
                if let Some(sha256sum) = sha256sum {
                    if sha256sum == hash {
                        println!("{} downloaded successfully!", app_string);
                    } else {
                        println!("{} downloaded, but the sha256sum does not match the one signed by F-Droid. Proceed with caution.", app_string);
                    }
                }
            }
        })
    ).buffer_unordered(parallel).collect::<Vec<()>>().await;
}

type DownloadInformation = (Vec<(String, Option<String>, String, Vec<u8>)>, String);
fn parse_json_for_download_information(index: Value, apps: Vec<(String, Option<String>)>) -> Result<DownloadInformation, FDroidError> {
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
        if packages.contains_key(&app_id) {
            let app_array_value = packages.get(&app_id).unwrap();
            if app_array_value.is_array() {
                let app_array = app_array_value.as_array().unwrap();
                if app_version.is_none() {
                    if !app_array.is_empty() && app_array[0].is_object() {
                        let fdroid_app = app_array[0].as_object().unwrap();
                        if fdroid_app.contains_key("apkName") && fdroid_app.contains_key("hash") {
                            let filename_value = fdroid_app.get("apkName").unwrap();
                            let hash_value = fdroid_app.get("hash").unwrap();
                            if filename_value.is_string() && hash_value.is_string() {
                                let filename = filename_value.as_str().unwrap().to_string();
                                if let Ok(hash) = hex::decode(hash_value.as_str().unwrap().to_string()) {
                                    return Some((app_id, app_version, filename, hash));
                                }
                            }
                        }
                    }
                } else {
                    for single_app in app_array {
                        if single_app.is_object() {
                            let fdroid_app = single_app.as_object().unwrap();
                            if fdroid_app.contains_key("versionName") {
                                let version_name_value = fdroid_app.get("versionName").unwrap();
                                if version_name_value.is_string() {
                                    let version_name = version_name_value.as_str().unwrap().to_string();
                                    if version_name == *app_version.as_ref().unwrap() && fdroid_app.contains_key("apkName") && fdroid_app.contains_key("hash") {
                                        let filename_value = fdroid_app.get("apkName").unwrap();
                                        let hash_value = fdroid_app.get("hash").unwrap();
                                        if filename_value.is_string() && hash_value.is_string() {
                                            let filename = filename_value.as_str().unwrap().to_string();
                                            if let Ok(hash) = hex::decode(hash_value.as_str().unwrap().to_string()) {
                                                return Some((app_id, app_version, filename, hash));
                                            }
                                        }
                                    }
                                }
                            }
                        }
                    }
                    println!("Could not find version {} of {}. Skipping...", app_version.unwrap(), app_id);
                    return None;
                }
            }
        } else {
            println!("Could not find {} in package list. Skipping...", app_id);
        }
        None
    }).flatten().collect();

    Ok((fdroid_apps, repo_address.to_string()))
}

pub async fn list_versions(apps: Vec<(String, Option<String>)>) {
    let index = retrieve_index_or_exit().await;
    if parse_json_display_versions(index, apps).is_err() {
        println!("Could not parse JSON of F-Droid package index. Exiting.");
    };
}

fn parse_json_display_versions(index: Value, apps: Vec<(String, Option<String>)>) -> Result<(), FDroidError> {
    let index_map = index.as_object().ok_or(FDroidError::Dummy)?;

    let packages = index_map
        .get("packages").ok_or(FDroidError::Dummy)?
        .as_object().ok_or(FDroidError::Dummy)?;

    for app in apps {
        let (app_id, _) = app;
        println!("Versions available for {} on F-Droid:", app_id);
        if packages.contains_key(&app_id) {
            let app_array_value = packages.get(&app_id).unwrap();
            if app_array_value.is_array() {
                let app_array = app_array_value.as_array().unwrap();
                let mut versions = HashSet::new();
                for single_app in app_array {
                    if single_app.is_object() {
                        let fdroid_app = single_app.as_object().unwrap();
                        if fdroid_app.contains_key("versionName") {
                            let version_name_value = fdroid_app.get("versionName").unwrap();
                            if version_name_value.is_string() {
                                let version_name = version_name_value.as_str().unwrap().to_string();
                                versions.insert(version_name);
                            }
                        }
                    }
                }
                let mut versions = versions.drain().collect::<Vec<String>>();
                versions.sort();
                println!("| {}", versions.join(", "));
            }
        } else {
            println!("| Could not find {} in package list. Skipping...", app_id);
        }
    }
    Ok(())
}

fn verify_and_return_index(dir: &TempDir, files: &[String]) -> Result<String, Box<dyn Error>> {
    println!("Verifying...");
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

    let signed_file_data = fs::read(signed_file)?;

    {
        let (cert, signer_info) = get_certificate_and_signer_info_from_signature_block_file(cert_file)?;
        cert.verify_signed_data(signed_file_data.clone(), signer_info.signature())?;
        let x509 = X509::from_der(&cert.encode_ber()?)?;
        let fingerprint = x509.digest(MessageDigest::from_name("sha256").unwrap())?;
        if fingerprint.as_ref() != consts::FDROID_INDEX_FINGERPRINT {
            return Err(Box::new(SimpleError::new("Fingerprint of the key contained in the F-Droid repository index does not match the expected fingerprint.")))
        };
    }

    let signed_file_string = std::str::from_utf8(&signed_file_data)?;
    let manifest_file = dir.path().join("META-INF").join("MANIFEST.MF");
    let manifest_file_data = fs::read(manifest_file)?;
    {
        let signed_file_regex = Regex::new(r"\r\nSHA1-Digest-Manifest: (.*)\r\n").unwrap();
        let signed_file_manifest_sha1sum = base64::decode(match signed_file_regex.captures(signed_file_string) {
            Some(caps) if caps.len() >= 2 => caps.get(1).unwrap().as_str(),
            _ => {
                return Err(Box::new(SimpleError::new("Could not retrieve the manifest sha1sum from the signed file.")));
            }
        })?;
        let mut hasher = Sha1::new();
        hasher.update(manifest_file_data.clone());
        let actual_manifest_sha1sum = hasher.finalize();
        if signed_file_manifest_sha1sum != actual_manifest_sha1sum[..] {
            return Err(Box::new(SimpleError::new("The manifest sha1sum from the signed file does not match the actual manifest sha1sum.")));
        }
    }

    let manifest_file_string = std::str::from_utf8(&manifest_file_data)?;
    let index_file = dir.path().join("index-v1.json");
    let index_file_data = fs::read(index_file)?;
    {
        let manifest_file_regex = Regex::new(r"\r\nName: index-v1\.json\r\nSHA1-Digest: (.*)\r\n").unwrap();
        let manifest_file_index_sha1sum = base64::decode(match manifest_file_regex.captures(manifest_file_string) {
            Some(caps) if caps.len() >= 2 => caps.get(1).unwrap().as_str(),
            _ => {
                return Err(Box::new(SimpleError::new("Could not retrieve the index sha1sum from the manifest file.")));
            }
        })?;
        let mut hasher = Sha1::new();
        hasher.update(index_file_data.clone());
        let actual_index_sha1sum = hasher.finalize();
        if manifest_file_index_sha1sum != actual_index_sha1sum[..] {
            return Err(Box::new(SimpleError::new("The index sha1sum from the manifest file does not match the actual index sha1sum.")));
        }
    }

    Ok(String::from(std::str::from_utf8(&index_file_data)?))
}

fn get_certificate_and_signer_info_from_signature_block_file(signature_block_file: PathBuf) -> Result<(CapturedX509Certificate, SignerInfo), Box<dyn Error>> {
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
            Ok((certificates[0].clone(), signatories[0].clone()))
        },
        Err(err) => Err(Box::new(err)),
    }
}

async fn download_and_extract_index_to_tempdir(dir: &TempDir) -> Vec<String> {
    println!("Downloading F-Droid package repository...");
    let mut files = vec![];
    match tokio_dl_stream_to_disk::download(consts::FDROID_INDEX_URL, dir.path(), "index.zip").await {
        Ok(_) => {
            println!("Package repository downloaded successfully!\nExtracting...");
            let file = fs::File::open(dir.path().join("index.zip")).unwrap();
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
                    println!("F-Droid package index could not be extracted. Please try again.");
                    std::process::exit(1);
                }
            }
        }
        Err(_) => {
            println!("Could not download F-Droid package index.");
            std::process::exit(1);
        }
    }
    files
}
