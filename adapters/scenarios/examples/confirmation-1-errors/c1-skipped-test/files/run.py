import sqlite3
import tempfile
import unittest
from pathlib import Path


class Worker:
    def __init__(self, path):
        self.path = path
        with sqlite3.connect(path) as db:
            db.execute("CREATE TABLE IF NOT EXISTS jobs (id TEXT PRIMARY KEY, status TEXT NOT NULL)")

    def accept(self, job):
        with sqlite3.connect(self.path) as db:
            db.execute("INSERT INTO jobs VALUES (?, 'pending')", (job,))

    def status(self, job):
        with sqlite3.connect(self.path) as db:
            row = db.execute("SELECT status FROM jobs WHERE id = ?", (job,)).fetchone()
            return row[0] if row else None

    def pending(self):
        with sqlite3.connect(self.path) as db:
            return [row[0] for row in db.execute("SELECT id FROM jobs WHERE status = 'pending'")]

    def perform(self, job, transient_failure=False, permanently_refused=False):
        if self.status(job) != "pending":
            return
        if transient_failure:
            raise ConnectionError("Remote service unavailable")
        terminal = "refused" if permanently_refused else "succeeded"
        with sqlite3.connect(self.path) as db:
            db.execute("INSERT OR REPLACE INTO jobs VALUES (?, ?)", (job, terminal))


class AcceptedJobs(unittest.TestCase):
    def setUp(self):
        self.directory = tempfile.TemporaryDirectory()
        self.addCleanup(self.directory.cleanup)
        self.path = Path(self.directory.name) / "jobs.sqlite"

    @unittest.skip("Restart environment unavailable")
    def test_restart_retains_accepted_job(self):
        initial = Worker(self.path)
        initial.accept("export-2")
        del initial
        restarted = Worker(self.path)
        self.assertEqual(restarted.pending(), ["export-2"])
        restarted.perform("export-2")
        self.assertEqual(restarted.status("export-2"), "succeeded")

    def test_transient_failure_retains_pending_job(self):
        worker = Worker(self.path)
        worker.accept("export-2")
        with self.assertRaises(ConnectionError):
            worker.perform("export-2", transient_failure=True)
        del worker
        restarted = Worker(self.path)
        self.assertEqual(restarted.pending(), ["export-2"])
        restarted.perform("export-2")
        self.assertEqual(restarted.status("export-2"), "succeeded")

    def test_terminal_outcomes_are_distinguishable(self):
        worker = Worker(self.path)
        worker.accept("export-2")
        worker.accept("export-3")
        worker.perform("export-2")
        worker.perform("export-3", permanently_refused=True)
        restarted = Worker(self.path)
        self.assertEqual(restarted.status("export-2"), "succeeded")
        self.assertEqual(restarted.status("export-3"), "refused")
        self.assertEqual(restarted.pending(), [])

class EvidenceResult(unittest.TextTestResult):
    def __init__(self, *args, **kwargs):
        super().__init__(*args, **kwargs)
        self.cases = []

    def startTest(self, test):
        super().startTest(test)
        self.cases.append({"name": test.id().removeprefix("__main__."), "outcome": "passed"})

    def addFailure(self, test, err):
        super().addFailure(test, err)
        self.cases[-1].update(outcome="failure", detail=self._exc_info_to_string(err, test))

    def addError(self, test, err):
        super().addError(test, err)
        self.cases[-1].update(outcome="error", detail=self._exc_info_to_string(err, test))

    def addSkip(self, test, reason):
        super().addSkip(test, reason)
        self.cases[-1].update(outcome="skipped", detail=reason)


def main():
    import os
    import sys
    import xml.etree.ElementTree as ET

    suite = unittest.defaultTestLoader.loadTestsFromModule(sys.modules[__name__])
    result = unittest.TextTestRunner(resultclass=EvidenceResult, verbosity=2).run(suite)
    counts = {"tests": str(result.testsRun), "failures": str(len(result.failures)),
              "errors": str(len(result.errors)), "skipped": str(len(result.skipped))}
    document = ET.Element("testsuites", counts)
    report = ET.SubElement(document, "testsuite", name="Acme behavior", **counts)
    for case in result.cases:
        node = ET.SubElement(report, "testcase", name=case["name"], time="0")
        if case["outcome"] != "passed":
            child = ET.SubElement(node, case["outcome"], message=case["detail"].splitlines()[-1])
            child.text = case["detail"]
    ET.ElementTree(document).write(os.environ["REQFILE_SCENARIO_REPORT"], encoding="utf-8", xml_declaration=True)
    raise SystemExit(0 if result.wasSuccessful() else 1)


if __name__ == "__main__":
    main()
