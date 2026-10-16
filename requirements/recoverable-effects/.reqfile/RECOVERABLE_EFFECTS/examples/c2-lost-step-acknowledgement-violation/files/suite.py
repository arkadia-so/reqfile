import sqlite3
import tempfile
from pathlib import Path

CONSULT_RECEIPT = False

class Shipment:
    def __init__(self, path):
        self.path = path

    def initialize(self):
        with sqlite3.connect(self.path) as db:
            db.executescript("CREATE TABLE operations (id TEXT PRIMARY KEY, stage TEXT); INSERT INTO operations VALUES ('parcel-4', 'accepted'); CREATE TABLE stock (quantity INTEGER); INSERT INTO stock VALUES (5); CREATE TABLE receipts (operation TEXT, quantity INTEGER);")

    def receipts(self):
        with sqlite3.connect(self.path) as db:
            return db.execute("SELECT operation, quantity FROM receipts").fetchall()

    def reserve(self):
        # The warehouse commits independently of the coordinator's stage update.
        with sqlite3.connect(self.path) as db:
            db.execute("UPDATE stock SET quantity = quantity - 1")
            db.execute("INSERT INTO receipts VALUES ('parcel-4', 1)")

    def finish(self, fail_after_effect=False):
        with sqlite3.connect(self.path) as db:
            stage = db.execute("SELECT stage FROM operations WHERE id = 'parcel-4'").fetchone()[0]
        if stage == "complete":
            return
        if not (CONSULT_RECEIPT and self.receipts()):
            self.reserve()
        if fail_after_effect:
            raise ConnectionError("Warehouse response lost")
        with sqlite3.connect(self.path) as db:
            db.execute("UPDATE operations SET stage = 'complete' WHERE id = 'parcel-4'")

    def snapshot(self):
        with sqlite3.connect(self.path) as db:
            return (db.execute("SELECT stage FROM operations").fetchone()[0], db.execute("SELECT quantity FROM stock").fetchone()[0])

def interrupted(path):
    shipment = Shipment(path)
    shipment.initialize()
    try:
        shipment.finish(fail_after_effect=True)
    except ConnectionError:
        pass
    else:
        raise AssertionError("The partial-failure injection did not occur")
    return shipment

def partial_failure():
    with tempfile.TemporaryDirectory() as directory:
        shipment = interrupted(str(Path(directory) / "shipment.sqlite"))
        assert shipment.snapshot() == ("accepted", 4)
        assert shipment.receipts() == [("parcel-4", 1)], "The committed effect is identifiable despite the lost response"

def recovery():
    with tempfile.TemporaryDirectory() as directory:
        path = str(Path(directory) / "shipment.sqlite")
        interrupted(path)
        replacement = Shipment(path)
        replacement.finish()
        assert replacement.snapshot() == ("complete", 4)
        assert replacement.receipts() == [("parcel-4", 1)]

def repeated_recovery():
    with tempfile.TemporaryDirectory() as directory:
        path = str(Path(directory) / "shipment.sqlite")
        interrupted(path)
        Shipment(path).finish()
        Shipment(path).finish()
        replacement = Shipment(path)
        assert replacement.snapshot() == ("complete", 4)
        assert replacement.receipts() == [("parcel-4", 1)]

SCENARIOS = {"partial-failure": partial_failure, "recovery": recovery, "repeated-recovery": repeated_recovery}

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
