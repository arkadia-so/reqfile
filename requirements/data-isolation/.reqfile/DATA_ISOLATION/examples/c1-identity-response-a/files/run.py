import unittest
from dataclasses import dataclass


class Documents:
    def __init__(self):
        self.rows = {"alice": "Alice private note", "bob": "Bob private note"}

    def read(self, principal, owner):
        if principal != owner:
            raise PermissionError("Private document")
        return self.rows[owner]

    def write(self, principal, owner, value):
        if principal != owner:
            raise PermissionError("Private document")
        self.rows[owner] = value


@dataclass
class PendingResponse:
    identity: str
    generation: int
    value: str


class Session:
    def __init__(self, service):
        self.service = service
        self.identity = None
        self.generation = 0
        self.cache = None

    def switch(self, identity):
        self.identity = identity
        self.generation += 1
        self.cache = None

    def begin_read(self):
        return PendingResponse(self.identity, self.generation,
                               self.service.read(self.identity, self.identity))

    def finish_read(self, response):
        if response.identity != self.identity or response.generation != self.generation:
            return
        self.cache = response.value


class PrivateNotes(unittest.TestCase):
    def test_foreign_read_is_denied(self):
        service = Documents()
        with self.assertRaises(PermissionError):
            service.read("bob", "alice")
        self.assertEqual(service.read("bob", "bob"), "Bob private note")

    def test_foreign_write_is_denied_and_preserves_state(self):
        service = Documents()
        with self.assertRaises(PermissionError):
            service.write("bob", "alice", "replaced")
        self.assertEqual(service.read("alice", "alice"), "Alice private note")

    def test_switch_discards_cache_and_pending_response(self):
        session = Session(Documents())
        session.switch("alice")
        session.finish_read(session.begin_read())
        self.assertEqual(session.cache, "Alice private note")
        old_response = session.begin_read()
        session.switch("bob")
        self.assertIsNone(session.cache)
        session.finish_read(old_response)
        self.assertIsNone(session.cache)
        session.finish_read(session.begin_read())
        self.assertEqual(session.cache, "Bob private note")

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
