use std::fs::File;
use std::io::Write;

include!("src/cli.rs");

fn main() {
    let mut file = File::create("USAGE").unwrap();
    let help = app().render_help();
    file.write_all(help.to_string().as_bytes()).unwrap();
}
