# Changelog

All notable changes to this project will be documented in this file.

This project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [0.17.0] - 2024-10-11
- Added support for F-Droid entry point specification and new index versions
- Update dependencies

## [0.16.0] - 2024-04-04
- Support Google Play API v3 and document workflow for downloading via Google Play

## [0.15.0] - 2023-01-04
- Add progress bars to all download sources
- Update dependencies

## [0.14.1] - 2022-11-23
- Bugfix release: updating `zstd-sys` dependency, which fixes cross-compilation for Windows

## [0.14.0] - 2022-11-21
- Downloading split APKs downloads the base APK as well
- Switch to OpenSSL 3.0.7
- Update dependencies

## [0.13.0] - 2022-05-26
- Add support for downloading split APKs with `google-play`
- Add support for downloading additional files with `google-play`
- Use the appropriate filename extensions (`xapk` or `apk`) for `apkpure`

## [0.12.2] - 2022-05-18
- Android-only release: switch to OpenSSL 3.0.3 for `termux` releases

## [0.12.1] - 2022-05-11
- Android-only release: fix dependencies to ensure `openssl-1.1` is used

## [0.12.0] - 2022-05-05
- Add a default config file which allows users to store Google credentials
- Allow specifying a custom path to the above config file
- Prompt users for a Google username and password if none is found

## [0.11.0] - 2022-04-22
- Adding `huawei-app-gallery` as a download source

## [0.10.0] - 2022-03-17
### Added
- `options` command-line option to specify options specific to a download source
- With `options`, adding ability to download from a specific F-Droid repo or mirror
- With `options`, adding ability to specify a device configuration and additional options for Google Play
- Documenting `options`

## [0.9.0] - 2022-02-22
- Fix bug where another package is fetched for certain ids in APKPure
- Updated usage for download sources
- Dependency updates

## [0.8.0] - 2021-12-07
### Added
- Cacheing of F-Droid package index to local config directory

## [0.7.0] - 2021-11-29
### Added
- Ability to download versioned apps on APKPure and F-Droid, as well as making it possible in a separate field if you're using a CSV
- Ability to look up what versions are available on these sources using the -l flag
