import os
import unittest
import unittest.mock as mock

from approve_to_pay.intent import PaymentIntent
from approve_to_pay.llm import LLMError, LLMNotConfigured, parse_with_llm

CONTENT = (
    '{"amount": "25.00", "currency": "USD", '
    '"recipient": "alice@example.com", "purpose": "design work"}'
)


def fake_ok(url, payload, headers):
    fake_ok.seen = (url, payload, headers)
    return {"choices": [{"message": {"content": CONTENT}}]}


class TestLLMParser(unittest.TestCase):
    def test_parses_json_response(self):
        intent = parse_with_llm(
            "pay $25 to alice@example.com", api_key="test-key", request_fn=fake_ok
        )
        self.assertEqual(
            intent, PaymentIntent("25.00", "USD", "alice@example.com", "design work")
        )
        url, payload, headers = fake_ok.seen
        self.assertTrue(url.endswith("/chat/completions"))
        self.assertEqual(headers["Authorization"], "Bearer test-key")
        self.assertEqual(
            payload["messages"][1]["content"], "pay $25 to alice@example.com"
        )

    def test_fenced_json_is_unwrapped(self):
        def fake(url, payload, headers):
            return {
                "choices": [
                    {"message": {"content": "```json\n" + CONTENT + "\n```"}}
                ]
            }

        intent = parse_with_llm("...", api_key="k", request_fn=fake)
        self.assertEqual(intent.amount, "25.00")

    def test_no_key_raises_not_configured(self):
        env = {
            k: v
            for k, v in os.environ.items()
            if k != "APPROVE_TO_PAY_LLM_API_KEY"
        }
        with mock.patch.dict(os.environ, env, clear=True):
            with self.assertRaises(LLMNotConfigured):
                parse_with_llm("pay $25 to alice@example.com")

    def test_invalid_json_raises(self):
        def fake(url, payload, headers):
            return {"choices": [{"message": {"content": "not json at all"}}]}

        with self.assertRaises(LLMError):
            parse_with_llm("...", api_key="k", request_fn=fake)

    def test_invalid_intent_raises(self):
        def fake(url, payload, headers):
            return {
                "choices": [
                    {
                        "message": {
                            "content": '{"amount": "25.00", "currency": "USD", '
                            '"recipient": "nobody", "purpose": ""}'
                        }
                    }
                ]
            }

        with self.assertRaises(LLMError):
            parse_with_llm("...", api_key="k", request_fn=fake)


if __name__ == "__main__":
    unittest.main()
