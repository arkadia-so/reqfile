import threading
import unittest

BUG = True

class Reservation:
    def __init__(self):
        self.barrier = threading.Barrier(2)
        self.lock = threading.Lock()
        self.capacity = 1
        self.bookings = []

    def reserve(self, name):
        observed = self.capacity
        self.barrier.wait(timeout=2)
        with self.lock:
            available = observed if BUG else self.capacity
            if available:
                self.capacity = available - 1
                self.bookings.append(name)
                return True
            return False

def contend():
    reservation = Reservation()
    results = []
    threads = [threading.Thread(target=lambda name=name: results.append(reservation.reserve(name)))
               for name in ("Acme", "Example")]
    for thread in threads:
        thread.start()
    for thread in threads:
        thread.join(timeout=3)
        assert not thread.is_alive()
    return reservation, results

class Scenarios(unittest.TestCase):
    def test_competing_transitions(self):
        reservation, outcomes = contend()
        self.assertEqual(len(outcomes), 2)
        self.assertEqual(sum(outcomes), 1)

    def test_invariant_after_contention(self):
        reservation, outcomes = contend()
        self.assertEqual(reservation.capacity + len(reservation.bookings), 1)
        self.assertEqual(len(reservation.bookings), sum(outcomes))

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
