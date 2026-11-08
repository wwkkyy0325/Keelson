import unittest

from shop.api import quote
from shop.checkout import checkout
from shop.inventory import stock_value
from shop.models import Item
from shop.pricing import category_total, price_breakdown, subtotal, total
from shop.reporting import invoice, render
from shop.shipping import shipping_quote
from shop.wholesale import wholesale_total


class DiscountTest(unittest.TestCase):
    def setUp(self):
        self.items = [Item("pen", 2.0, 3, "office")]

    def test_total_plain(self):
        self.assertEqual(total(self.items), 6.0)

    def test_total_tax_plain(self):
        self.assertEqual(total(self.items, include_tax=True), 6.48)

    def test_subtotal_discount(self):
        self.assertEqual(subtotal(self.items, discount=0.10), 5.40)

    def test_total_discount(self):
        self.assertEqual(total(self.items, discount=0.10), 5.40)

    def test_total_tax_discount(self):
        self.assertAlmostEqual(
            total(self.items, include_tax=True, discount=0.10), 5.832, places=3
        )

    def test_category_discount(self):
        self.assertEqual(category_total(self.items, "office", discount=0.10), 5.40)

    def test_breakdown_discount(self):
        self.assertEqual(price_breakdown(self.items, discount=0.10)["total"], 5.40)

    def test_render_discount(self):
        self.assertEqual(render(self.items, discount=0.10), "total: 5.40")

    def test_invoice_discount(self):
        self.assertEqual(invoice(self.items, discount=0.10)["total"], 5.40)

    def test_checkout_discount(self):
        self.assertEqual(checkout(self.items, discount=0.10), 5.40)

    def test_quote_discount(self):
        self.assertEqual(quote(self.items, discount=0.10)["total"], 5.40)

    def test_wholesale_discount(self):
        self.assertEqual(wholesale_total(self.items, discount=0.10), 5.40)

    def test_stock_value_discount(self):
        self.assertEqual(stock_value(self.items, discount=0.10), 5.40)

    def test_shipping_discount(self):
        self.assertEqual(shipping_quote(self.items, discount=0.10), 10.40)


if __name__ == "__main__":
    unittest.main()
