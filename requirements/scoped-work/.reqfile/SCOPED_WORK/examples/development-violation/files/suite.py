import unittest

BUG = True

class Owner:
    def __init__(self):
        self.live = True
        self.active = True
        self.releases = 0
        self.value = None

    def close(self):
        if self.live:
            self.live = False
            self.active = False
            self.releases += 1

    def complete(self, value):
        if self.live or BUG:
            self.value = value

    def fail(self):
        self.close()

class Scenarios(unittest.TestCase):
    def test_owner_close(self):
        owner = Owner()
        owner.close()
        self.assertFalse(owner.active)
        self.assertEqual(owner.releases, 1)

    def test_late_completion(self):
        owner = Owner()
        callback = owner.complete
        owner.close()
        callback("obsolete response")
        self.assertIsNone(owner.value)

    def test_failure_cleanup(self):
        owner = Owner()
        owner.fail()
        owner.close()
        self.assertEqual(owner.releases, 1)

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
