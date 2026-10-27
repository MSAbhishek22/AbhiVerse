import unittest

from approve_to_pay.intent import (
    IntentParseError,
    parse_heuristic,
    validate_intent,
)


class TestHeuristicParser(unittest.TestCase):
    def test_dollar_amount_and_purpose(self):
        intent = parse_heuristic("pay $25 to alice@example.com for the design work")
        self.assertEqual(intent.amount, "25.00")
        self.assertEqual(intent.currency, "USD")
        self.assertEqual(intent.recipient, "alice@example.com")
        self.assertEqual(intent.purpose, "the design work")

    def test_currency_word(self):
        intent = parse_heuristic("send 100 USD to bob@example.com")
        self.assertEqual((intent.amount, intent.currency), ("100.00", "USD"))
        self.assertEqual(intent.purpose, "")

    def test_euro_symbol_and_decimal(self):
        intent = parse_heuristic("pay €50.50 to carol@example.com for dinner")
        self.assertEqual((intent.amount, intent.currency), ("50.50", "EUR"))
        self.assertEqual(intent.purpose, "dinner")

    def test_rupee_word(self):
        intent = parse_heuristic("pay 500 rupees to dave@example.com for rent")
        self.assertEqual((intent.amount, intent.currency), ("500.00", "INR"))

    def test_purpose_before_recipient(self):
        intent = parse_heuristic("pay $25 for lunch to eve@example.com")
        self.assertEqual(intent.purpose, "lunch")
        self.assertEqual(intent.recipient, "eve@example.com")

    def test_missing_amount(self):
        with self.assertRaises(IntentParseError):
            parse_heuristic("pay alice@example.com for lunch")

    def test_missing_recipient(self):
        with self.assertRaises(IntentParseError):
            parse_heuristic("pay $25 for lunch")

    def test_bare_number_without_currency_marker(self):
        # A bare number is ambiguous (which currency?) — refuse, don't guess.
        with self.assertRaises(IntentParseError):
            parse_heuristic("pay 25 to frank@example.com")

    def test_empty(self):
        with self.assertRaises(IntentParseError):
            parse_heuristic("   ")


class TestValidateIntent(unittest.TestCase):
    def test_zero_amount(self):
        with self.assertRaises(IntentParseError):
            validate_intent("0", "USD", "a@example.com")

    def test_unsupported_currency(self):
        with self.assertRaises(IntentParseError):
            validate_intent("10", "JPY", "a@example.com")

    def test_bad_email(self):
        with self.assertRaises(IntentParseError):
            validate_intent("10", "USD", "not-an-email")

    def test_normalizes_amount_and_currency(self):
        intent = validate_intent("25.5", "usd", "a@example.com", "  work ")
        self.assertEqual(intent.amount, "25.50")
        self.assertEqual(intent.currency, "USD")
        self.assertEqual(intent.purpose, "work")


if __name__ == "__main__":
    unittest.main()
