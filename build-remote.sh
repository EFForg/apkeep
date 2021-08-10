#!/bin/bash
# Build the APK Downloader for release from a fresh Debian 10 x64 install

ssh -o 'StrictHostKeyChecking no' apk-downloader-compiler <<EOF
sudo dpkg --add-architecture armhf
sudo dpkg --add-architecture i386
sudo apt-get -y update
sudo apt-get -y dist-upgrade
sudo apt-get -y install git build-essential libssl-dev pkg-config
sudo apt-get -y install libc6-armhf-cross libc6-dev-armhf-cross gcc-arm-linux-gnueabihf libssl-dev:armhf
sudo apt-get -y install libc6-i386-cross libc6-dev-i386-cross gcc-i686-linux-gnu libssl-dev:i386
curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs > /tmp/get_rust.sh
bash /tmp/get_rust.sh -y
source ~/.cargo/env
rustup target install armv7-unknown-linux-gnueabihf i686-unknown-linux-gnu
git clone https://www.github.com/EFForg/apk-downloader.git
cd apk-downloader
export PKG_CONFIG_ALLOW_CROSS="true"
cargo build --release
export PKG_CONFIG_PATH="/usr/lib/arm-linux-gnueabihf/pkgconfig"
cargo build --release --target=armv7-unknown-linux-gnueabihf
export PKG_CONFIG_PATH="/usr/lib/i686-linux-gnu-gcc/pkgconfig"
cargo build --release --target=armv7-unknown-linux-gnueabihf
EOF

scp apk-downloader-compiler:~/apk-downloader/target/release/apk-downloader ./apk-downloader-x86_64-unknown-linux-gnu
scp apk-downloader-compiler:~/apk-downloader/target/armv7-unknown-linux-gnueabihf/release/apk-downloader ./apk-downloader-armv7-unknown-linux-gnueabihf
scp apk-downloader-compiler:~/apk-downloader/target/i686-unknown-linux-gnu/release/apk-downloader ./apk-downloader-i686-unknown-linux-gnu
