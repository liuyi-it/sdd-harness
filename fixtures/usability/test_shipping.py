import unittest

from shipping import shipping_fee


class ShippingTest(unittest.TestCase):
    def test_free_shipping_boundary(self):
        for amount, expected in [(0, 10), (99, 10), (100, 0), (101, 0)]:
            with self.subTest(amount=amount):
                self.assertEqual(shipping_fee(amount), expected)

    def test_rejects_negative_amount(self):
        with self.assertRaises(ValueError):
            shipping_fee(-1)
