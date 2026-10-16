import sqlite3
import tempfile
import unittest
from pathlib import Path

BUG = False

class Queue:
    def __init__(self, path):
        self.db = sqlite3.connect(":memory:" if BUG else path)
        self.db.execute("create table if not exists jobs (id text primary key, status text)")
        self.db.commit()

    def accept(self, identity):
        self.db.execute("insert into jobs values (?, 'pending')", (identity,))
        self.db.commit()

    def run(self, identity, transient=False, refused=False):
        if transient:
            raise ConnectionError("Unavailable")
        outcome = "refused" if refused else "succeeded"
        self.db.execute("update jobs set status=? where id=?", (outcome, identity))
        self.db.commit()

    def status(self, identity):
        row = self.db.execute("select status from jobs where id=?", (identity,)).fetchone()
        return row[0] if row else None

    def close(self):
        self.db.close()

class Scenarios(unittest.TestCase):
    def setUp(self):
        self.directory = tempfile.TemporaryDirectory()
        self.path = Path(self.directory.name) / "queue.db"

    def tearDown(self):
        self.directory.cleanup()

    def test_accepted_restart(self):
        worker = Queue(self.path)
        worker.accept("job-1")
        worker.close()
        replacement = Queue(self.path)
        try:
            self.assertEqual(replacement.status("job-1"), "pending")
            replacement.run("job-1")
            self.assertEqual(replacement.status("job-1"), "succeeded")
        finally:
            replacement.close()

    def test_retryable_failure(self):
        worker = Queue(self.path)
        try:
            worker.accept("job-1")
            with self.assertRaises(ConnectionError):
                worker.run("job-1", transient=True)
            self.assertEqual(worker.status("job-1"), "pending")
            worker.run("job-1")
            self.assertEqual(worker.status("job-1"), "succeeded")
        finally:
            worker.close()

    def test_terminal_outcome(self):
        worker = Queue(self.path)
        try:
            worker.accept("job-1")
            worker.accept("job-2")
            worker.run("job-1")
            worker.run("job-2", refused=True)
            self.assertEqual(worker.status("job-1"), "succeeded")
            self.assertEqual(worker.status("job-2"), "refused")
        finally:
            worker.close()

if __name__ == "__main__":
    import os
    import sys
    import xml.etree.ElementTree as ET
    suite = unittest.defaultTestLoader.loadTestsFromTestCase(Scenarios)
    names = [case.id().rsplit(".", 1)[-1] for case in suite]
    result = unittest.TestResult()
    suite.run(result)
    root = ET.Element("testsuite", tests=str(result.testsRun))
    failures = {case.id().rsplit(".", 1)[-1]: detail for case, detail in result.failures + result.errors}
    for name in names:
        case = ET.SubElement(root, "testcase", name=name)
        if name in failures:
            ET.SubElement(case, "failure").text = failures[name]
    ET.ElementTree(root).write(os.environ["REQFILE_SCENARIO_REPORT"], encoding="unicode")
    sys.exit(0 if result.wasSuccessful() else 1)
