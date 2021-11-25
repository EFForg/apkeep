pub const APKPURE_DETAILS_URL_FORMAT: &'static str = "https://api.pureapk.com/m/v3/app/detail?hl=en-US&package_name=";
pub const APKPURE_VERSIONS_URL_FORMAT: &'static str = "https://api.pureapk.com/m/v3/cms/app_version?hl=en-US&package_name=";
pub const APKPURE_DOWNLOAD_URL_REGEX: &'static str = r"APKJ..(https?://(www\.)?[-a-zA-Z0-9@:%._\+~#=]{1,256}\.[a-zA-Z0-9()]{1,6}\b([-a-zA-Z0-9()@:%_\+.~#?&//=]*))";
pub const FDROID_INDEX_URL: &'static str = "https://f-droid.org/repo/index-v1.jar";
pub const FDROID_INDEX_FINGERPRINT: &'static [u8] = &[67, 35, 141, 81, 44, 30, 94, 178, 214, 86, 159, 74, 58, 251, 245, 82, 52, 24, 184, 46, 10, 62, 209, 85, 39, 112, 171, 185, 169, 201, 204, 171];
pub const FDROID_SIGNATURE_BLOCK_FILE_REGEX: &'static str = r"^META-INF/.*\.(DSA|EC|RSA)$";
