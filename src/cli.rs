use clap::{value_parser, Command, Arg, ArgAction, ValueEnum, {builder::EnumValueParser}};

#[derive(Copy, Clone, PartialEq, Eq, ValueEnum)]
pub enum DownloadSource {
    APKPure,
    GooglePlay,
    FDroid,
    HuaweiAppGallery,
}

impl std::fmt::Display for DownloadSource {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        self.to_possible_value()
            .expect("no values are skipped")
            .get_name()
            .fmt(f)
    }
}

impl std::str::FromStr for DownloadSource {
    type Err = String;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        for variant in Self::value_variants() {
            if variant.to_possible_value().unwrap().matches(s, false) {
                return Ok(*variant);
            }
        }
        Err(format!("Invalid variant: {}", s))
    }
}

pub fn app() -> Command {
    Command::new("apkeep")
        .version(env!("CARGO_PKG_VERSION"))
        .author("William Budington <bill@eff.org>")
        .about("Downloads APKs from various sources")
        .override_usage("apkeep <-a app_id[@version] | -c csv [-f field] [-v version_field]> [-d download_source] [-r parallel] OUTPATH")
        .arg(
            Arg::new("app")
                .help("Provide the ID and optionally the version of an app directly (e.g. com.instagram.android)")
                .short('a')
                .long("app")
                .action(ArgAction::Set)
                .conflicts_with("csv")
                .required_unless_present("csv"),
        )
        .arg(
            Arg::new("csv")
                .help("CSV file to use")
                .short('c')
                .long("csv")
                .action(ArgAction::Set),
        )
        .arg(
            Arg::new("field")
                .help("CSV field containing app IDs (used only if CSV is specified)")
                .short('f')
                .long("field")
                .action(ArgAction::Set)
                .value_parser(value_parser!(usize))
                .default_value("1"),
        )
        .arg(
            Arg::new("version_field")
                .help("CSV field containing versions (used only if CSV is specified)")
                .short('v')
                .long("version-field")
                .action(ArgAction::Set)
                .value_parser(value_parser!(usize))
                .required(false),
        )
        .arg(
            Arg::new("list_versions")
                .help("List the versions available")
                .short('l')
                .long("list-versions")
                .action(ArgAction::SetTrue)
                .required(false),
        )
        .arg(
            Arg::new("download_source")
                .help("Where to download the APKs from")
                .short('d')
                .long("download-source")
                .default_value("apk-pure")
                .action(ArgAction::Set)
                .value_parser(EnumValueParser::<DownloadSource>::new())
                .required(false),
        )
        .arg(
            Arg::new("options")
                .help("A comma-separated list of additional options to pass to the download source")
                .short('o')
                .long("options")
                .action(ArgAction::Set)
                .required(false),
        )
        .arg(
            Arg::new("ini")
                .help("The path to an ini file which contains configuration data")
                .short('i')
                .long("ini")
                .action(ArgAction::Set)
                .required(false),
        )
        .arg(
            Arg::new("google_username")
                .help("Google Username (required if download source is Google Play)")
                .short('u')
                .long("username")
                .action(ArgAction::Set)
                .required_if_eq("download_source", "google-play"),
        )
        .arg(
            Arg::new("google_password")
                .help("Google App Password (required if download source is Google Play)")
                .short('p')
                .long("password")
                .action(ArgAction::Set)
                .required_if_eq("download_source", "google-play"),
        )
        .arg(
            Arg::new("sleep_duration")
                .help("Sleep duration (in ms) before download requests")
                .short('s')
                .long("sleep-duration")
                .action(ArgAction::Set)
                .value_parser(value_parser!(u64))
                .default_value("0"),
        )
        .arg(
            Arg::new("parallel")
                .help("The number of parallel APK fetches to run at a time")
                .short('r')
                .long("parallel")
                .action(ArgAction::Set)
                .value_parser(value_parser!(usize))
                .default_value("4")
                .required(false),
        )
        .arg(
            Arg::new("OUTPATH")
                .help("Path to store output files")
                .action(ArgAction::Set)
                .index(1),
        )
}
