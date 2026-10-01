APKPure is a clone of the Google Play Store which has been found modifying apps available on Google Play to [distribute malware](https://vpnrevie.ws/apkpure-fake-telegram-collector/). This download source should be used *only* for research purposes. If you acknowledge the dangers and still wish to proceed, please add the `-o` option `acknowledge_dangers=true` to explicitly allow downloading from this source:

```shell
apkeep -a  com.instagram.android -d apk-pure -o 'acknowledge_dangers=true' .
```

More options can be passed with the `-o` option. For instance, download a specific architecture variant of an app with `arch=`:

```shell
apkeep -a com.instagram.android -d apk-pure -o 'acknowledge_dangers=true,arch=x86' .
```

To specify multiple architectures, separate the `arch=` specification with a semicolon. The following shows the default `arch` option:

```shell
apkeep -a com.instagram.android -d apk-pure -o 'acknowledge_dangers=true,arch=arm64-v8a;armeabi-v7a;armeabi;x86;x86_64' .
```

You can also list the versions available, either specifying a specific architecture or not. This does not require the `acknowledge_dangers=true` option:

```shell
apkeep -l -a com.instagram.android -o 'arch=x86'
apkeep -l -a com.instagram.android
```
