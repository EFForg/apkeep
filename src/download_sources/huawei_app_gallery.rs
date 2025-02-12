use std::path::Path;
use std::rc::Rc;

use futures_util::StreamExt;
use indicatif::MultiProgress;
use reqwest::header::{HeaderMap, HeaderValue};
use reqwest::{Url, Response};
use serde_json::Value;
use tokio_dl_stream_to_disk::{AsyncDownload, error::ErrorKind as TDSTDErrorKind};
use tokio::time::{sleep, Duration as TokioDuration};

use crate::util::progress_bar::progress_wrapper;

fn http_headers() -> HeaderMap {
    let mut headers = HeaderMap::new();
    headers.insert("User-Agent", HeaderValue::from_static("UpdateSDK##4.0.1.300##Android##Pixel 2##com.huawei.appmarket##12.0.1.301"));
    headers
}

fn client_api_body(app_id: &str) -> String {
    format!("agVersion=12.0.1&brand=Android&buildNumber=QQ2A.200405.005.2020.04.07.17&density=420&deviceSpecParams=%7B%22abis%22%3A%22arm64-v8a%2Carmeabi-v7a%2Carmeabi%22%2C%22deviceFeatures%22%3A%22U%2CP%2CB%2C0c%2Ce%2C0J%2Cp%2Ca%2Cb%2C04%2Cm%2Candroid.hardware.wifi.rtt%2Ccom.google.hardware.camera.easel%2Ccom.google.android.feature.PIXEL_2017_EXPERIENCE%2C08%2C03%2CC%2CS%2C0G%2Cq%2CL%2C2%2C6%2CY%2CZ%2C0M%2Candroid.hardware.vr.high_performance%2Cf%2C1%2C07%2C8%2C9%2Candroid.hardware.sensor.hifi_sensors%2CO%2CH%2Ccom.google.android.feature.TURBO_PRELOAD%2Candroid.hardware.vr.headtracking%2CW%2Cx%2CG%2Co%2C06%2C0N%2Ccom.google.android.feature.PIXEL_EXPERIENCE%2C3%2CR%2Cd%2CQ%2Cn%2Candroid.hardware.telephony.carrierlock%2Cy%2CT%2Ci%2Cr%2Cu%2Ccom.google.android.feature.WELLBEING%2Cl%2C4%2C0Q%2CN%2CM%2C01%2C09%2CV%2C7%2C5%2C0H%2Cg%2Cs%2Cc%2C0l%2Ct%2C0L%2C0W%2C0X%2Ck%2C00%2Ccom.google.android.feature.GOOGLE_EXPERIENCE%2Candroid.hardware.sensor.assist%2Candroid.hardware.audio.pro%2CK%2CE%2C02%2CI%2CJ%2Cj%2CD%2Ch%2Candroid.hardware.wifi.aware%2C05%2CX%2Cv%22%2C%22dpi%22%3A420%2C%22preferLan%22%3A%22en%22%7D&emuiApiLevel=0&firmwareVersion=10&getSafeGame=1&gmsSupport=0&hardwareType=0&harmonyApiLevel=0&harmonyDeviceType=&installCheck=0&isFullUpgrade=0&isUpdateSdk=1&locale=en_US&magicApiLevel=0&magicVer=&manufacturer=Google&mapleVer=0&method=client.updateCheck&odm=0&packageName=com.huawei.appmarket&phoneType=Pixel%202&pkgInfo=%7B%22params%22%3A%5B%7B%22isPre%22%3A0%2C%22maple%22%3A0%2C%22oldVersion%22%3A%221.0%22%2C%22package%22%3A%22{}%22%2C%22pkgMode%22%3A0%2C%22shellApkVer%22%3A0%2C%22targetSdkVersion%22%3A19%2C%22versionCode%22%3A1%7D%5D%7D&resolution=1080_1794&sdkVersion=4.0.1.300&serviceCountry=IE&serviceType=0&supportMaple=0&ts=1649970862661&ver=1.2&version=12.0.1.301&versionCode=120001301", app_id)
}

pub async fn download_apps(
    apps: Vec<(String, Option<String>)>,
    parallel: usize,
    sleep_duration: u64,
    outpath: &Path,
) {
    let http_client = Rc::new(reqwest::Client::new());
    let headers = http_headers();

    let mp = Rc::new(MultiProgress::new());
    futures_util::stream::iter(
        apps.into_iter().map(|app| {
            let (app_id, app_version) = app;
            let http_client = Rc::clone(&http_client);
            let headers = headers.clone();
            let mp = Rc::clone(&mp);
            let mp_log = Rc::clone(&mp);
            async move {
                if app_version.is_none() {
                    mp_log.println(format!("Downloading {}...", app_id)).unwrap();
                    if sleep_duration > 0 {
                        sleep(TokioDuration::from_millis(sleep_duration)).await;
                    }
                    let client_api_url = Url::parse(crate::consts::HUAWEI_APP_GALLERY_CLIENT_API_URL).unwrap();
                    let client_api_response = http_client
                        .post(client_api_url)
                        .body(client_api_body(&app_id))
                        .headers(headers)
                        .send().await.unwrap();
                    download_from_response(client_api_response, app_id.to_string(), outpath, mp).await;
                } else {
                    mp_log.println(format!("Specific versions can not be downloaded from Huawei AppGallery ({}@{}). Skipping...", app_id, app_version.unwrap())).unwrap();
                }
            }
        })
    ).buffer_unordered(parallel).collect::<Vec<()>>().await;
}

async fn download_from_response(response: Response, app_string: String, outpath: &Path, mp: Rc<MultiProgress>) {
    let mp_log = Rc::clone(&mp);
    let mp = Rc::clone(&mp);
    let fname = format!("{}.apk", app_string);
    match response.status() {
        reqwest::StatusCode::OK => {
            let body = response.text().await.unwrap();
            let response_value: Value = serde_json::from_str(&body).unwrap();
            if response_value.is_object() {
                let response_obj = response_value.as_object().unwrap();
                if response_obj.contains_key("list") {
                    let list_value = response_obj.get("list").unwrap();
                    if list_value.is_array() {
                        let list = list_value.as_array().unwrap();
                        if !list.is_empty() && list[0].is_object(){
                            let first_list_entry = list[0].as_object().unwrap();
                            if first_list_entry.contains_key("downurl") {
                                let downurl = first_list_entry.get("downurl").unwrap();
                                if downurl.is_string() {
                                    let download_url = downurl.as_str().unwrap();
                                    match AsyncDownload::new(download_url, Path::new(outpath), &fname).get().await {
                                        Ok(mut dl) => {
                                            let length = dl.length();
                                            let cb = match length {
                                                Some(length) => Some(progress_wrapper(mp)(fname.clone(), length)),
                                                None => None,
                                            };

                                            match dl.download(&cb).await {
                                                Ok(_) => mp_log.println(format!("{} downloaded successfully!", app_string)).unwrap(),
                                                Err(err) if matches!(err.kind(), TDSTDErrorKind::FileExists) => {
                                                    mp_log.println(format!("File already exists for {}. Skipping...", app_string)).unwrap();
                                                },
                                                Err(err) if matches!(err.kind(), TDSTDErrorKind::PermissionDenied) => {
                                                    mp_log.println(format!("Permission denied when attempting to write file for {}. Skipping...", app_string)).unwrap();
                                                },
                                                Err(_) => {
                                                    mp_log.println(format!("An error has occurred attempting to download {}.  Retry #1...", app_string)).unwrap();
                                                    match AsyncDownload::new(download_url, Path::new(outpath), &fname).download(&cb).await {
                                                        Ok(_) => mp_log.println(format!("{} downloaded successfully!", app_string)).unwrap(),
                                                        Err(_) => {
                                                            mp_log.println(format!("An error has occurred attempting to download {}.  Retry #2...", app_string)).unwrap();
                                                            match AsyncDownload::new(download_url, Path::new(outpath), &fname).download(&cb).await {
                                                                Ok(_) => mp_log.println(format!("{} downloaded successfully!", app_string)).unwrap(),
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
                                }
                            }
                        }
                    }
                }
            }
        },
        _ => {
            mp_log.println(format!("Invalid app response for {}. Skipping...", app_string)).unwrap();
        }
    }
}

pub async fn list_versions(apps: Vec<(String, Option<String>)>) {
    for app in apps {
        let (app_id, _) = app;
        println!("Versions available for {} on Huawei AppGallery:", app_id);
        println!("| Huawei AppGallery does not make old versions of apps available.");
    }
}
