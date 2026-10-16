import unittest


class Ledger:
    def __init__(self):
        self.applied = set()
        self.entries = []

    def deliver(self, operation, amount, lose_ack=False):
        if operation in self.applied:
            return "already-applied"
        self.entries.append((operation, amount))
        self.applied.add(operation)
        if lose_ack:
            raise ConnectionError("Acknowledgement connection closed")
        return "applied"


class Deliveries(unittest.TestCase):
    def test_redelivery_keeps_one_entry(self):
        ledger = Ledger()
        ledger.deliver("order-81", 12)
        ledger.deliver("order-81", 12)
        self.assertEqual(ledger.entries, [("order-81", 12)])

    def test_distinct_orders_keep_distinct_entries(self):
        ledger = Ledger()
        ledger.deliver("order-81", 12)
        ledger.deliver("order-82", 12)
        self.assertEqual(ledger.entries, [("order-81", 12), ("order-82", 12)])

    def test_lost_acknowledgement_does_not_charge_twice(self):
        ledger = Ledger()
        with self.assertRaises(ConnectionError):
            ledger.deliver("order-81", 12, lose_ack=True)
        ledger.deliver("order-81", 12)
        self.assertEqual(ledger.entries, [("order-81", 12)])

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
    result.cases = result.cases[1:]
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
