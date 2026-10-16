import unittest

BUG = True

class Directory:
    def __init__(self):
        self.rows = {"acme": "Acme secret", "example": "Example secret"}

    def read(self, principal, owner):
        if principal != owner:
            raise PermissionError("Forbidden")
        return self.rows[owner]

    def write(self, principal, owner, value):
        if principal != owner:
            raise PermissionError("Forbidden")
        self.rows[owner] = value

class Session:
    def __init__(self, identity):
        self.identity = identity
        self.cached = None
        self.generation = 0

    def pending(self):
        generation = self.generation
        def complete(value):
            if generation == self.generation or BUG:
                self.cached = value
        return complete

    def switch(self, identity):
        self.identity = identity
        self.cached = None
        self.generation += 1

class Scenarios(unittest.TestCase):
    def test_cross_identity_read(self):
        directory = Directory()
        with self.assertRaises(PermissionError):
            directory.read("acme", "example")

    def test_cross_identity_write(self):
        directory = Directory()
        with self.assertRaises(PermissionError):
            directory.write("acme", "example", "changed")
        self.assertEqual(directory.read("example", "example"), "Example secret")

    def test_identity_switch(self):
        session = Session("acme")
        session.cached = "Acme secret"
        complete = session.pending()
        session.switch("example")
        self.assertIsNone(session.cached)
        complete("Late Acme secret")
        self.assertIsNone(session.cached)

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
