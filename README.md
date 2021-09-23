<!--- `README.md` is automatically generated from the rustdoc using [`cargo-readme`](https://crates.io/crates/cargo-readme). -->
# `apkeep` - A command-line tool for downloading APK files from various sources

[![crates.io](https://img.shields.io/crates/v/apkeep.svg)](https://crates.io/crates/apkeep)
[![MIT licensed](https://img.shields.io/crates/l/apkeep.svg)](./LICENSE)

![apkeep logo](logo.png)

## Installation

Precompiled binaries for `apkeep` on various platforms can be downloaded
[here](https://github.com/EFForg/apkeep/releases).

To install from `crates.io`, simply [install rust](https://www.rust-lang.org/tools/install) and
run

```shell
cargo install apkeep
```

Or to install from the latest commit in our repository, run

```shell
cargo install --git https://github.com/EFForg/apkeep.git
```

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
apkeep -a com.instagram.android -d GooglePlay -u 'someone@gmail.com' -p somepass .
```

Or, to download from the F-Droid open source repository:

```shell
apkeep -a org.mozilla.fennec_fdroid -d FDroid .
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
* F-Droid, a repository for free and open-source Android apps. `apkeep` verifies that these
APKs are signed by the F-Droid maintainers, and alerts the user if an APK was downloaded but
could not be verified

## Usage Note

Users should not use app lists or choose so many parallel APK fetches as to place unreasonable
or disproportionately large load on the infrastructure of the app distributor.

When using with the Google Play Store as the download source, a few considerations should be
made:

* Google may terminate your Google account based on Terms of Service violations.  Read their
[Terms of Service](https://play.google.com/about/play-terms/index.html), avoid violating it,
and choose an account where this outcome is acceptable.
* The session works with a specific "device profile," so only APKs available for that device,
location, language, etc. will be available.  In time we hope to make this profile configurable.
* Paid and DRM apps will not be available.
* Using Tor will make it a lot more likely that the download will fail.

License: MIT
