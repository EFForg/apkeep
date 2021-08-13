#!/bin/bash
# Build the APK Downloader for release from a fresh Debian 10 x64 install

ssh -o 'StrictHostKeyChecking no' apk-downloader-compiler << 'EOF'
sudo dpkg --add-architecture armhf
sudo dpkg --add-architecture i386
sudo dpkg --add-architecture arm64
sudo apt-get -y update
sudo apt-get -y dist-upgrade
sudo apt-get -y install git build-essential libssl-dev pkg-config unzip
sudo apt-get -y install libc6-armhf-cross libc6-dev-armhf-cross gcc-arm-linux-gnueabihf libssl-dev:armhf
sudo apt-get -y install libc6-i386-cross libc6-dev-i386-cross gcc-i686-linux-gnu libssl-dev:i386
sudo apt-get -y install libc6-arm64-cross libc6-dev-arm64-cross gcc-aarch64-linux-gnu libssl-dev:arm64
curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs > /tmp/get_rust.sh
bash /tmp/get_rust.sh -y
source ~/.cargo/env
rustup target install armv7-unknown-linux-gnueabihf i686-unknown-linux-gnu aarch64-unknown-linux-gnu aarch64-linux-android armv7-linux-androideabi

git clone https://www.github.com/EFForg/apk-downloader.git
cd apk-downloader
export PKG_CONFIG_ALLOW_CROSS="true"
cargo build --release
export PKG_CONFIG_PATH="/usr/lib/arm-linux-gnueabihf/pkgconfig"
cargo build --release --target=armv7-unknown-linux-gnueabihf
export PKG_CONFIG_PATH="/usr/lib/i686-linux-gnu-gcc/pkgconfig"
cargo build --release --target=i686-unknown-linux-gnu
export PKG_CONFIG_PATH="/usr/lib/aarch-linux-gnu-gcc/pkgconfig"
cargo build --release --target=aarch64-unknown-linux-gnu

cd ~
wget https://www.openssl.org/source/openssl-1.1.1k.tar.gz
tar -zxvf openssl-1.1.1k.tar.gz
cd openssl-1.1.1k
export OPENSSL_DIR=$PWD
export OPENSSL_LIB_DIR=$PWD

cd ~
wget https://dl.google.com/android/repository/android-ndk-r21e-linux-x86_64.zip
# later versions are available, but have to use an earlier version for compatability with openssl-1.1.1k
unzip android-ndk-r21e-linux-x86_64.zip
cd android-ndk-r21e
export ANDROID_NDK_HOME="$PWD"
export PATH="$PATH:$PWD/toolchains/llvm/prebuilt/linux-x86_64/bin"

cd $OPENSSL_DIR
./Configure android-arm64 -D__ANDROID_API__=21
make
cd ../apk-downloader
cargo build --release --target=aarch64-linux-android

cd $OPENSSL_DIR
make clean
./Configure android-arm -D__ANDROID_API__=21
make
cd ../apk-downloader
cargo build --release --target=armv7-linux-androideabi
EOF

scp apk-downloader-compiler:~/apk-downloader/target/release/apk-downloader ./apk-downloader-x86_64-unknown-linux-gnu
scp apk-downloader-compiler:~/apk-downloader/target/armv7-unknown-linux-gnueabihf/release/apk-downloader ./apk-downloader-armv7-unknown-linux-gnueabihf
scp apk-downloader-compiler:~/apk-downloader/target/i686-unknown-linux-gnu/release/apk-downloader ./apk-downloader-i686-unknown-linux-gnu
scp apk-downloader-compiler:~/apk-downloader/target/aarch64-unknown-linux-gnu/release/apk-downloader ./apk-downloader-aarch64-unknown-linux-gnu
scp apk-downloader-compiler:~/apk-downloader/target/aarch64-linux-android/release/apk-downloader ./apk-downloader-aarch64-linux-android
scp apk-downloader-compiler:~/apk-downloader/target/armv7-linux-androideabi/release/apk-downloader ./apk-downloader-armv7-linux-androideabi
