CHECK_INCARNATION = True

class Resource:
    def __init__(self):
        self.closed = 0

    def close(self):
        self.closed += 1

class Owner:
    def __init__(self, registry, name):
        self.registry, self.name = registry, name
        self.active, self.value, self.tasks = True, None, []
        registry[name] = self

    def start(self):
        task = Task(self)
        self.tasks.append(task)
        return task

    def close(self):
        self.active = False
        for task in self.tasks:
            task.stop()
        if self.registry.get(self.name) is self:
            del self.registry[self.name]

class Task:
    def __init__(self, owner):
        self.owner, self.resource, self.running = owner, Resource(), True

    def stop(self):
        if self.running:
            self.running = False
            self.resource.close()

    def fail(self):
        self.stop()

    def dependency_callback(self):
        owner = self.owner
        # An external API cannot cancel a callback already queued for delivery.
        def complete(value):
            current = owner.registry.get(owner.name)
            if current is not None and (not CHECK_INCARNATION or current is owner):
                current.value = value
        return complete

def owner_close():
    owner = Owner({}, "search")
    task = owner.start()
    assert task.running
    owner.close()
    assert not task.running
    assert not owner.active

def late_completion():
    registry = {}
    old_owner = Owner(registry, "search")
    complete = old_owner.start().dependency_callback()
    old_owner.close()
    replacement = Owner(registry, "search")
    complete("obsolete search result")
    assert old_owner.value is None
    assert replacement.value is None, "A reused owner name must not accept the former incarnation's callback"

def failure_cleanup():
    owner = Owner({}, "search")
    task = owner.start()
    task.fail()
    owner.close()
    owner.close()
    assert task.resource.closed == 1
    assert not task.running

SCENARIOS = {"owner-close": owner_close, "late-completion": late_completion, "failure-cleanup": failure_cleanup}

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
