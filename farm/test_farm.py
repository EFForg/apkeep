#!/usr/bin/env python3
"""Self-check for farm.py queue logic. Run: python3 test_farm.py

No apkeep, no network, no files -- exercises the SQLite queue invariants that
make resume + redistribution correct: atomic claim, crash recovery, terminal
failure at max_attempts, and release-back-to-pending (redistribution).
"""
import sqlite3
import farm


def fresh():
    c = sqlite3.connect(":memory:")
    farm.init_db(c)
    return c


def test_claim_is_atomic_and_disjoint():
    c = fresh()
    farm.seed_apps(c, ["a", "b", "c", "d"], now=0)
    b1 = farm.claim_batch(c, "acct1", 2, now=1)
    b2 = farm.claim_batch(c, "acct2", 2, now=1)
    assert set(b1) | set(b2) == {"a", "b", "c", "d"}, (b1, b2)
    assert set(b1).isdisjoint(b2), "same pkg claimed twice"
    assert farm.claim_batch(c, "acct3", 2, now=1) == [], "nothing left to claim"


def test_recover_reverts_claimed():
    c = fresh()
    farm.seed_apps(c, ["a", "b"], now=0)
    farm.claim_batch(c, "acct1", 2, now=1)  # simulate a crash mid-batch
    assert farm.remaining(c) == 2
    n = farm.recover(c, now=2)
    assert n == 2
    # after recovery they're claimable again
    assert set(farm.claim_batch(c, "acct2", 2, now=3)) == {"a", "b"}


def test_success_and_redistribution():
    c = fresh()
    farm.seed_apps(c, ["a", "b"], now=0)
    farm.claim_batch(c, "acct1", 2, now=1)
    # a succeeded, b failed (acct1 got banned) -> b returns to the pool
    farm.record_results(c, successes=["a"], failures=["b"], max_attempts=3, now=2)
    assert farm.summary(c).get("done") == 1
    # b is pending again -> another account picks it up (redistribution)
    assert farm.claim_batch(c, "acct2", 5, now=3) == ["b"]


def test_terminal_failure_at_max_attempts():
    c = fresh()
    farm.seed_apps(c, ["x"], now=0)
    for i in range(3):  # max_attempts=3
        farm.claim_batch(c, "acct", 1, now=i)
        farm.record_results(c, successes=[], failures=["x"], max_attempts=3, now=i)
    assert farm.summary(c).get("failed") == 1, "should be terminal after 3 attempts"
    assert farm.claim_batch(c, "acct", 1, now=9) == [], "failed apps are not re-claimed"
    assert farm.remaining(c) == 0


def test_reseed_is_idempotent():
    c = fresh()
    farm.seed_apps(c, ["a", "b"], now=0)
    farm.record_results(c, successes=["a"], failures=[], max_attempts=3, now=1)
    farm.seed_apps(c, ["a", "b", "c"], now=2)  # re-run with an extended list
    s = farm.summary(c)
    assert s.get("done") == 1 and s.get("pending") == 2, s  # 'a' stays done, 'c' added


if __name__ == "__main__":
    for name, fn in sorted(globals().items()):
        if name.startswith("test_") and callable(fn):
            fn()
            print(f"ok  {name}")
    print("all passed")
