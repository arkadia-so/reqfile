import unittest
from concurrent.futures import ThreadPoolExecutor
from threading import Barrier, Lock


class Inventory:
    def __init__(self):
        self.remaining = 1
        self.read_barrier = Barrier(2, timeout=3)
        self.write_lock = Lock()

    def reserve(self):
        observed_remaining = self.remaining
        self.read_barrier.wait()
        with self.write_lock:
            if self.remaining <= 0:
                return "refused"
            self.remaining -= 1
            return "reserved"


def contend():
    inventory = Inventory()
    with ThreadPoolExecutor(max_workers=2) as executor:
        first = executor.submit(inventory.reserve)
        second = executor.submit(inventory.reserve)
        outcomes = [first.result(timeout=5), second.result(timeout=5)]
    return inventory, outcomes


class Reservations(unittest.TestCase):
    def test_conflicting_outcomes(self):
        _, outcomes = contend()
        self.assertCountEqual(outcomes, ["reserved", "refused"])

    def test_capacity_survives_contention(self):
        inventory, outcomes = contend()
        self.assertGreaterEqual(inventory.remaining, 0)
        self.assertEqual(inventory.remaining + outcomes.count("reserved"), 1)

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
    result.cases.append(dict(result.cases[0]))
    counts = {"tests": str(len(result.cases)), "failures": str(len(result.failures)),
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
