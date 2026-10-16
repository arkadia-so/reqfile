import unittest

BUG = True

class Fulfillment:
    def __init__(self):
        self.charges = 0
        self.deliveries = 0
        self.progress = set()

    def recover(self, fail_after_charge=False):
        if "charged" not in self.progress:
            self.charges += 1
            if not BUG:
                self.progress.add("charged")
        if fail_after_charge:
            raise ConnectionError("Delivery unavailable")
        if "delivered" not in self.progress:
            self.deliveries += 1
            self.progress.add("delivered")
        self.progress.add("charged")

def partially_apply():
    operation = Fulfillment()
    try:
        operation.recover(fail_after_charge=True)
    except ConnectionError:
        pass
    return operation

class Scenarios(unittest.TestCase):
    def test_partial_failure(self):
        operation = partially_apply()
        self.assertEqual(operation.charges, 1)
        self.assertEqual(operation.deliveries, 0)
        self.assertEqual(operation.progress, {"charged"})

    def test_recovery(self):
        operation = partially_apply()
        operation.recover()
        self.assertEqual((operation.charges, operation.deliveries), (1, 1))

    def test_repeated_recovery(self):
        operation = partially_apply()
        operation.recover()
        operation.recover()
        self.assertEqual((operation.charges, operation.deliveries), (1, 1))

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
