import json
import os
import unittest
import unittest.mock as mock

from approve_to_pay.intent import PaymentIntent
from approve_to_pay.paypal import MockPayPalClient, PayPalClient, PayPalError

INTENT = PaymentIntent("25.00", "USD", "alice@example.com", "design work")


class FakeTransport:
    """Records calls; returns canned PayPal Sandbox responses."""

    def __init__(self):
        self.calls = []

    def __call__(self, method, url, headers, data, auth):
        self.calls.append((method, url, headers, data, auth))
        if url.endswith("/v1/oauth2/token"):
            return {
                "access_token": "TEST-TOKEN",
                "expires_in": 3600,
                "token_type": "Bearer",
            }
        if url.endswith("/v2/checkout/orders"):
            return {"id": "ORDER-123", "status": "CREATED"}
        if url.endswith("/capture"):
            return {"id": "CAPTURE-123", "status": "COMPLETED"}
        raise AssertionError(f"unexpected url {url}")


class TestPayPalClient(unittest.TestCase):
    def test_create_and_capture(self):
        transport = FakeTransport()
        client = PayPalClient("cid", "secret", request_fn=transport)
        order = client.create_order(INTENT)
        capture = client.capture_order(order["id"])
        self.assertEqual(order["id"], "ORDER-123")
        self.assertEqual(capture["status"], "COMPLETED")

        # Access token fetched once, then cached for the second call.
        token_calls = [
            c for c in transport.calls if c[1].endswith("/v1/oauth2/token")
        ]
        self.assertEqual(len(token_calls), 1)

        # Create body carries the amount and the payee email.
        create = [
            c for c in transport.calls if c[1].endswith("/v2/checkout/orders")
        ][0]
        body = json.loads(create[3])
        self.assertEqual(
            body["purchase_units"][0]["amount"],
            {"currency_code": "USD", "value": "25.00"},
        )
        self.assertEqual(
            body["purchase_units"][0]["payee"],
            {"email_address": "alice@example.com"},
        )

        # Capture URL carries the order id.
        cap = [c for c in transport.calls if c[1].endswith("/capture")][0]
        self.assertIn("ORDER-123", cap[1])

    def test_missing_credentials(self):
        env = {
            k: v
            for k, v in os.environ.items()
            if k not in ("PAYPAL_CLIENT_ID", "PAYPAL_CLIENT_SECRET")
        }
        with mock.patch.dict(os.environ, env, clear=True):
            with self.assertRaises(PayPalError):
                PayPalClient("", "")

    def test_refuses_non_sandbox_url(self):
        with self.assertRaises(PayPalError):
            PayPalClient("cid", "secret", base_url="https://api-m.paypal.com")

    def test_transport_error_propagates(self):
        def boom(method, url, headers, data, auth):
            raise PayPalError("503 down")

        client = PayPalClient("cid", "secret", request_fn=boom)
        with self.assertRaises(PayPalError):
            client.create_order(INTENT)


class TestMockPayPalClient(unittest.TestCase):
    def test_fake_ids(self):
        client = MockPayPalClient()
        order = client.create_order(INTENT)
        capture = client.capture_order(order["id"])
        self.assertTrue(order["id"].startswith("MOCK-ORDER-"))
        self.assertTrue(capture["id"].startswith("MOCK-CAPTURE-"))
        self.assertEqual(capture["status"], "COMPLETED")


if __name__ == "__main__":
    unittest.main()
