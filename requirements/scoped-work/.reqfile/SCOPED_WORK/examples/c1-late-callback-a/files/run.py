import asyncio
import unittest


class Owner:
    def __init__(self):
        self.open = True
        self.values = []
        self.release_count = 0
        self.dependency = asyncio.get_running_loop().create_future()
        self.dependency.add_done_callback(self.publish)
        self.task = asyncio.create_task(self.run())

    async def run(self):
        try:
            await asyncio.shield(self.dependency)
        finally:
            self.release_count += 1

    def publish(self, completed):
        if completed.cancelled() or completed.exception() is not None:
            return
        if not self.open:
            return
        self.values.append(completed.result())

    async def close(self):
        self.open = False
        self.task.cancel()
        await asyncio.gather(self.task, return_exceptions=True)


class Ownership(unittest.IsolatedAsyncioTestCase):
    async def test_close_stops_owned_task(self):
        owner = Owner()
        await asyncio.sleep(0)
        await owner.close()
        self.assertTrue(owner.task.done())
        self.assertTrue(owner.task.cancelled())
        self.assertEqual(owner.release_count, 1)

    async def test_late_dependency_cannot_publish(self):
        owner = Owner()
        await asyncio.sleep(0)
        await owner.close()
        self.assertFalse(owner.dependency.cancelled())
        owner.dependency.set_result("obsolete response")
        await asyncio.sleep(0)
        self.assertEqual(owner.values, [])
        self.assertEqual(owner.release_count, 1)

    async def test_failure_releases_resource_once(self):
        owner = Owner()
        await asyncio.sleep(0)
        owner.dependency.set_exception(ConnectionError("Dependency failed"))
        with self.assertRaises(ConnectionError):
            await owner.task
        await owner.close()
        self.assertEqual(owner.release_count, 1)
        self.assertEqual(owner.values, [])

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
