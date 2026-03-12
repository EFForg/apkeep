# `apkeep` - A command-line tool for downloading APK files

## Installation

Precompiled binaries for `apkeep` are available for download [here](https://github.com/EFForg/apkeep/releases).

### Cargo

To install from `crates.io`, simply [install rust](https://www.rust-lang.org/tools/install) and run

```shell
cargo install apkeep
```

Or to install from the latest commit in our repository, run

```shell
cargo install --git https://github.com/EFForg/apkeep.git
```

### Homebrew

To install from Homebrew, run

```shell
brew install apkeep
```

If using on an Android platform, first install [`termux`](https://termux.org/). Upgrade to the latest packages with `pkg update`, then install the `apkeep` precompiled binary as described above or run `pkg install apkeep` to install from the `termux` repository.

Docker images are also available through the GitHub Container Registry. Aside from using a specific release version, the following floating tags are available:

- stable: tracks the latest stable release (recommended)
- latest: tracks the latest release, including pre-releases
- edge: tracks the latest commit

## Usage

See [`USAGE`](https://github.com/EFForg/apkeep/blob/master/USAGE).

## Examples

The simplest example downloads a single APK to the current directory:

```shell
apkeep -a com.instagram.android .
```

This downloads from the default source, APKPure, which requires no credentials. For more APKPure usage examples, such as specifying a package architecture, refer to the [`USAGE-apkpure.md`](USAGE-apkpure.md) document.

To download directly from the Google Play Store, you must first [obtain an AAS token](USAGE-google-play.md). Then,

```shell
apkeep -a com.instagram.android -d google-play -e 'someone@gmail.com' -t aas_token .
```

For more Google Play usage examples, such as specifying a device configuration, timezone, or locale, refer to the [`USAGE-google-play.md`](USAGE-google-play.md) document.

To download from the F-Droid open source repository:

```shell
apkeep -a org.mozilla.fennec_fdroid -d f-droid .
```

For more F-Droid usage examples, such as downloading from F-Droid mirrors or other F-Droid repositories, refer to the [`USAGE-fdroid.md`](USAGE-fdroid.md) document.

Or, to download from the Huawei AppGallery:

```shell
apkeep -a com.elysiumlabs.newsbytes -d huawei-app-gallery .
```

To download a specific version of an APK (possible for APKPure or F-Droid), use the `@version` convention:

```shell
apkeep -a com.instagram.android@1.2.3 .
```

Or, to list available versions, use `-l`:

```shell
apkeep -l -a org.mozilla.fennec_fdroid -d f-droid
```

Refer to [`USAGE`](https://github.com/EFForg/apkeep/blob/master/USAGE) to download multiple APKs in a single run.

All the above examples work in Docker with minimal changes. For example, to download a single APK to your chosen output directory:

```shell
docker run --rm -v output_path:/output ghcr.io/efforg/apkeep:stable -a com.instagram.android /output
```

## Specify a CSV file or individual app ID

You can either specify a CSV file listing the apps to download, or an individual app ID. If you specify a CSV file and the first column does not contain the app ID, you must also use the `--field` option. If you have a simple file with one app ID per line, treat it as a CSV with a single field.

## Download Sources

You can use this tool to download from the following sources:

* The Google Play Store (`-d google-play`), given an email address and AAS token
* APKPure (`-d apk-pure`), a third-party site hosting APKs available on the Play Store
* F-Droid (`-d f-droid`), a repository for free and open-source Android apps. `apkeep` verifies that F-Droid maintainers signed these APKs, and alerts the user if it downloaded but could not verify an APK
* The Huawei AppGallery (`-d huawei-app-gallery`), an app store popular in China

## Usage Note

Users should not use app lists or choose parallel APK fetches often in order to not place unreasonable or disproportionately large load on the app distributor's infrastructure.

When using the Google Play Store as the download source, consider the following:

* Google may terminate your Google account for Terms of Service violations. Read their [Terms of Service](https://play.google.com/about/play-terms/index.html), avoid violating it, and choose an account where this outcome is acceptable.
* The tool cannot download paid or DRM apps.
* Using Tor will make downloads far more likely to fail.

License: MIT
