<!--- `README.md` is automatically generated from the rustdoc using [`cargo-readme`](https://crates.io/crates/cargo-readme). -->
# `apkeep` - A command-line tool for downloading APK files from various sources

[![crates.io](https://img.shields.io/crates/v/apkeep.svg)](https://crates.io/crates/apkeep)
[![Documentation](https://docs.rs/apkeep/badge.svg)](https://docs.rs/apkeep)
[![MIT licensed](https://img.shields.io/crates/l/apkeep.svg)](./LICENSE)

## Usage

See [`USAGE`](https://github.com/EFForg/apkeep/blob/master/USAGE).

## Examples

The simplest example is to download a single APK to the current directory:

```shell
apkeep -a com.instagram.android .
```

This downloads from the default source, `APKPure`, which does not require credentials.  To
download directly from the google play store:

```shell
apk-downloader -a com.instagram.android -d GooglePlay -u 'someone@gmail.com' -p somepass .
```

Refer to [`USAGE`](https://github.com/EFForg/apkeep/blob/master/USAGE) to download multiple
APKs in a single run.

## Specify a CSV file or individual app ID

You can either specify a CSV file which lists the apps to download, or an individual app ID.
If you specify a CSV file and the app ID is not specified by the first column, you'll have to
use the --field option as well.  If you have a simple file with one app ID per line, you can
just treat it as a CSV with a single field.

## Download Sources

You can use this tool to download from a few distinct sources.

* The Google Play Store, given a username and password
* APKPure, a third-party site hosting APKs available on the Play Store

## Usage Note

Users should not use app lists or choose so many parallel APK fetches as to place unreasonable
or disproportionately large load on the infrastructure of the app distributor.

License: MIT
