#!/usr/bin/env python3
"""Phone-farm orchestrator for apkeep Google Play crawling.

Drives N apkeep subprocesses (one worker per Google account) over a crash-safe
SQLite work queue. Durable resume + automatic redistribution of a banned
account's apps. Stdlib only.

See README.md in this directory for setup and usage.
"""
import argparse
import csv
import os
import subprocess
import sys
import tempfile
import threading
import time
from pathlib import Path

# ---- pure queue logic (unit-tested in test_farm.py) -------------------------

SCHEMA = """
CREATE TABLE IF NOT EXISTS apps (
    pkg TEXT PRIMARY KEY,
    status TEXT NOT NULL DEFAULT 'pending',  -- pending | claimed | done | failed
    attempts INTEGER NOT NULL DEFAULT 0,
    account TEXT,
    updated REAL NOT NULL DEFAULT 0
);
CREATE TABLE IF NOT EXISTS accounts (
    email TEXT PRIMARY KEY,
    status TEXT NOT NULL DEFAULT 'active',   -- active | cooldown | disabled
    fails INTEGER NOT NULL DEFAULT 0
);
"""


def init_db(conn):
    conn.executescript(SCHEMA)
    conn.commit()


def seed_apps(conn, pkgs, now):
    conn.executemany(
        "INSERT OR IGNORE INTO apps(pkg, status, updated) VALUES (?, 'pending', ?)",
        [(p, now) for p in pkgs],
    )
    conn.commit()


def seed_accounts(conn, emails):
    conn.executemany(
        "INSERT OR IGNORE INTO accounts(email, status, fails) VALUES (?, 'active', 0)",
        [(e,) for e in emails],
    )
    conn.commit()


def recover(conn, now):
    """Revert rows left 'claimed' by a crashed run back to 'pending'. Returns count."""
    cur = conn.execute("SELECT COUNT(*) FROM apps WHERE status='claimed'")
    n = cur.fetchone()[0]
    conn.execute(
        "UPDATE apps SET status='pending', account=NULL, updated=? WHERE status='claimed'",
        (now,),
    )
    conn.commit()
    return n


def claim_batch(conn, account, n, now):
    """Atomically claim up to n pending pkgs for `account`. Returns list of pkgs."""
    rows = conn.execute(
        "SELECT pkg FROM apps WHERE status='pending' ORDER BY attempts, pkg LIMIT ?",
        (n,),
    ).fetchall()
    pkgs = [r[0] for r in rows]
    if pkgs:
        qs = ",".join("?" * len(pkgs))
        conn.execute(
            f"UPDATE apps SET status='claimed', account=?, updated=? WHERE pkg IN ({qs})",
            [account, now, *pkgs],
        )
        conn.commit()
    return pkgs


def record_results(conn, successes, failures, max_attempts, now):
    """Mark successes done; bump failures back to pending (or 'failed' at max_attempts)."""
    if successes:
        qs = ",".join("?" * len(successes))
        conn.execute(
            f"UPDATE apps SET status='done', updated=? WHERE pkg IN ({qs})",
            [now, *successes],
        )
    for pkg in failures:
        conn.execute(
            """UPDATE apps SET attempts=attempts+1, account=NULL, updated=?,
               status=CASE WHEN attempts+1 >= ? THEN 'failed' ELSE 'pending' END
               WHERE pkg=?""",
            (now, max_attempts, pkg),
        )
    conn.commit()


def remaining(conn):
    """Count apps still workable (pending or claimed)."""
    return conn.execute(
        "SELECT COUNT(*) FROM apps WHERE status IN ('pending','claimed')"
    ).fetchone()[0]


def release_claimed(conn, account, now):
    """Return an account's in-flight claimed rows to the pool (crash/exit cleanup)."""
    conn.execute(
        "UPDATE apps SET status='pending', account=NULL, updated=? WHERE status='claimed' AND account=?",
        (now, account),
    )
    conn.commit()


def set_account(conn, email, status, fails):
    conn.execute(
        "UPDATE accounts SET status=?, fails=? WHERE email=?", (status, fails, email)
    )
    conn.commit()


def summary(conn):
    return dict(conn.execute("SELECT status, COUNT(*) FROM apps GROUP BY status").fetchall())


# ---- apkeep invocation ------------------------------------------------------


def build_cmd(apkeep, account, batch_csv, outdir, options, parallel, sleep, accept_tos):
    opts = list(options)
    if account["locale"]:
        opts.append(f"locale={account['locale']}")
    if account["device_properties_path"]:
        # a custom file must be paired with device=default (see USAGE-google-play.md)
        opts.append("device=default")
        opts.append(f"device_properties_file={account['device_properties_path']}")
    cmd = [
        apkeep, "-c", batch_csv, "-d", "google-play",
        "-e", account["email"],
        "-r", str(parallel), "-s", str(sleep),
    ]
    if account["token_type"] == "auth":
        cmd += ["--auth-token", account["token"]]
    else:
        cmd += ["-t", account["token"]]
    if accept_tos:
        cmd += ["--accept-tos"]
    cmd += ["-o", ",".join(opts), outdir]
    return cmd


def check_accounts(cfg, accounts):
    """Probe each account's login without downloading. Returns list of (email, ok, detail)."""
    import concurrent.futures

    bogus = "com.apkeep.farm.login.probe"  # nonexistent app: login runs, download is skipped
    tmp = tempfile.mkdtemp(prefix="apkeep-check-")

    def probe(a):
        cmd = [cfg.apkeep, "-d", "google-play", "-e", a["email"], "-r", "1", "-s", "0", "--accept-tos"]
        cmd += ["--auth-token", a["token"]] if a["token_type"] == "auth" else ["-t", a["token"]]
        opts = []
        if a["locale"]:
            opts.append(f"locale={a['locale']}")
        if a["device_properties_path"]:
            opts.append("device=default")
            opts.append(f"device_properties_file={a['device_properties_path']}")
        if opts:
            cmd += ["-o", ",".join(opts)]
        cmd += ["-a", bogus, tmp]
        try:
            r = subprocess.run(cmd, capture_output=True, text=True, timeout=120)
            out = (r.stdout + r.stderr).lower()
        except subprocess.TimeoutExpired:
            return a["email"], False, "timed out"
        if "could not log in" in out or "could not accept" in out:
            return a["email"], False, "login rejected (bad/expired token)"
        # login succeeded; the probe app is (correctly) reported invalid/skipped
        return a["email"], True, "ok"

    with concurrent.futures.ThreadPoolExecutor(max_workers=min(8, len(accounts))) as ex:
        return list(ex.map(probe, accounts))


def produced(outdir, pkg):
    """True if apkeep produced output for pkg (single apk or split dir)."""
    apk = Path(outdir) / f"{pkg}.apk"
    if apk.is_file() and apk.stat().st_size > 0:
        return True
    d = Path(outdir) / pkg
    if d.is_dir() and any(d.iterdir()):
        return True
    # gpapi may suffix versioned names; fall back to a prefix glob
    return any(Path(outdir).glob(f"{pkg}*"))


# ---- worker -----------------------------------------------------------------


def worker(name, account, conn, lock, cfg, stop):
    email = account["email"]
    cooldowns_used = 0
    try:
        _worker_loop(name, account, conn, lock, cfg, stop, cooldowns_used)
    except Exception as e:  # disk full, DB error, etc. -- don't strand this account's work
        print(f"[{name}] worker crashed: {e}; releasing its claimed apps")
    finally:
        try:
            with lock:
                release_claimed(conn, email, time.time())
        except Exception:
            pass  # next run's recover() will reclaim if this also fails


def _worker_loop(name, account, conn, lock, cfg, stop, cooldowns_used):
    email = account["email"]
    while not stop.is_set():
        with lock:
            batch = claim_batch(conn, email, cfg.batch_size, time.time())
        if not batch:
            with lock:
                left = remaining(conn)
            if left == 0:
                return  # queue drained
            time.sleep(5)  # others still hold claimed rows; wait for possible release
            continue

        with tempfile.NamedTemporaryFile("w", suffix=".csv", delete=False) as f:
            f.write("\n".join(batch) + "\n")
            batch_csv = f.name
        try:
            cmd = build_cmd(cfg.apkeep, account, batch_csv, cfg.outdir,
                            cfg.options, cfg.parallel, cfg.sleep, cfg.accept_tos)
            log_path = Path(cfg.logdir) / f"{email}.log"
            with open(log_path, "a") as lf:
                lf.write(f"\n=== batch of {len(batch)} @ {time.strftime('%F %T')} ===\n")
                rc = subprocess.run(cmd, stdout=lf, stderr=subprocess.STDOUT).returncode
        finally:
            os.unlink(batch_csv)

        successes = [p for p in batch if produced(cfg.outdir, p)]
        failures = [p for p in batch if p not in successes]
        with lock:
            record_results(conn, successes, failures, cfg.max_attempts, time.time())

        # account health: rc!=0 (login death) or a batch that produced nothing = a strike
        if rc != 0 or (batch and not successes):
            account["fails"] += 1
            print(f"[{name}] strike {account['fails']}/{cfg.max_fails} "
                  f"(rc={rc}, {len(successes)}/{len(batch)} ok)")
        else:
            account["fails"] = 0

        if account["fails"] >= cfg.max_fails:
            if cooldowns_used < cfg.cooldowns:
                cooldowns_used += 1
                account["fails"] = 0
                with lock:
                    set_account(conn, email, "cooldown", 0)
                print(f"[{name}] cooldown {cooldowns_used}/{cfg.cooldowns} "
                      f"for {cfg.cooldown}s")
                if stop.wait(cfg.cooldown):
                    return
                with lock:
                    set_account(conn, email, "active", 0)
            else:
                with lock:
                    set_account(conn, email, "disabled", account["fails"])
                print(f"[{name}] DISABLED after repeated failures; "
                      f"its apps return to the queue for other accounts")
                return


# ---- setup ------------------------------------------------------------------


def read_accounts(path):
    accounts = []
    with open(path, newline="") as f:
        for row in csv.DictReader(f):
            if not row.get("email", "").strip() or row["email"].lstrip().startswith("#"):
                continue
            accounts.append({
                "email": row["email"].strip(),
                "token": row["token"].strip(),
                "token_type": (row.get("token_type") or "aas").strip().lower(),
                "device_properties_path": (row.get("device_properties_path") or "").strip(),
                "locale": (row.get("locale") or "").strip(),
                "fails": 0,
            })
    return accounts


def read_pkgs(path, field):
    pkgs = []
    with open(path, newline="") as f:
        for line in f:
            line = line.strip()
            if not line or line.startswith("#"):
                continue
            cols = line.split(",")
            if field - 1 < len(cols):
                pkg = cols[field - 1].strip()
                if pkg:
                    pkgs.append(pkg)
    return pkgs


def main(argv=None):
    import sqlite3

    p = argparse.ArgumentParser(description="apkeep phone-farm orchestrator")
    p.add_argument("--apps", help="CSV/text of package names (not needed with --check)")
    p.add_argument("--accounts", required=True, help="accounts.csv (see .example)")
    p.add_argument("--outdir", help="download output directory (not needed with --check)")
    p.add_argument("--check", action="store_true",
                   help="probe every account's login and report valid/invalid, then exit")
    p.add_argument("--db", default="queue.db", help="SQLite queue file")
    p.add_argument("--logdir", default="logs", help="per-account apkeep logs")
    p.add_argument("--apkeep", default="apkeep", help="path to apkeep binary")
    p.add_argument("--field", type=int, default=1, help="1-based app-id column in --apps")
    p.add_argument("--batch-size", type=int, default=50)
    p.add_argument("--parallel", type=int, default=4, help="apkeep -r (in-flight per account)")
    p.add_argument("--device", help="built-in device profile name (e.g. px_9a); overrides default")
    p.add_argument("--sleep", type=int, default=1000, help="apkeep -s ms between apps")
    p.add_argument("--max-attempts", type=int, default=3, help="per-app tries before 'failed'")
    p.add_argument("--max-fails", type=int, default=3, help="consecutive bad batches before cooldown")
    p.add_argument("--cooldowns", type=int, default=1, help="cooldowns before an account is disabled")
    p.add_argument("--cooldown", type=int, default=600, help="cooldown seconds")
    p.add_argument("--accept-tos", action="store_true")
    p.add_argument("--split-apk", action="store_true", default=True)
    p.add_argument("--no-split-apk", dest="split_apk", action="store_false")
    p.add_argument("--additional-files", action="store_true", default=True,
                   help="include OBB expansion files")
    p.add_argument("--dex-metadata", action="store_true", help="include .dm cloud profiles")
    cfg = p.parse_args(argv)

    accounts = read_accounts(cfg.accounts)
    if not accounts:
        sys.exit("no accounts found")

    if cfg.check:
        print(f"Checking {len(accounts)} accounts...")
        results = check_accounts(cfg, accounts)
        bad = 0
        for email, ok, detail in results:
            print(f"  {'OK   ' if ok else 'BAD  '} {email}  ({detail})")
            bad += not ok
        print(f"\n{len(results) - bad}/{len(results)} valid.")
        sys.exit(1 if bad else 0)

    if not cfg.apps or not cfg.outdir:
        sys.exit("--apps and --outdir are required (unless using --check)")
    if not Path(cfg.outdir).is_dir():
        sys.exit(f"outdir is not a directory: {cfg.outdir}")
    Path(cfg.logdir).mkdir(parents=True, exist_ok=True)

    cfg.options = []
    if cfg.device:  # per-account device_properties_path (Option B) takes precedence in build_cmd
        cfg.options.append(f"device={cfg.device}")
    if cfg.split_apk:
        cfg.options.append("split_apk=1")
    if cfg.additional_files:
        cfg.options.append("include_additional_files=1")
    if cfg.dex_metadata:
        cfg.options.append("include_dex_metadata=1")

    pkgs = read_pkgs(cfg.apps, cfg.field)
    if not pkgs:
        sys.exit("no packages found")

    conn = sqlite3.connect(cfg.db, check_same_thread=False)
    conn.execute("PRAGMA busy_timeout=5000")
    init_db(conn)
    now = time.time()
    seed_apps(conn, pkgs, now)
    seed_accounts(conn, [a["email"] for a in accounts])
    reverted = recover(conn, now)
    if reverted:
        print(f"Recovered {reverted} claimed apps from a previous run -> pending")
    # a fresh run re-activates accounts disabled last time (tokens may be refreshed)
    conn.execute("UPDATE accounts SET status='active', fails=0")
    conn.commit()

    print(f"{len(pkgs)} apps seeded ({remaining(conn)} remaining), "
          f"{len(accounts)} accounts. Starting workers...")

    lock = threading.Lock()  # ponytail: one global DB lock; batch claims make contention negligible
    stop = threading.Event()
    threads = []
    for i, acc in enumerate(accounts):
        t = threading.Thread(target=worker, args=(f"w{i}:{acc['email']}", acc, conn, lock, cfg, stop))
        t.start()
        threads.append(t)
    try:
        while any(t.is_alive() for t in threads):
            for t in threads:
                t.join(timeout=1)
    except KeyboardInterrupt:
        print("\nInterrupted; signalling workers to stop after current batch...")
        stop.set()
        for t in threads:
            t.join()

    s = summary(conn)
    print(f"\nDone. {s}")
    if s.get("failed"):
        print(f"{s['failed']} apps hit --max-attempts and are marked 'failed' "
              f"(query the DB: SELECT pkg FROM apps WHERE status='failed').")
    conn.close()


if __name__ == "__main__":
    main()
