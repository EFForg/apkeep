To download directly from the Google Play Store, first you'll have to obtain an OAuth token by visiting the Google [embedded setup page](https://accounts.google.com/EmbeddedSetup/identifier?flowName=EmbeddedSetupAndroid) and opening the browser debugging console, logging in, and looking for the `oauth_token` cookie being set on your browser.  It will be present in the last requests being made and start with "oauth2_4/".  Copy this value.  It can only be used once, in order to obtain the AAS token which can be used subsequently.  To obtain this token:

```shell
apkeep -e 'someone@gmail.com' --oauth-token 'oauth2_4/...'
```

An AAS token should be printed. You can use this to download an app:

```shell
apkeep -a com.instagram.android -d google-play -e 'someone@gmail.com' -t some_aas_token .
```

This will use a default device configuration of `px_7a`, a timezone of `UTC`, and a locale of `en_US`.  To specify a different device profile, use the `-o` option:

```shell
apkeep -a com.instagram.android -d google-play -o device=ad_g3_pro -e 'someone@gmail.com' -t some_aas_token .
```

Available devices are specified [here](https://github.com/EFForg/rs-google-play/blob/master/gpapi/device.properties).

Likewise, a separate timezone or locale can also be specified:

```shell
apkeep -a com.instagram.android -d google-play -o device=cloudbook,locale=es_MX -e 'someone@gmail.com' -t some_aas_token .
```

This option attempts to download a split APK if available, and falls back to the full APK:

```shell
apkeep -a hk.easyvan.app.client -d google-play -o split_apk=true -e 'someone@gmail.com' -t some_aas_token .
```

A full list of options:

* `device`: specify a device profile as described above
* `locale`: specify a locale
* `split_apk`: when set to `1` or `true`, attempts to download a [split APK](https://developer.android.com/studio/build/configure-apk-splits)
* `include_additional_files`: when set to `1` or `true`, attempts to download any [additional `obb` expansion files](https://developer.android.com/google/play/expansion-files) for the app

If you prefer not to provide your credentials on the command line, you can specify them in a config file named `apkeep.ini`.  This config file may have to be created, and must be located in the user config directory under the subpath `apkeep`.  Usually on Linux systems this will be `~/.config/apkeep/apkeep.ini`.  In this file specify your username and/or password:

```ini
[google]
email = someone@gmail.com
aas_token = somepass
```

Optionally, the path to this `ini` file can be specified:

```shell
apkeep -a com.instagram.android -d google-play -i ~/path/to/some.ini .
```
