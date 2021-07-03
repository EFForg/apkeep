#[macro_use]
extern crate clap;

use std::fs::File;

include!("src/cli.rs");

fn main() {
    let mut file = File::create("USAGE").unwrap();
    app().write_help(&mut file).unwrap();
}
