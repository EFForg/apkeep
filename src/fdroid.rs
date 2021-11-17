use std::error::Error;
use std::fs;
use std::io;
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

pub async fn download_apps(
    app_ids: Vec<String>,
    parallel: usize,
    sleep_duration: u64,
    outpath: &PathBuf,
) {
    let dir = match tempdir() {
        Ok(dir) => dir,
        Err(_) => {
            println!("Could not create temporary directory for F-Droid package index. Exiting.");
            std::process::exit(1);
        }
    };
    let files = download_and_extract_index_to_tempdir(&dir).await;
    let index = match verify_and_return_index(&dir, &files) {
        Ok(index) => index,
        Err(_) => {
            println!("Could not verify F-Droid package index. Exiting.");
            std::process::exit(1);
        },
    };
    
    let (apps, repo_address) = match parse_json(index, app_ids) {
        Ok((apps, repo_address)) => (apps, repo_address),
        Err(_) => {
            println!("Could not parse JSON of F-Droid package index. Exiting.");
            std::process::exit(1);
        },
    };

    let repo_address = Rc::new(repo_address);
    futures_util::stream::iter(
        apps.into_iter().map(|app| {
            let (app_id, url_filename, hash) = app;
            let repo_address = Rc::clone(&repo_address);
            async move {
                println!("Downloading {}...", app_id);
                if sleep_duration > 0 {
                    sleep(Duration::from_millis(sleep_duration)).await;
                }
                let download_url = format!("{}/{}", repo_address, url_filename);
                let fname = format!("{}.apk", app_id);
                let sha256sum = match tokio_dl_stream_to_disk::download_and_return_sha256sum(&download_url, &Path::new(outpath), &fname).await {
                    Ok(sha256sum) => Some(sha256sum),
                    Err(err) if matches!(err.kind(), TDSTDErrorKind::FileExists) => {
                        println!("File already exists for {}. Skipping...", app_id);
                        None
                    },
                    Err(err) if matches!(err.kind(), TDSTDErrorKind::PermissionDenied) => {
                        println!("Permission denied when attempting to write file for {}. Skipping...", app_id);
                        None
                    },
                    Err(_) => {
                        println!("An error has occurred attempting to download {}.  Retry #1...", app_id);
                        match tokio_dl_stream_to_disk::download_and_return_sha256sum(&download_url, &Path::new(outpath), &fname).await {
                            Ok(sha256sum) => Some(sha256sum),
                            Err(_) => {
                                println!("An error has occurred attempting to download {}.  Retry #2...", app_id);
                                match tokio_dl_stream_to_disk::download_and_return_sha256sum(&download_url, &Path::new(outpath), &fname).await {
                                    Ok(sha256sum) => Some(sha256sum),
                                    Err(_) => {
                                        println!("An error has occurred attempting to download {}. Skipping...", app_id);
                                        None
                                    }
                                }
                            }
                        }
                    }
                };
                if let Some(sha256sum) = sha256sum {
                    if sha256sum == hash {
                        println!("{} downloaded successfully!", app_id);
                    } else {
                        println!("{} downloaded, but the sha256sum does not match the one signed by F-Droid. Proceed with caution.", app_id);
                    }
                }
            }
        })
    ).buffer_unordered(parallel).collect::<Vec<()>>().await;
}

fn parse_json(index: Value, app_ids: Vec<String>) -> Result<(Vec<(String, String, Vec<u8>)>, String), FDroidError> { //(Vec<(String, String)>, String), FDroidError> {
    let index_map = index.as_object().ok_or(FDroidError::Dummy)?;
    let repo_address = index_map
        .get("repo").ok_or(FDroidError::Dummy)?
        .get("address").ok_or(FDroidError::Dummy)?
        .as_str().ok_or(FDroidError::Dummy)?;

    let packages = index_map
        .get("packages").ok_or(FDroidError::Dummy)?
        .as_object().ok_or(FDroidError::Dummy)?;

    let apps: Vec<(String, String, Vec<u8>)> = app_ids.into_iter().map(|app_id| {
        if packages.contains_key(&app_id) {
            let app_array_value = packages.get(&app_id).unwrap();
            if app_array_value.is_array() {
                let app_array = app_array_value.as_array().unwrap();
                if app_array.len() >= 1 && app_array[0].is_object() {
                    let app = app_array[0].as_object().unwrap();
                    if app.contains_key("apkName") && app.contains_key("hash") {
                        let filename_value = app.get("apkName").unwrap();
                        let hash_value = app.get("hash").unwrap();
                        if filename_value.is_string() && hash_value.is_string() {
                            let filename = filename_value.as_str().unwrap().to_string();
                            if let Ok(hash) = hex::decode(hash_value.as_str().unwrap().to_string()) {
                                return Some((app_id.to_string(), filename, hash));
                            }
                        }
                    }
                }
            }
        } else {
            println!("Could not find {} in package list. Skipping...", app_id);
        }
        None
    }).filter_map(|i| i).collect();

    Ok((apps, repo_address.to_string()))
}

fn verify_and_return_index(dir: &TempDir, files: &Vec<String>) -> Result<Value, Box<dyn Error>> {
    println!("Verifying...");
    let re = Regex::new(consts::FDROID_SIGNATURE_BLOCK_FILE_REGEX).unwrap();
    let cert_file = {
        let mut cert_files = vec![];
        for file in files {
            if re.is_match(&file) {
                cert_files.push(file.clone());
            }
        }
        if cert_files.len() < 1 {
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
        let signed_file_manifest_sha1sum = base64::decode(match signed_file_regex.captures(&signed_file_string) {
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
        let manifest_file_index_sha1sum = base64::decode(match manifest_file_regex.captures(&manifest_file_string) {
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

    Ok(serde_json::from_str(std::str::from_utf8(&index_file_data)?)?)
}

fn get_certificate_and_signer_info_from_signature_block_file(signature_block_file: PathBuf) -> Result<(CapturedX509Certificate, SignerInfo), Box<dyn Error>> {
    let bytes = fs::read(signature_block_file).unwrap();
    match SignedData::parse_ber(&bytes) {
        Ok(signed_data) => {
            let certificates: Vec<&CapturedX509Certificate> = signed_data.certificates().collect();
            if certificates.len() > 1 {
                return Err(Box::new(SimpleError::new("Too many certificates provided.")));
            }
            if certificates.len() < 1 {
                return Err(Box::new(SimpleError::new("No certificate provided.")));
            }
            let signatories: Vec<&SignerInfo> = signed_data.signers().collect();
            if signatories.len() > 1 {
                return Err(Box::new(SimpleError::new("Too many signatories provided.")));
            }
            if signatories.len() < 1 {
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
