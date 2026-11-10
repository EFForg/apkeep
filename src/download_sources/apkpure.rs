use std::cell::RefCell;
use std::collections::{HashMap, HashSet};
use std::ops::Deref;
use std::path::Path;
use std::rc::Rc;

use futures_util::StreamExt;
use indicatif::MultiProgress;
use regex::Regex;
use reqwest::header::{HeaderMap, HeaderValue, CONTENT_RANGE, RANGE};
use reqwest::{Url, Response};
use serde_json::json;
use tokio_dl_stream_to_disk::{AsyncDownload, error::ErrorKind as TDSTDErrorKind};
use tokio::time::{sleep, Duration as TokioDuration};

use crate::util::{OutputFormat, progress_bar::progress_wrapper};

fn http_headers(options: &HashMap<&str, &str>) -> HeaderMap {
    let mut headers = HeaderMap::new();
    headers.insert("x-cv", HeaderValue::from_static("3172501"));
    headers.insert("x-sv", HeaderValue::from_static("29"));
    let arch = match options.get("arch"){
        Some(arch) => {
            arch.split(";").collect::<Vec<_>>().join(",")
        },
        None => "arm64-v8a,armeabi-v7a,armeabi,x86,x86_64".to_string()
    };
    match HeaderValue::from_str(&arch) {
        Ok(abis_header) => {
            headers.insert(
                "x-abis",
                abis_header
            );
        },
        Err(_) => {
            println!("Invalid options specified, excluding device arch specification.");
        }
    }
    headers.insert("x-gp", HeaderValue::from_static("1"));
    headers
}

pub async fn download_apps(
    apps: Vec<(String, Option<String>)>,
    parallel: usize,
    sleep_duration: u64,
    outpath: &Path,
    options: HashMap<&str, &str>,
) {
    match options.get("acknowledge_dangers") {
        Some(val) if *val == "1" || val.to_lowercase() == "true" => {},
        _ => {
            eprintln!("The dangers of using this download source must be explicitly acknowledged: https://github.com/EFForg/apkeep/blob/master/USAGE-apkpure.md");
            std::process::exit(1);
        }
    }

    let mp = Rc::new(MultiProgress::new());
    let http_client = Rc::new(reqwest::Client::new());
    let app_arch = options.get("arch").cloned();
    let headers = http_headers(&options);
    let re = Rc::new(Regex::new(crate::consts::APKPURE_DOWNLOAD_URL_REGEX).unwrap());

    futures_util::stream::iter(
        apps.into_iter().map(|app| {
            let (app_id, app_version) = app;
            let http_client = Rc::clone(&http_client);
            let re = Rc::clone(&re);
            let headers = headers.clone();
            let mp = Rc::clone(&mp);
            let mp_log = Rc::clone(&mp);
            async move {
                let app_string = match (&app_version, app_arch) {
                    (None, None) => {
                        mp_log.suspend(|| println!("Downloading {} from APKPure...", app_id));
                        app_id.to_string()
                    },
                    (None, Some(ref arch)) => {
                        mp_log.suspend(|| println!("Downloading {} arch {} from APKPure...", app_id, arch));
                        format!("{}@{}", app_id, arch)
                    },
                    (Some(ref version), None) => {
                        mp_log.suspend(|| println!("Downloading {} version {} from APKPure...", app_id, version));
                        format!("{}@{}", app_id, version)
                    },
                    (Some(ref version), Some(ref arch)) => {
                        mp_log.suspend(|| println!("Downloading {} version {} arch {} from APKPure...", app_id, version, arch));
                        format!("{}@{}@{}", app_id, version, arch)
                    },
                };
                if sleep_duration > 0 {
                    sleep(TokioDuration::from_millis(sleep_duration)).await;
                }
                let versions_url = Url::parse(&format!("{}{}", crate::consts::APKPURE_VERSIONS_URL_FORMAT, app_id)).unwrap();
                let versions_response = http_client
                    .get(versions_url)
                    .headers(headers)
                    .send().await.unwrap();
                let variant_selector = app_arch.map(|arch| (http_client.as_ref(), arch, app_version.as_deref()));
                if let Some(app_version) = &app_version {
                    let regex_string = format!("[[:^digit:]]{}:(?s:.)+?{}", regex::escape(app_version), crate::consts::APKPURE_DOWNLOAD_URL_REGEX);
                    let re = Regex::new(&regex_string).unwrap();
                    download_from_response(versions_response, Box::new(Box::new(re)), app_string, outpath, mp, variant_selector).await;
                } else {
                    download_from_response(versions_response, Box::new(re), app_string, outpath, mp, variant_selector).await;
                }
            }
        })
    ).buffer_unordered(parallel).collect::<Vec<()>>().await;
}

async fn download_from_response(response: Response, re: Box<dyn Deref<Target=Regex>>, app_string: String, outpath: &Path, mp: Rc<MultiProgress>, variant_selector: Option<(&reqwest::Client, &str, Option<&str>)>) {
    let mp_log = Rc::clone(&mp);
    let mp = Rc::clone(&mp);
    match response.status() {
        reqwest::StatusCode::OK => {
            let body = response.text().await.unwrap();
            // Several variants of the same version may be available.  If an architecture was
            // specified, pick a variant which actually contains it rather than the first one.
            let variant = match variant_selector {
                Some((http_client, arch, app_version)) => {
                    match select_variant(http_client, &body, arch, app_version).await {
                        VariantSelection::Found(apk_xapk, download_url) => Some((apk_xapk, download_url)),
                        VariantSelection::Unknown => None,
                        VariantSelection::NotAvailable => {
                            mp_log.println(format!("Could not find a download containing the specified arch for {}. Skipping...", app_string)).unwrap();
                            return;
                        }
                    }
                },
                None => None,
            };
            let variant = variant.or_else(|| match re.captures(&body) {
                Some(caps) if caps.len() >= 2 => Some((caps.get(1).unwrap().as_str().to_string(), caps.get(2).unwrap().as_str().to_string())),
                _ => None,
            });
            match variant {
                Some((apk_xapk, download_url)) => {
                    let apk_xapk = apk_xapk.as_str();
                    let download_url = download_url.as_str();
                    let fname = match apk_xapk {
                        "XAPKJ" => format!("{}.xapk", app_string),
                        _ => format!("{}.apk", app_string),
                    };

                    match AsyncDownload::new(download_url, Path::new(outpath), &fname).get().await {
                        Ok(mut dl) => {
                            let length = dl.length();
                            let cb = match length {
                                Some(length) => Some(progress_wrapper(mp)(fname.clone(), length)),
                                None => None,
                            };

                            match dl.download(&cb).await {
                                Ok(_) => mp_log.suspend(|| println!("{} downloaded successfully!", app_string)),
                                Err(err) if matches!(err.kind(), TDSTDErrorKind::FileExists) => {
                                    mp_log.println(format!("File already exists for {}. Skipping...", app_string)).unwrap();
                                },
                                Err(err) if matches!(err.kind(), TDSTDErrorKind::PermissionDenied) => {
                                    mp_log.println(format!("Permission denied when attempting to write file for {}. Skipping...", app_string)).unwrap();
                                },
                                Err(_) => {
                                    mp_log.println(format!("An error has occurred attempting to download {}.  Retry #1...", app_string)).unwrap();
                                    match AsyncDownload::new(download_url, Path::new(outpath), &fname).download(&cb).await {
                                        Ok(_) => mp_log.suspend(|| println!("{} downloaded successfully!", app_string)),
                                        Err(_) => {
                                            mp_log.println(format!("An error has occurred attempting to download {}.  Retry #2...", app_string)).unwrap();
                                            match AsyncDownload::new(download_url, Path::new(outpath), &fname).download(&cb).await {
                                                Ok(_) => mp_log.suspend(|| println!("{} downloaded successfully!", app_string)),
                                                Err(_) => {
                                                    mp_log.println(format!("An error has occurred attempting to download {}. Skipping...", app_string)).unwrap();
                                                }
                                            }
                                        }
                                    }
                                }
                            }
                        },
                        Err(_) => {
                            mp_log.println(format!("Invalid response for {}. Skipping...", app_string)).unwrap();
                        }
                    }
                },
                _ => {
                    mp_log.println(format!("Could not get download URL for {}. Skipping...", app_string)).unwrap();
                }
            }

        },
        _ => {
            mp_log.println(format!("Invalid app response for {}. Skipping...", app_string)).unwrap();
        }
    }
}

enum VariantSelection {
    Found(String, String),
    NotAvailable,
    Unknown,
}

// Choose which of the variants available for a version to download, given the architectures
// specified, in order of preference.  The API returns every variant compatible with a device of
// the requested architecture (e.g. `armeabi-v7a` variants for `arm64-v8a`) without indicating the
// architectures they contain, so we determine this from the files themselves.
async fn select_variant(http_client: &reqwest::Client, body: &str, arch: &str, app_version: Option<&str>) -> VariantSelection {
    let version_re = Regex::new(crate::consts::APKPURE_VERSION_REGEX).unwrap();
    let url_re = Regex::new(crate::consts::APKPURE_DOWNLOAD_URL_REGEX).unwrap();
    let versions: Vec<(usize, usize, &str)> = version_re.captures_iter(body).map(|caps| {
        let version = caps.get(1).unwrap();
        (caps.get(0).unwrap().start(), caps.get(0).unwrap().end(), version.as_str())
    }).collect();
    let app_version = match app_version.or(versions.first().map(|version| version.2)) {
        Some(app_version) => app_version,
        None => return VariantSelection::Unknown,
    };

    let mut variants: Vec<(String, String)> = Vec::new();
    for (i, (_, end, version)) in versions.iter().enumerate() {
        if *version != app_version {
            continue;
        }
        let next_start = versions.get(i + 1).map_or(body.len(), |next| next.0);
        if let Some(caps) = url_re.captures(&body[*end..next_start]) {
            let variant = (caps.get(1).unwrap().as_str().to_string(), caps.get(2).unwrap().as_str().to_string());
            if !variants.contains(&variant) {
                variants.push(variant);
            }
        }
    }
    if variants.is_empty() {
        return VariantSelection::Unknown;
    }

    let mut variants_abis = Vec::new();
    for (_, download_url) in &variants {
        variants_abis.push(remote_abis(http_client, download_url).await);
    }
    for abi in arch.split(";").map(|abi| abi.trim()) {
        for (variant, abis) in variants.iter().zip(&variants_abis) {
            match abis {
                // a variant without native code runs on any architecture
                Some(abis) if abis.is_empty() || abis.contains(abi) => {
                    return VariantSelection::Found(variant.0.clone(), variant.1.clone());
                },
                _ => {}
            }
        }
    }
    if variants_abis.iter().any(|abis| abis.is_none()) {
        VariantSelection::Unknown
    } else {
        VariantSelection::NotAvailable
    }
}

struct RemoteZipEntry {
    name: String,
    stored: bool,
    compressed_size: u64,
    header_offset: u64,
}

async fn fetch_range(http_client: &reqwest::Client, url: &str, range: String) -> Option<(Vec<u8>, u64)> {
    let response = http_client.get(url).header(RANGE, format!("bytes={}", range)).send().await.ok()?;
    if response.status() != reqwest::StatusCode::PARTIAL_CONTENT {
        return None;
    }
    let total = response.headers().get(CONTENT_RANGE)?.to_str().ok()?.rsplit('/').next()?.parse::<u64>().ok()?;
    Some((response.bytes().await.ok()?.to_vec(), total))
}

// List the entries of a zip file by fetching only its central directory.  The zip file may itself
// be stored within the remote file, at `offset` and of length `size`.
async fn remote_zip_entries(http_client: &reqwest::Client, url: &str, offset: u64, size: Option<u64>) -> Option<Vec<RemoteZipEntry>> {
    const TAIL_SIZE: u64 = 65536 + 22;
    let (tail, size) = match size {
        Some(size) => {
            let tail_size = TAIL_SIZE.min(size);
            let (tail, _) = fetch_range(http_client, url, format!("{}-{}", offset + size - tail_size, offset + size - 1)).await?;
            (tail, size)
        },
        None => fetch_range(http_client, url, format!("-{}", TAIL_SIZE)).await?,
    };
    let eocd = tail.windows(4).rposition(|window| window == b"PK\x05\x06")?;
    let eocd = tail.get(eocd..eocd + 22)?;
    let directory_size = u32::from_le_bytes(eocd[12..16].try_into().ok()?) as u64;
    let directory_offset = u32::from_le_bytes(eocd[16..20].try_into().ok()?) as u64;
    if directory_size == u32::MAX as u64 || directory_offset == u32::MAX as u64 || directory_offset + directory_size > size {
        return None;
    }
    let tail_offset = size - tail.len() as u64;
    let directory = if directory_offset >= tail_offset {
        let start = (directory_offset - tail_offset) as usize;
        tail.get(start..start + directory_size as usize)?.to_vec()
    } else {
        fetch_range(http_client, url, format!("{}-{}", offset + directory_offset, offset + directory_offset + directory_size - 1)).await?.0
    };

    let mut entries = Vec::new();
    let mut pos = 0;
    while let Some(header) = directory.get(pos..pos + 46) {
        if &header[0..4] != b"PK\x01\x02" {
            break;
        }
        let field_u16 = |at: usize| u16::from_le_bytes([header[at], header[at + 1]]) as usize;
        let field_u32 = |at: usize| u32::from_le_bytes([header[at], header[at + 1], header[at + 2], header[at + 3]]) as u64;
        let name_len = field_u16(28);
        entries.push(RemoteZipEntry {
            name: String::from_utf8_lossy(directory.get(pos + 46..pos + 46 + name_len)?).to_string(),
            stored: field_u16(10) == 0,
            compressed_size: field_u32(20),
            header_offset: field_u32(42),
        });
        pos += 46 + name_len + field_u16(30) + field_u16(32);
    }
    Some(entries)
}

fn lib_abis(entries: &[RemoteZipEntry], abis: &mut HashSet<String>) {
    for entry in entries {
        if let Some(abi) = entry.name.strip_prefix("lib/").and_then(|path| path.split_once('/')).map(|(abi, _)| abi) {
            abis.insert(abi.to_string());
        }
    }
}

// Determine the architectures an APK or XAPK contains native code for, without downloading it.
// Returns an empty set if it does not contain any native code, or `None` if this can not be
// determined.
async fn remote_abis(http_client: &reqwest::Client, url: &str) -> Option<HashSet<String>> {
    let entries = remote_zip_entries(http_client, url, 0, None).await?;
    let mut abis = HashSet::new();
    lib_abis(&entries, &mut abis);

    // An XAPK contains the native code either in split APKs named after the architecture, or
    // within the base APK
    let apks: Vec<&RemoteZipEntry> = entries.iter().filter(|entry| entry.name.ends_with(".apk") && !entry.name.contains('/')).collect();
    for apk in &apks {
        if let Some(abi) = apk.name.strip_suffix(".apk").and_then(|name| name.strip_prefix("config.").or(name.strip_prefix("split_config."))) {
            if let Some(abi) = crate::consts::APKPURE_SPLIT_ABIS.iter().find(|split_abi| split_abi.0 == abi) {
                abis.insert(abi.1.to_string());
            }
        }
    }
    if abis.is_empty() {
        for apk in apks.iter().filter(|apk| !apk.name.starts_with("config.") && !apk.name.starts_with("split_config.")) {
            if !apk.stored {
                return None;
            }
            let (header, _) = fetch_range(http_client, url, format!("{}-{}", apk.header_offset, apk.header_offset + 29)).await?;
            if header.len() != 30 || &header[0..4] != b"PK\x03\x04" {
                return None;
            }
            let data_offset = apk.header_offset + 30 + u16::from_le_bytes([header[26], header[27]]) as u64 + u16::from_le_bytes([header[28], header[29]]) as u64;
            lib_abis(&remote_zip_entries(http_client, url, data_offset, Some(apk.compressed_size)).await?, &mut abis);
        }
    }
    Some(abis)
}

pub async fn list_versions(apps: Vec<(String, Option<String>)>, options: HashMap<&str, &str>) {
    let http_client = Rc::new(reqwest::Client::new());
    let re = Rc::new(Regex::new(crate::consts::APKPURE_VERSION_REGEX).unwrap());
    let headers = http_headers(&options);
    let output_format = match options.get("output_format") {
        Some(val) if val.to_lowercase() == "json" => OutputFormat::Json,
        _ => OutputFormat::Plaintext,
    };
    let json_root = Rc::new(RefCell::new(match output_format {
        OutputFormat::Json => Some(HashMap::new()),
        _ => None,
    }));

    for app in apps {
        let (app_id, _) = app;
        let http_client = Rc::clone(&http_client);
        let re = Rc::clone(&re);
        let json_root = Rc::clone(&json_root);
        let output_format = output_format.clone();
        let headers = headers.clone();
        async move {
            if output_format.is_plaintext() {
                println!("Versions available for {} on APKPure:", app_id);
            }
            let versions_url = Url::parse(&format!("{}{}", crate::consts::APKPURE_VERSIONS_URL_FORMAT, app_id)).unwrap();
            let versions_response = http_client
                .get(versions_url)
                .headers(headers)
                .send().await.unwrap();

            match versions_response.status() {
                reqwest::StatusCode::OK => {
                    let body = versions_response.text().await.unwrap();
                    let mut versions = HashSet::new();
                    for caps in re.captures_iter(&body) {
                        if caps.len() >= 2 {
                            versions.insert(caps.get(1).unwrap().as_str().to_string());
                        }
                    }
                    let mut versions = versions.drain().collect::<Vec<String>>();
                    versions.sort();
                    match output_format {
                        OutputFormat::Plaintext => {
                            println!("| {}", versions.join(", "));
                        },
                        OutputFormat::Json => {
                            let mut app_root: HashMap<String, Vec<HashMap<String, String>>> = HashMap::new();
                            app_root.insert("available_versions".to_string(), versions.into_iter().map(|v| {
                                let mut version_map = HashMap::new();
                                version_map.insert("version".to_string(), v);
                                version_map
                            }).collect());
                            json_root.borrow_mut().as_mut().unwrap().insert(app_id.to_string(), json!(app_root));
                        },
                    }
                }
                _ => {
                    match output_format {
                        OutputFormat::Plaintext => {
                            eprintln!("| Invalid app response for {}. Skipping...", app_id);
                        },
                        OutputFormat::Json => {
                            let mut app_root = HashMap::new();
                            app_root.insert("error".to_string(), "Invalid app response.".to_string());
                            json_root.borrow_mut().as_mut().unwrap().insert(app_id.to_string(), json!(app_root));
                        },
                    }
                }
            }
        }.await;
    }
    if output_format.is_json() {
        println!("{{\"source\":\"APKPure\",\"apps\":{}}}", json!(*json_root));
    };
}
