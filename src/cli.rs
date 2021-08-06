use clap::{App, Arg};

arg_enum! {
    pub enum DownloadSource {
        APKPure,
        GooglePlay,
    }
}

pub fn app() -> App<'static, 'static> {
    App::new("APK Downloader")
        .author("William Budington <bill@eff.org>")
        .about("Downloads APKs from various sources")
        .usage("apk-downloader <-a app_id | -c csv [-f field]> [-d download_source] [-p parallel] OUTPATH")
        .arg(
            Arg::with_name("app_id")
                .help("Provide the ID of an app directly (e.g. com.instagram.android)")
                .short("a")
                .long("app-id")
                .takes_value(true))
        .arg(
            Arg::with_name("csv")
                .help("CSV file to use")
                .short("c")
                .long("csv")
                .takes_value(true)
                .conflicts_with("app_id")
                .required_unless("app_id"))
        .arg(
            Arg::with_name("field")
                .help("CSV field containing app IDs (used only if CSV is specified)")
                .short("f")
                .long("field")
                .takes_value(true)
                .default_value("1"))
        .arg(
            Arg::with_name("download_source")
                .help("Where to download the APKs from")
                .short("d")
                .long("download-source")
                .default_value("APKPure")
                .takes_value(true)
                .possible_values(&DownloadSource::variants())
                .required(false))
        .arg(
            Arg::with_name("google_username")
                .help("Google Username (required if download source is Google Play)")
                .short("u")
                .long("username")
                .takes_value(true)
                .required_if("download_source", "GooglePlay"))
        .arg(
            Arg::with_name("google_password")
                .help("Google App Password (required if download source is Google Play)")
                .short("p")
                .long("password")
                .takes_value(true)
                .required_if("download_source", "GooglePlay"))
        .arg(
            Arg::with_name("sleep_duration")
                .help("Sleep duration (in ms) before download requests")
                .short("s")
                .long("sleep-duration")
                .takes_value(true)
                .default_value("0"))
        .arg(
            Arg::with_name("parallel")
                .help("The number of parallel APK fetches to run at a time")
                .short("r")
                .long("parallel")
                .takes_value(true)
                .default_value("4")
                .required(false))
        .arg(Arg::with_name("OUTPATH")
            .help("Path to store output files")
            .required(true)
            .index(1))
}
