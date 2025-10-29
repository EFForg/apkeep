APKPure is the default download source. To download from APKPure:

```shell
apkeep -a com.instagram.android .
```

Or, if you wish to explicitly specify the download source:

```shell
apkeep -a com.instagram.android -d apk-pure .
```

More advanced options for this download source can be passed with the `-o` option. For instance, download a specific architecture variant of an app with `arch=`:

```shell
apkeep -a com.instagram.android -o 'arch=x86' .
```

To specify multiple architectures, separate the `arch=` specification with a semicolon. The following shows the default `arch` option:

```shell
apkeep -a com.instagram.android -o 'arch=arm64-v8a;armeabi-v7a;armeabi;x86;x86_64' .
```

You can also list the versions available, either specifying a specific architecture or not:

```shell
apkeep -l -a com.instagram.android -o 'arch=x86'
apkeep -l -a com.instagram.android
```
