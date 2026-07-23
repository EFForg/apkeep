# Phone-farm downloader for apkeep

Downloads a list of apps from Google Play across a pool of Google accounts (one per
phone). It's a driver over the `apkeep` binary with a SQLite queue, so it resumes
after interruptions and, if an account gets rate-limited or banned, hands its
remaining apps to the other accounts automatically.

Stdlib Python only — no `pip install`.

## 1. Build apkeep

```sh
cargo build --release        # produces ../target/release/apkeep
```

## 2. Prepare accounts

Create `accounts.csv` from the example:

```sh
cp accounts.csv.example accounts.csv
```

One row per phone/account:

```
email,token,token_type,device_properties_path,locale
someone@gmail.com,AAS_TOKEN_HERE,aas,,us
```

- **token** — the account's AAS token. Get it via:
  ```sh
  ../target/release/apkeep -e someone@gmail.com --oauth-token oauth2_4/<value>
  ```
  where `<value>` is the `oauth_token` cookie (starts `oauth2_4/`) from the network
  tab of `https://accounts.google.com/EmbeddedSetup` after signing in. apkeep prints
  the AAS token. (Full walkthrough: `../USAGE-google-play.md`.)
- **token_type** — `aas` (default) or `auth` for an AUTH token (`ya29.…`).
- **device_properties_path** — leave blank to use the default device (Pixel 9a), or
  see section 3.
- **locale** — e.g. `us`, `fr`.

Use **disposable** accounts — Google may terminate accounts used this way.

## 3. Device profile (optional)

This decides which APK variant Google serves (CPU/ABI, screen size, Android
version). Match it to the device you'll install/test on, or some splits may not
install. Skip this section to use the default (Pixel 9a).

**Option A — pick a built-in profile.** Find your test device's model
(Settings → About phone), match it to a `[name]` in
https://github.com/EFForg/rs-google-play/blob/master/gpapi/device.properties,
and pass it to a run with `--device <name>` (leave the CSV column blank).

**Option B — export the real device.** On the test device install **Aurora Store**
(https://auroraoss.com), open **Settings → Spoof Manager → Device**, and use
**export/share** to save its `.properties` file. Copy it to `farm/devices/mydevice.properties`
and put that path in the `device_properties_path` column.

## 4. Verify accounts

Check every account can log in (no downloads):

```sh
python3 farm.py --accounts accounts.csv --apkeep ../target/release/apkeep --check
```

Prints `OK`/`BAD` per account and exits non-zero if any are bad. Fix any `BAD`
token before running.

## 5. Run

`apps.csv` is one package name per line (or a CSV — pick the column with `--field`).

```sh
mkdir -p out
python3 farm.py \
  --apps apps.csv \
  --accounts accounts.csv \
  --outdir out \
  --apkeep ../target/release/apkeep \
  --batch-size 50 --parallel 4 --sleep 1000 --accept-tos
```

Stop any time with Ctrl-C (workers finish their current batch). Re-run the same
command to resume — finished apps are skipped, interrupted ones are picked back up.

### Options

| flag | default | meaning |
|------|---------|---------|
| `--apps` | — | package list (CSV/text), one per line |
| `--accounts` | — | accounts.csv |
| `--outdir` | — | where APKs are written |
| `--apkeep` | `apkeep` | path to the apkeep binary |
| `--field` | 1 | app-id column in `--apps` |
| `--batch-size` | 50 | apps per account per apkeep call |
| `--parallel` | 4 | concurrent downloads within one account |
| `--sleep` | 1000 | ms between apps (rate control) |
| `--device` | — | built-in device profile name (Option A) |
| `--max-attempts` | 3 | tries per app before it's marked `failed` |
| `--max-fails` | 3 | bad batches before an account cools down |
| `--cooldowns` | 1 | cooldowns before an account is disabled |
| `--cooldown` | 600 | cooldown seconds |
| `--accept-tos` | off | accept Google Play ToS on first use |
| `--no-split-apk` | off | download full APK instead of split APKs |

Effective download rate ≈ `accounts × parallel`, paced by `--sleep`. If you see
retries or errors in the logs, raise `--sleep` or lower `--parallel`.

## Progress & results

```sh
sqlite3 queue.db "SELECT status, COUNT(*) FROM apps GROUP BY status"   # overview
sqlite3 queue.db "SELECT pkg FROM apps WHERE status='failed'"          # gave up on these
```

- `logs/<email>.log` — per-account apkeep output.
- Watch for throttling: `grep -iE 'retry|error' logs/*.log`.

## Notes

- **Disk**: apps are large. Thousands of apps can be hundreds of GB — make sure
  `--outdir` has room.
- **Region-locked apps** must be downloaded once from an allowed-region IP before
  they work elsewhere; otherwise they fail with `Invalid app response`.

## Test the tool itself

```sh
python3 test_farm.py
```
