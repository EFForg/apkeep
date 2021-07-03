<!--- `README.md` is automatically generated from the rustdoc using [`cargo-readme`](https://crates.io/crates/cargo-readme). -->
# `apk-downloader` - A command-line tool for downloading APK files from various sources

[![crates.io](https://img.shields.io/crates/v/apk-downloader.svg)](https://crates.io/crates/apk-downloader)
[![Documentation](https://docs.rs/apk-downloader/badge.svg)](https://docs.rs/apk-downloader)
[![MIT licensed](https://img.shields.io/crates/l/apk-downloader.svg)](./LICENSE)

## Usage

See [`USAGE`](https://github.com/Hainish/apk-downloader/blob/master/USAGE).

## List Sources

A few distinct lists of APKs are used.  AndroidRank compiles the most popular apps available on
the Google Play Store.  You can also specify a CSV file which lists the apps to download.  If
you have a simple file with one app ID per line, you can just treat it as a CSV with a single
field.

## Download Sources

You can use this tool to download from a few distinct sources.

* The Google Play Store, given a username and password.
* APKPure, a third-party site hosting APKs available on the Play Store.  You must be running
an instance of the ChromeDriver for this to work, since a headless browser is used.
either from the Google Play Store directly, given a username

License: MIT
