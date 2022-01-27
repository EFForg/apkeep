# `apkeep` - A command-line tool for downloading APK files from various sources

To build `apkeep` from source, simply [install rust](https://www.rust-lang.org/tools/install) and in the repository path run

```shell
cargo build
```

If you wish to build the release version, run

```shell
cargo build --release
```

This will compile the binaries and put them in a new `target/` path.

To build and run all in one step, run

```shell
cargo run -- ARGS
```

or

```shell
cargo run --release -- ARGS
```
