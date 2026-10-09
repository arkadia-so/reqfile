import unittest


class Parsing(unittest.TestCase):
    def test_empty(self):
        self.assertEqual(parse(""), [])
