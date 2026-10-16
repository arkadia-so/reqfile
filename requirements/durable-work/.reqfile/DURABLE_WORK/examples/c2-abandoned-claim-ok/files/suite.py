import os
import sqlite3
import subprocess
import sys
import tempfile
from pathlib import Path

RECOVER_ABANDONED = True

class Worker:
    def __init__(self, path):
        self.path = path
        if RECOVER_ABANDONED:
            with sqlite3.connect(path) as db:
                db.execute("UPDATE jobs SET state = 'ready' WHERE state = 'claimed'")

    def run(self, outcome):
        with sqlite3.connect(self.path) as db:
            row = db.execute("SELECT id FROM jobs WHERE state = 'ready' ORDER BY id LIMIT 1").fetchone()
            if row is None:
                return None
            state = {"transient": "ready", "success": "succeeded", "permanent": "refused"}[outcome]
            db.execute("UPDATE jobs SET state = ? WHERE id = ?", (state, row[0]))
            return state

def initialize(path):
    with sqlite3.connect(path) as db:
        db.executescript("CREATE TABLE jobs (id INTEGER PRIMARY KEY, state TEXT); INSERT INTO jobs VALUES (1, 'ready');")

def state(path):
    with sqlite3.connect(path) as db:
        return db.execute("SELECT state FROM jobs WHERE id = 1").fetchone()[0]

def accepted_restart():
    with tempfile.TemporaryDirectory() as directory:
        path = str(Path(directory) / "queue.sqlite")
        initialize(path)
        crashed = subprocess.run([sys.executable, __file__, "crash-after-claim", path], timeout=5)
        assert crashed.returncode == 73, crashed.returncode
        assert state(path) == "claimed"
        replacement = Worker(path)
        assert replacement.run("success") == "succeeded"
        assert state(path) == "succeeded"

def retryable_failure():
    with tempfile.TemporaryDirectory() as directory:
        path = str(Path(directory) / "queue.sqlite")
        initialize(path)
        assert Worker(path).run("transient") == "ready"
        assert state(path) == "ready"
        assert Worker(path).run("success") == "succeeded"

def terminal_outcome():
    for outcome, expected in (("success", "succeeded"), ("permanent", "refused")):
        with tempfile.TemporaryDirectory() as directory:
            path = str(Path(directory) / "queue.sqlite")
            initialize(path)
            assert Worker(path).run(outcome) == expected
            assert state(path) == expected
            assert Worker(path).run("success") is None

if __name__ == "__main__" and len(sys.argv) > 1:
    assert sys.argv[1] == "crash-after-claim"
    with sqlite3.connect(sys.argv[2]) as db:
        db.execute("UPDATE jobs SET state = 'claimed' WHERE id = 1")
    os._exit(73)

SCENARIOS = {"accepted-restart": accepted_restart, "retryable-failure": retryable_failure, "terminal-outcome": terminal_outcome}

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
