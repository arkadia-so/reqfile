import sqlite3
import tempfile
import threading
from pathlib import Path

RECHECK_CAPACITY = True

def contend():
    with tempfile.TemporaryDirectory() as directory:
        path = str(Path(directory) / "capacity.sqlite")
        with sqlite3.connect(path) as db:
            db.executescript("CREATE TABLE capacity (available INTEGER); INSERT INTO capacity VALUES (10); CREATE TABLE reservations (owner TEXT PRIMARY KEY, quantity INTEGER);")
        overlap = threading.Barrier(2, timeout=3)
        outcomes, errors = [], []

        def reserve(owner):
            try:
                with sqlite3.connect(path, timeout=3) as db:
                    observed = db.execute("SELECT available FROM capacity").fetchone()[0]
                    overlap.wait()
                    db.execute("BEGIN IMMEDIATE")
                    if RECHECK_CAPACITY:
                        claimed = db.execute("UPDATE capacity SET available = available - 7 WHERE available >= 7").rowcount
                    else:
                        claimed = db.execute("UPDATE capacity SET available = ?", (observed - 7,)).rowcount
                    if claimed:
                        db.execute("INSERT INTO reservations VALUES (?, 7)", (owner,))
                    outcomes.append("accepted" if claimed else "rejected")
            except Exception as error:
                errors.append(error)

        workers = [threading.Thread(target=reserve, args=(owner,)) for owner in ("north", "south")]
        for worker in workers:
            worker.start()
        for worker in workers:
            worker.join(timeout=6)
            assert not worker.is_alive(), "Contention must terminate"
        assert not errors, repr(errors)
        assert len(outcomes) == 2
        with sqlite3.connect(path) as db:
            available = db.execute("SELECT available FROM capacity").fetchone()[0]
            allocated = db.execute("SELECT COALESCE(SUM(quantity), 0) FROM reservations").fetchone()[0]
        return outcomes, available, allocated

def competing_transitions():
    outcomes, _, _ = contend()
    assert sorted(outcomes) == ["accepted", "rejected"], outcomes

def invariant_after_contention():
    _, available, allocated = contend()
    assert available >= 0
    assert available + allocated == 10, (available, allocated)

SCENARIOS = {"competing-transitions": competing_transitions, "invariant-after-contention": invariant_after_contention}

def report():
    import os
    import traceback
    import xml.etree.ElementTree as ET
    suite = ET.Element("testsuite", name="independent-scenarios")
    failures = 0
    for name, scenario in SCENARIOS.items():
        case = ET.SubElement(suite, "testcase", name=name, classname="contract")
        try:
            scenario()
        except Exception:
            failures += 1
            ET.SubElement(case, "failure", message="scenario assertion failed").text = traceback.format_exc()
    suite.set("tests", str(len(SCENARIOS)))
    suite.set("failures", str(failures))
    suite.set("errors", "0")
    suite.set("skipped", "0")
    ET.ElementTree(suite).write(os.environ["REQFILE_SCENARIO_REPORT"], encoding="utf-8", xml_declaration=True)
    raise SystemExit(1 if failures else 0)

if __name__ == "__main__":
    report()
