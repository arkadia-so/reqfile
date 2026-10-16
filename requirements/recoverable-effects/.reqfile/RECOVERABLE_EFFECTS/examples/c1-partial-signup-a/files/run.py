import unittest


class Signup:
    def __init__(self):
        self.journal = set()
        self.effects = {"membership": 0, "invoice": 0}

    def complete(self, fail_after_membership=False):
        for effect in ("membership", "invoice"):
            if effect in self.journal:
                continue
            self.effects[effect] += 1
            self.journal.add(effect)
            if effect == "membership" and fail_after_membership:
                raise ConnectionError("Process stopped between effects")


class SignupRecovery(unittest.TestCase):
    def partially_applied(self):
        signup = Signup()
        with self.assertRaises(ConnectionError):
            signup.complete(fail_after_membership=True)
        return signup

    def test_partial_state_identifies_applied_effect(self):
        signup = self.partially_applied()
        self.assertEqual(signup.journal, {"membership"})
        self.assertEqual(signup.effects, {"membership": 1, "invoice": 0})

    def test_recovery_finishes_without_repeating_membership(self):
        signup = self.partially_applied()
        signup.complete()
        self.assertEqual(signup.effects, {"membership": 1, "invoice": 1})
        self.assertEqual(signup.journal, {"membership", "invoice"})

    def test_recovery_can_be_repeated(self):
        signup = self.partially_applied()
        signup.complete()
        signup.complete()
        self.assertEqual(signup.effects, {"membership": 1, "invoice": 1})

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
