CHECK_CONTEXT = False

class Repository:
    def __init__(self):
        self.records = {"private": {"organization": "north", "value": "internal draft"}, "public": {"organization": None, "value": "published guide"}}

    def read(self, principal, key):
        record = self.records[key]
        if record["organization"] not in (None, principal[1]):
            raise PermissionError("Organization does not own the document")
        return record["value"]

    def write(self, principal, key, value):
        if self.records[key]["organization"] != principal[1]:
            raise PermissionError("Organization does not own the document")
        self.records[key]["value"] = value

class Session:
    def __init__(self, repository):
        self.repository = repository
        self.principal, self.generation, self.cache = ("member-1", "north"), 0, {}

    def read(self, key):
        self.cache[key] = self.repository.read(self.principal, key)
        return self.cache[key]

    def prepare_read(self, key):
        principal, generation = self.principal, self.generation
        value = self.repository.read(principal, key)
        def deliver():
            permitted = self.generation == generation if CHECK_CONTEXT else self.principal[0] == principal[0]
            if permitted:
                self.cache[key] = value
        return deliver

    def switch_organization(self, organization):
        self.principal = (self.principal[0], organization)
        self.generation += 1
        self.cache.clear()

def cross_identity_read():
    repository = Repository()
    assert repository.read(("member-1", "south"), "public") == "published guide"
    try:
        repository.read(("member-1", "south"), "private")
    except PermissionError:
        pass
    else:
        raise AssertionError("The other organization read a private document")

def cross_identity_write():
    repository = Repository()
    try:
        repository.write(("member-1", "south"), "private", "changed")
    except PermissionError:
        pass
    else:
        raise AssertionError("The other organization changed a private document")
    assert repository.read(("member-1", "north"), "private") == "internal draft"

def identity_switch():
    session = Session(Repository())
    assert session.read("private") == "internal draft"
    pending = session.prepare_read("private")
    session.switch_organization("south")
    assert "private" not in session.cache
    pending()
    assert "private" not in session.cache, "A user's organization change is an authorization-context change"
    assert session.read("public") == "published guide"

SCENARIOS = {"cross-identity-read": cross_identity_read, "cross-identity-write": cross_identity_write, "identity-switch": identity_switch}

def report():
    import os
    import traceback
    import xml.etree.ElementTree as ET
    suite = ET.Element("testsuite", name="independent-scenarios")
    failures = 0
    for name, scenario in SCENARIOS.items():
        case = ET.SubElement(suite, "testcase", name=name, classname="contract")
        try:
            scenario()
        except Exception:
            failures += 1
            ET.SubElement(case, "failure", message="scenario assertion failed").text = traceback.format_exc()
    suite.set("tests", str(len(SCENARIOS)))
    suite.set("failures", str(failures))
    suite.set("errors", "0")
    suite.set("skipped", "0")
    ET.ElementTree(suite).write(os.environ["REQFILE_SCENARIO_REPORT"], encoding="utf-8", xml_declaration=True)
    raise SystemExit(1 if failures else 0)

if __name__ == "__main__":
    report()
