To download directly from the Google Play Store, you have two authentication options:

## Option 1: Using OAuth Token (recommended for personal accounts)

First, obtain an OAuth token by visiting the Google [embedded setup page](https://accounts.google.com/EmbeddedSetup) and:

**Method 1: Using Network tab (recommended)**
1. Open the browser debugging console (F12 or Ctrl+Shift+I) and go to the `Network` tab
2. Log in to your Google account
3. If the "Google Terms of Services" pop up, click `I agree` (there may be some delay afterwards)
4. Select the last request from `accounts.google.com` in the `Network` tab
5. Select the `Cookies` tab of this request
6. One of the response cookies is `oauth_token`
7. Copy the `value` field (it will start with `oauth2_4/`)

**Method 2: Using Application tab (alternative)**
1. Open the browser debugging console and go to the `Application` tab
2. In the left sidebar, expand `Cookies` → `https://accounts.google.com`
3. Find and select `oauth_token` cookie
4. Copy the `Value` field (it will start with `oauth2_4/`)

> **Note:** The OAuth token can only be used once, in order to obtain the AAS token which can be used subsequently. Do not enclose the token in quotes when passing it to apkeep.

To obtain the AAS token:

```shell
apkeep -e 'someone@gmail.com' --oauth-token oauth2_4/...
```

An AAS token should be printed. You can use this to download an app:

```shell
apkeep -a com.instagram.android -e 'someone@gmail.com' -t some_aas_token .
```

## Option 2: Using AUTH Token (for token dispensers)

If you have an AUTH token from a token dispenser (e.g., Aurora Store's dispenser), you can use it directly with the `--auth-token` flag:

```shell
apkeep -a com.instagram.android -d google-play -e 'dispenser_email@gmail.com' --auth-token 'ya29.a0...' --accept-tos .
```

AUTH tokens typically start with `ya29.`. The `--accept-tos` flag is recommended for first-time use with a new account to automatically accept Google Play's Terms of Service.

## Device Configuration

Both options will use a default device configuration of `px_9a`, a timezone of `UTC`, and a locale of `en_US`. To specify a different device profile, use the `-o` option:

```shell
apkeep -a com.instagram.android -d google-play -o device=ad_g3_pro -e 'someone@gmail.com' -t some_aas_token .
```

Available devices are specified [here](https://github.com/EFForg/rs-google-play/blob/master/gpapi/device.properties).

To specify your own device properties file (e.g. one you've exported from Aurora Store's spoof manager):

```shell
apkeep -a com.instagram.android -d google-play -o device=default,device_properties_file=/path/to/device.properties .
```

Likewise, a separate timezone or locale can also be specified:

```shell
apkeep -a com.instagram.android -d google-play -o device=ad_g3_pro,locale=es_MX -e 'someone@gmail.com' -t some_aas_token .
```

Note that if the app you're downloading is region-locked and not available in your region, the locale and timezone changes alone will not let apkeep download it. 
In that case the download will fail with a `Invalid app response` error. 
To download a region-locked app you first need to bind it to your account by downloading it with an IP address from a region it's available in.
After the first download, you can download the app from any region.


This option attempts to download a split APK if available, and falls back to the full APK:

```shell
apkeep -a hk.easyvan.app.client -d google-play -o split_apk=true -e 'someone@gmail.com' -t some_aas_token .
```

A full list of options:

* `device`: specify a device profile as described above
* `device_properties_file`: specify a custom device properties file
* `locale`: specify a locale
* `timezone`: specify a timezone 
* `split_apk`: when set to `1` or `true`, attempts to download a [split APK](https://developer.android.com/studio/build/configure-apk-splits)
* `include_dex_metadata`: when set to `1` or `true`, attempts to download the DexMetadata (dm) file for an app, which contains the apps' cloud profile
* `include_additional_files`: when set to `1` or `true`, attempts to download any [additional `obb` expansion files](https://developer.android.com/google/play/expansion-files) for the app

If you prefer not to provide your credentials on the command line, you can specify them in a config file named `apkeep.ini`. This config file may have to be created, and must be located in the user config directory under the subpath `apkeep`. Usually on Linux systems this will be `~/.config/apkeep/apkeep.ini`. In this file specify your email and token:

```ini
[google]
email = someone@gmail.com
aas_token = some_aas_token
# OR use auth_token instead of aas_token:
# auth_token = ya29.a0...
```

Optionally, the path to this `ini` file can be specified:

```shell
apkeep -a com.instagram.android -d google-play -i ~/path/to/some.ini .
```
