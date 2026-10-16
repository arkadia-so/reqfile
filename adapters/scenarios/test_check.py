"""Protocol guards: successful collection is not successful scenario execution."""

import json
import os
from pathlib import Path
import sys
import tempfile
import unittest
from unittest.mock import patch

import check


class Reports(unittest.TestCase):
    def test_junit_records_success_and_assertion_failures(self):
        tests = check.junit('<testsuite tests="2" failures="1"><testcase name="pass"/>'
                            '<testcase name="fail" file="invoice.py"><failure/></testcase></testsuite>')
        self.assertEqual([(t.name, t.status) for t in tests], [("pass", "passed"), ("fail", "failed")])

    def test_junit_suite_setup_errors_and_summary_errors_never_pass(self):
        for evidence in (
            '<testsuite tests="1" errors="1"><error message="setup failed"/><testcase name="pass"/></testsuite>',
            '<testsuite tests="1" failures="1"><testcase name="pass"/></testsuite>',
            '<testsuites errors="1"><testsuite><testcase name="pass"/></testsuite></testsuites>',
            '<testsuite skipped="1"><testcase name="pass"/></testsuite>',
            '<testsuite disabled="1"><testcase name="pass"/></testsuite>',
        ):
            with self.subTest(evidence=evidence), self.assertRaises(check.EvidenceError):
                check.junit(evidence)
        self.assertEqual(check.junit('<testsuite tests="1" errors="0" failures="0"><testcase name="pass"/></testsuite>')[0].status, "passed")

    def test_junit_rejects_inconsistent_counts_unknown_status_and_entities(self):
        for text in ('<testsuite tests="2"><testcase name="one"/></testsuite>',
                     '<testsuite><testcase name="one" status="mystery"/></testsuite>',
                     '<!DOCTYPE x><testsuite/>', '<testsuite>'):
            with self.subTest(text=text), self.assertRaises(check.EvidenceError):
                check.junit(text)

    def test_json_duplicate_fields_cannot_hide_failure(self):
        with self.assertRaises(check.EvidenceError):
            check.load_json('{"success": false, "success": true}')

    def test_vitest_requires_completed_suites_and_consistent_outcomes(self):
        report = {"success": True, "numTotalTests": 1, "testResults": [{
            "name": "invoice.test.ts", "status": "passed", "assertionResults": [{
                "fullName": "an invoice is applied once", "status": "passed"}]}]}
        self.assertEqual(check.vitest(json.dumps(report))[0].status, "passed")
        report["testResults"][0]["status"] = "failed"
        with self.assertRaises(check.EvidenceError):
            check.vitest(json.dumps(report))
        report["testResults"][0]["status"] = "passed"
        report["numTotalTests"] = 0
        with self.assertRaises(check.EvidenceError):
            check.vitest(json.dumps(report))

    def test_dart_requires_finished_visible_tests_and_done(self):
        events = [{"type": "testStart", "test": {"id": 1, "name": "loading", "suiteID": 0}},
                  {"type": "testDone", "testID": 1, "result": "success", "skipped": False, "hidden": True},
                  {"type": "testStart", "test": {"id": 2, "name": "invoice", "suiteID": 0}},
                  {"type": "testDone", "testID": 2, "result": "success", "skipped": False, "hidden": False},
                  {"type": "done", "success": True}]
        render = lambda data: "\n".join(json.dumps(event) for event in data)
        self.assertEqual([t.name for t in check.dart(render(events))], ["invoice"])
        with self.assertRaises(check.EvidenceError):
            check.dart(render(events[:-1]))
        events.insert(-1, events[-2])
        with self.assertRaises(check.EvidenceError):
            check.dart(render(events))


class Execution(unittest.TestCase):
    def setUp(self):
        self.directory = tempfile.TemporaryDirectory()
        self.root = Path(self.directory.name).resolve()
        self.binding = self.root / ".reqfile/REPLAY_SAFE/binding.json"
        self.binding.parent.mkdir(parents=True)
        self.mapping = dict(zip(check.OBLIGATIONS["REPLAY_SAFE"], ("same", "distinct", "retry")))
        self.environment = patch.dict(os.environ, {"REQFILE_BINDING": "{}"})
        self.environment.start()

    def tearDown(self):
        self.environment.stop()
        self.directory.cleanup()

    def prepare(self, xml, code=0, suffix=""):
        program = ('import os,sys\nfrom pathlib import Path\n'
                   f'Path(os.environ["REQFILE_SCENARIO_REPORT"]).write_text({xml!r})\n'
                   f'{suffix}\nsys.exit({code})\n')
        (self.root / "suite.py").write_text(program)
        self.binding.write_text(json.dumps({"command": [sys.executable, "suite.py"], "scenarios": self.mapping}))

    def report(self, replacement=""):
        return '<testsuite>' + ''.join(f'<testcase name="{name}">{replacement if name == "same" else ""}</testcase>'
                                     for name in self.mapping.values()) + '</testsuite>'

    def test_complete_execution_passes_and_failed_assertions_have_a_location(self):
        self.prepare(self.report())
        self.assertEqual(check.assess("REPLAY_SAFE", self.root), [])
        self.prepare(self.report('<failure/>'), 1)
        failure = check.assess("REPLAY_SAFE", self.root)[0]
        self.assertEqual(failure["locations"][0]["physicalLocation"]["artifactLocation"]["uri"], "suite.py")

    def test_empty_missing_skipped_duplicate_and_pending_evidence_cannot_pass(self):
        for report in ('<testsuite/>', '<testsuite><testcase name="same"/></testsuite>',
                       self.report('<skipped/>'), self.report().replace('</testsuite>', '<testcase name="same"/></testsuite>')):
            with self.subTest(report=report):
                self.prepare(report)
                with self.assertRaises(check.EvidenceError):
                    check.assess("REPLAY_SAFE", self.root)

    def test_command_failure_without_assertion_failure_is_a_tool_error(self):
        self.prepare(self.report(), 2)
        with self.assertRaises(check.EvidenceError):
            check.assess("REPLAY_SAFE", self.root)

    def test_no_report_cannot_reuse_an_old_success(self):
        self.prepare(self.report())
        self.assertEqual(check.assess("REPLAY_SAFE", self.root), [])
        (self.root / "suite.py").write_text('print("no report this run")\n')
        with self.assertRaises(check.EvidenceError):
            check.assess("REPLAY_SAFE", self.root)

    def test_binding_and_working_directory_cannot_escape_through_symlinks(self):
        self.prepare(self.report())
        with tempfile.TemporaryDirectory() as external:
            (self.root / "outside").symlink_to(external, target_is_directory=True)
            data = json.loads(self.binding.read_text())
            data["cwd"] = "outside"
            self.binding.write_text(json.dumps(data))
            with self.assertRaises(check.EvidenceError):
                check.assess("REPLAY_SAFE", self.root)
        os.environ["REQFILE_BINDING"] = '{"binding":"../other.json"}'
        with self.assertRaises(check.EvidenceError):
            check.assess("REPLAY_SAFE", self.root)

    def test_command_argv_is_not_interpreted_as_shell_source(self):
        code, stdout, _ = check.execute([sys.executable, "-c", "import sys;print(sys.argv[1])", "$(touch unwanted)"],
                                        self.root, os.environ.copy(), 2)
        self.assertEqual((code, stdout), (0, "$(touch unwanted)\n"))
        self.assertFalse((self.root / "unwanted").exists())

    def test_timeout_stops_the_process(self):
        with self.assertRaisesRegex(check.EvidenceError, "timed out"):
            check.execute([sys.executable, "-c", "import time;time.sleep(60)"], self.root, os.environ.copy(), 0.1)

    def test_language_selection_cannot_be_changed_in_a_binding(self):
        self.prepare(self.report())
        data = json.loads(self.binding.read_text())
        data["runner"] = "junit"
        self.binding.write_text(json.dumps(data))
        os.environ["REQFILE_BINDING"] = '{"runner":"vitest"}'
        with self.assertRaises(check.EvidenceError):
            check.assess("REPLAY_SAFE", self.root)


if __name__ == "__main__":
    unittest.main()
