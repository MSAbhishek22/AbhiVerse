"""Approve-to-Pay: natural-language payment instruction -> approved PayPal Sandbox payment.

Pipeline: parse (LLM or offline heuristic) -> explicit approval -> PayPal
Sandbox create + capture. Nothing moves without approval, and the PayPal
client is sandbox-only: real money can never move through this code.

See README.md in this directory for setup, env vars, and honest limits.
"""

from .intent import IntentParseError, PaymentIntent, parse_heuristic, validate_intent
from .pipeline import PaymentResult, run_payment

__version__ = "0.1.0"

__all__ = [
    "__version__",
    "IntentParseError",
    "PaymentIntent",
    "PaymentResult",
    "parse_heuristic",
    "run_payment",
    "validate_intent",
]
