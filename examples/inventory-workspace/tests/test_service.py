import unittest

from inventory.models import Item
from inventory.report import render
from inventory.service import total


class ServiceTest(unittest.TestCase):
    def test_total(self):
        items = [Item("pen", 2.0, 3)]
        self.assertEqual(total(items), 6.0)

    def test_tax(self):
        items = [Item("pen", 2.0, 3)]
        self.assertEqual(total(items, include_tax=True), 6.48)

    def test_discount(self):
        items = [Item("pen", 2.0, 3)]
        self.assertEqual(total(items, discount=0.10), 5.40)

    def test_tax_and_discount(self):
        items = [Item("pen", 2.0, 3)]
        self.assertAlmostEqual(
            total(items, include_tax=True, discount=0.10), 5.832, places=3
        )

    def test_report(self):
        items = [Item("pen", 2.0, 3)]
        self.assertEqual(render(items), "total: 6.00")

    def test_report_with_discount(self):
        items = [Item("pen", 2.0, 3)]
        self.assertEqual(render(items, discount=0.10), "total: 5.40")


if __name__ == "__main__":
    unittest.main()
