To download directly from the google play store:

```shell
apkeep -a com.instagram.android -d google-play -u 'someone@gmail.com' -p somepass .
```

This will use a default device configuration of `hero2lte`, a timezone of `UTC`, and a locale of `en_US`.  To specify a different device profile, use the `-o` option:

```shell
apkeep -a com.instagram.android -d google-play -o device=cloudbook -u 'someone@gmail.com' -p somepass .
```

Available devices are specified [here](https://github.com/EFForg/rs-google-play/blob/master/gpapi/device.properties).

Likewise, a separate timezone or locale can also be specified:

```shell
apkeep -a com.instagram.android -d google-play -o device=cloudbook,locale=es_MX -u 'someone@gmail.com' -p somepass .
```

This option attempts to download a split APK if available, and falls back to the full APK:

```shell
apkeep -a hk.easyvan.app.client -d google-play -o split_apk=true -u 'someone@gmail.com' -p somepass .
```

If you prefer not to provide your credentials on the command line, you can specify them in a config file named `apkeep.ini`.  This config file may have to be created, and must be located in the user config directory under the subpath `apkeep`.  Usually on Linux systems this will be `~/.config/apkeep/apkeep.ini`.  In this file specify your username and/or password:

```ini
[google]
username = someone@gmail.com
password = somepass
```

Optionally, the path to this `ini` file can be specified:

```shell
apkeep -a com.instagram.android -d google-play -i ~/path/to/some.ini .
```
