"""PayPal REST client — SANDBOX ONLY — plus a no-network mock.

The base URL is hardcoded to the sandbox. There is no live/production
mode in this extension: real money can never move through this code.
Credentials come from environment variables only, never from files or
code: PAYPAL_CLIENT_ID and PAYPAL_CLIENT_SECRET.
"""

from __future__ import annotations

import base64
import json
import os
import time
import urllib.error
import urllib.request

from .intent import PaymentIntent

SANDBOX_BASE_URL = "https://api-m.sandbox.paypal.com"


class PayPalError(RuntimeError):
    """Raised for credential, transport, or PayPal API failures."""


def _default_request(method, url, headers, data, auth):
    """method, url, headers, data (bytes|None), auth (id, secret)|None -> dict."""
    req = urllib.request.Request(url, data=data, headers=headers, method=method)
    if auth is not None:
        token = base64.b64encode(f"{auth[0]}:{auth[1]}".encode()).decode()
        req.add_header("Authorization", f"Basic {token}")
    try:
        with urllib.request.urlopen(req, timeout=30.0) as resp:
            raw = resp.read().decode("utf-8")
            return json.loads(raw) if raw else {}
    except urllib.error.HTTPError as exc:
        body = exc.read().decode("utf-8", "replace")[:500]
        raise PayPalError(
            f"PayPal {method} {url} failed ({exc.code}): {body}"
        ) from exc
    except (urllib.error.URLError, TimeoutError, OSError) as exc:
        raise PayPalError(f"PayPal request failed: {exc}") from exc


class PayPalClient:
    """Real PayPal Sandbox client (REST v2 checkout orders)."""

    def __init__(
        self,
        client_id: str | None = None,
        client_secret: str | None = None,
        base_url: str = SANDBOX_BASE_URL,
        request_fn=None,
    ):
        self.client_id = client_id or os.environ.get("PAYPAL_CLIENT_ID", "")
        self.client_secret = client_secret or os.environ.get(
            "PAYPAL_CLIENT_SECRET", ""
        )
        if not self.client_id or not self.client_secret:
            raise PayPalError(
                "PAYPAL_CLIENT_ID and PAYPAL_CLIENT_SECRET must be set "
                "for sandbox mode."
            )
        if "sandbox" not in base_url:
            raise PayPalError(
                "refusing non-sandbox PayPal base URL: "
                "this extension is sandbox-only."
            )
        self.base_url = base_url.rstrip("/")
        self._request = request_fn or _default_request
        self._token: str | None = None
        self._token_expires_at = 0.0

    def _access_token(self) -> str:
        if self._token and time.time() < self._token_expires_at - 30:
            return self._token
        data = self._request(
            "POST",
            self.base_url + "/v1/oauth2/token",
            {
                "Content-Type": "application/x-www-form-urlencoded",
                "Accept": "application/json",
            },
            b"grant_type=client_credentials",
            (self.client_id, self.client_secret),
        )
        self._token = data["access_token"]
        self._token_expires_at = time.time() + int(data.get("expires_in", 3600))
        return self._token

    def create_order(self, intent: PaymentIntent) -> dict:
        """Create a CAPTURE order for the approved intent. Returns the order."""
        token = self._access_token()
        body = {
            "intent": "CAPTURE",
            "purchase_units": [
                {
                    "amount": {
                        "currency_code": intent.currency,
                        "value": intent.amount,
                    },
                    "payee": {"email_address": intent.recipient},
                    "description": intent.purpose or "Approve-to-Pay",
                }
            ],
        }
        return self._request(
            "POST",
            self.base_url + "/v2/checkout/orders",
            {
                "Content-Type": "application/json",
                "Authorization": f"Bearer {token}",
            },
            json.dumps(body).encode("utf-8"),
            None,
        )

    def capture_order(self, order_id: str) -> dict:
        """Capture a previously created order. Returns the capture."""
        token = self._access_token()
        return self._request(
            "POST",
            self.base_url + f"/v2/checkout/orders/{order_id}/capture",
            {
                "Content-Type": "application/json",
                "Authorization": f"Bearer {token}",
            },
            b"{}",
            None,
        )


class MockPayPalClient:
    """No-network stand-in. Order/capture ids are clearly fake.

    ``created_orders`` / ``captured_orders`` let tests prove that a
    rejected payment made zero PayPal calls.
    """

    def __init__(self):
        self.created_orders: list[dict] = []
        self.captured_orders: list[dict] = []

    def create_order(self, intent: PaymentIntent) -> dict:
        order = {
            "id": f"MOCK-ORDER-{len(self.created_orders) + 1:04d}",
            "status": "CREATED",
            "intent": intent.to_dict(),
        }
        self.created_orders.append(order)
        return order

    def capture_order(self, order_id: str) -> dict:
        capture = {
            "id": f"MOCK-CAPTURE-{len(self.captured_orders) + 1:04d}",
            "status": "COMPLETED",
            "order_id": order_id,
        }
        self.captured_orders.append(capture)
        return capture
