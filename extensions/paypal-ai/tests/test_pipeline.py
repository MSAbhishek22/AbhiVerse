import os
import unittest
import unittest.mock as mock

from approve_to_pay.approval import MockApprover
from approve_to_pay.intent import PaymentIntent
from approve_to_pay.paypal import MockPayPalClient
from approve_to_pay.pipeline import run_payment

INSTR = "pay $25 to alice@example.com for the design work"


def _no_llm_key_env():
    return {
        k: v
        for k, v in os.environ.items()
        if k != "APPROVE_TO_PAY_LLM_API_KEY"
    }


class TestPipeline(unittest.TestCase):
    def test_approved_mock_payment_completes(self):
        client = MockPayPalClient()
        with mock.patch.dict(os.environ, _no_llm_key_env(), clear=True):
            result = run_payment(
                INSTR,
                approver=MockApprover(True),
                paypal_client=client,
                mode="mock",
            )
        self.assertEqual(result.status, "COMPLETED")
        self.assertEqual(result.mode, "mock")
        self.assertEqual(result.parser, "heuristic")
        self.assertEqual(
            result.intent,
            PaymentIntent("25.00", "USD", "alice@example.com", "the design work"),
        )
        self.assertTrue(result.order_id.startswith("MOCK-ORDER-"))
        self.assertTrue(result.capture_id.startswith("MOCK-CAPTURE-"))
        self.assertEqual(len(client.created_orders), 1)
        self.assertEqual(len(client.captured_orders), 1)

    def test_rejected_payment_makes_zero_paypal_calls(self):
        client = MockPayPalClient()
        with mock.patch.dict(os.environ, _no_llm_key_env(), clear=True):
            result = run_payment(
                INSTR,
                approver=MockApprover(False),
                paypal_client=client,
                mode="mock",
            )
        self.assertEqual(result.status, "ABORTED_NOT_APPROVED")
        self.assertEqual(client.created_orders, [])
        self.assertEqual(client.captured_orders, [])

    def test_auto_uses_llm_when_key_present(self):
        intent = PaymentIntent("10.00", "USD", "bob@example.com", "x")
        with mock.patch.dict(os.environ, {"APPROVE_TO_PAY_LLM_API_KEY": "k"}):
            with mock.patch(
                "approve_to_pay.pipeline.parse_with_llm", return_value=intent
            ) as llm:
                result = run_payment(
                    "...",
                    approver=MockApprover(True),
                    paypal_client=MockPayPalClient(),
                    mode="mock",
                )
        self.assertEqual(result.parser, "llm")
        llm.assert_called_once()

    def test_auto_uses_heuristic_when_no_key(self):
        with mock.patch.dict(os.environ, _no_llm_key_env(), clear=True):
            with mock.patch(
                "approve_to_pay.pipeline.parse_with_llm"
            ) as llm:
                result = run_payment(
                    INSTR,
                    approver=MockApprover(True),
                    paypal_client=MockPayPalClient(),
                    mode="mock",
                )
        self.assertEqual(result.parser, "heuristic")
        llm.assert_not_called()

    def test_unknown_parser_rejected(self):
        with self.assertRaises(ValueError):
            run_payment(
                INSTR,
                approver=MockApprover(True),
                paypal_client=MockPayPalClient(),
                mode="mock",
                parser="bogus",
            )


if __name__ == "__main__":
    unittest.main()
