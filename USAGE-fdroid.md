To download from the F-Droid open source repository:

```shell
apkeep -a org.mozilla.fennec_fdroid -d f-droid .
```

More advanced options for this download source can be passed with the `-o` option.  For instance, to use an F-Droid mirror:

```shell
apkeep -a org.mozilla.fennec_fdroid -d f-droid -o repo=https://cloudflare.f-droid.org/repo .
```

In addition to specifying a mirror, a wholly separate F-Droid repo can be specified along with its fingerprint:

```shell
apkeep -a org.torproject.android -d f-droid -o repo=https://guardianproject.info/fdroid/repo?fingerprint=B7C2EEFD8DAC7806AF67DFCD92EB18126BC08312A7F2D6F3862E46013C7A6135 .
```

If a repo supports the new [entry point specification](https://f-droid.org/docs/All_our_APIs/#the-repo-index), you can specify that be used instead of the older (v1) package index.  This may become the default behavior in the future, but can be specified by use of the `use_entry` option:

```shell
apkeep -a org.torproject.android -d f-droid -o repo=https://guardianproject.info/fdroid/repo?fingerprint=B7C2EEFD8DAC7806AF67DFCD92EB18126BC08312A7F2D6F3862E46013C7A6135,use_entry=true .
```

A special option can also be used to skip verification of the repository index.  *Only use for debugging purposes*:

```shell
apkeep -a org.torproject.android -d f-droid -o repo=https://guardianproject.info/fdroid/repo,verify-index=false .
```

It is also possible to download a specific architecture variant of an app with the `arch=` option:

```shell
apkeep -a org.videloan.vlc@3.5.4 -d f-droid -o arch=arm64-v8a .
```
