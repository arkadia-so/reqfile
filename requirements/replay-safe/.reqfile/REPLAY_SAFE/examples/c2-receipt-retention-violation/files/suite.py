import sqlite3

KEEP_RECEIPTS = False

class Ledger:
    def __init__(self):
        self.db = sqlite3.connect(":memory:")
        self.db.executescript("CREATE TABLE receipts (operation TEXT PRIMARY KEY); CREATE TABLE charges (amount INTEGER);")

    def charge(self, operation, fail_after_commit=False):
        with self.db:
            created = self.db.execute("INSERT OR IGNORE INTO receipts VALUES (?)", (operation,)).rowcount
            if created:
                self.db.execute("INSERT INTO charges VALUES (7)")
        if fail_after_commit:
            raise ConnectionError("Acknowledgement lost after commit")

    def archive(self):
        if not KEEP_RECEIPTS:
            with self.db:
                self.db.execute("DELETE FROM receipts")

    def total(self):
        return self.db.execute("SELECT COALESCE(SUM(amount), 0) FROM charges").fetchone()[0]

def same_operation():
    ledger = Ledger()
    ledger.charge("invoice-8")
    ledger.archive()
    ledger.charge("invoice-8")
    assert ledger.total() == 7, "Archival must not end the published replay lifetime"

def distinct_operations():
    ledger = Ledger()
    ledger.charge("invoice-8")
    ledger.archive()
    ledger.charge("invoice-9")
    assert ledger.total() == 14

def retry_after_failure():
    ledger = Ledger()
    try:
        ledger.charge("invoice-8", fail_after_commit=True)
    except ConnectionError:
        pass
    else:
        raise AssertionError("The acknowledgement fault was not injected")
    ledger.charge("invoice-8")
    assert ledger.total() == 7

SCENARIOS = {"same-operation": same_operation, "distinct-operations": distinct_operations, "retry-after-failure": retry_after_failure}

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
