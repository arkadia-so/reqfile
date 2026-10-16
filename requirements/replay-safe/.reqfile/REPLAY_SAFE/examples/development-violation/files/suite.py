import unittest

BUG = True

class Ledger:
    def __init__(self):
        self.applied = set()
        self.balance = 0

    def apply(self, operation, fail_after_effect=False):
        if not BUG and operation in self.applied:
            return
        self.balance += 1
        self.applied.add(operation)
        if fail_after_effect:
            raise ConnectionError("Acknowledgement lost")

class Scenarios(unittest.TestCase):
    def test_same_operation(self):
        ledger = Ledger()
        ledger.apply("order-1")
        ledger.apply("order-1")
        self.assertEqual(ledger.balance, 1)

    def test_distinct_operations(self):
        ledger = Ledger()
        ledger.apply("order-1")
        ledger.apply("order-2")
        self.assertEqual(ledger.balance, 2)

    def test_retry_after_failure(self):
        ledger = Ledger()
        with self.assertRaises(ConnectionError):
            ledger.apply("order-1", fail_after_effect=True)
        ledger.apply("order-1")
        self.assertEqual(ledger.balance, 1)

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
