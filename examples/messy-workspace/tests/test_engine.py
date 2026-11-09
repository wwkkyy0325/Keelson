import unittest

from app.engine import Engine


class EngineTest(unittest.TestCase):
    def test_a_total_below_the_threshold_is_unchanged(self):
        self.assertEqual(Engine().rule_017({"total": 5}), 5)

    def test_a_damaged_region_rule_subtracts_its_number(self):
        self.assertEqual(Engine().rule_017({"total": 100}), 83)

    def test_the_second_damaged_region_subtracts_its_number(self):
        self.assertEqual(Engine().rule_062({"total": 100}), 38)

    def test_a_rule_after_every_damaged_region_still_works(self):
        self.assertEqual(Engine().rule_150({"total": 1000}), 850)

    def test_apply_records_the_rule(self):
        engine = Engine()
        self.assertEqual(engine.apply("rule_003", {"total": 10}), 7)
        self.assertEqual(engine.applied, ["rule_003"])


if __name__ == "__main__":
    unittest.main()
