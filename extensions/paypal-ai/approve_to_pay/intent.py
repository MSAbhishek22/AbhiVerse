"""Structured payment intents and the offline heuristic parser.

The heuristic parser is NOT the AI path. It exists so the pipeline is
testable offline with no key and no network. The real AI path is
``llm.parse_with_llm``. Anything that reports which parser ran must say
so honestly (see ``pipeline.PaymentResult.parser``).
"""

from __future__ import annotations

import re
from dataclasses import asdict, dataclass
from decimal import Decimal, InvalidOperation

SUPPORTED_CURRENCIES = {"USD", "EUR", "GBP", "INR"}

_SYMBOL_TO_CURRENCY = {"$": "USD", "€": "EUR", "£": "GBP", "₹": "INR"}
_WORD_TO_CURRENCY = {
    "dollar": "USD", "dollars": "USD", "usd": "USD",
    "euro": "EUR", "euros": "EUR", "eur": "EUR",
    "pound": "GBP", "pounds": "GBP", "gbp": "GBP",
    "rupee": "INR", "rupees": "INR", "inr": "INR",
}

_EMAIL_RE = re.compile(r"[A-Za-z0-9._%+-]+@[A-Za-z0-9.-]+\.[A-Za-z]{2,}")

_AMOUNT_RE = re.compile(
    r"(?P<symbol>[$€£₹])\s*(?P<sym_num>\d+(?:\.\d{1,2})?)"
    r"|(?P<word_num>\d+(?:\.\d{1,2})?)\s*(?P<word>[A-Za-z]{3,})",
    re.IGNORECASE,
)


class IntentParseError(ValueError):
    """Raised when an instruction cannot be turned into a valid intent."""


@dataclass(frozen=True)
class PaymentIntent:
    amount: str  # decimal string, always 2dp, e.g. "25.00"
    currency: str  # ISO 4217 code, one of SUPPORTED_CURRENCIES
    recipient: str  # PayPal payee email address
    purpose: str = ""

    def to_dict(self) -> dict:
        return asdict(self)


def validate_intent(amount, currency, recipient, purpose="") -> PaymentIntent:
    """Validate raw fields and return a normalized PaymentIntent."""
    try:
        value = Decimal(str(amount)).quantize(Decimal("0.01"))
    except (InvalidOperation, ValueError) as exc:
        raise IntentParseError(f"invalid amount: {amount!r}") from exc
    if value <= 0:
        raise IntentParseError(f"amount must be positive, got {amount!r}")
    currency = str(currency).upper()
    if currency not in SUPPORTED_CURRENCIES:
        raise IntentParseError(f"unsupported currency: {currency!r}")
    recipient = str(recipient).strip()
    if not _EMAIL_RE.fullmatch(recipient):
        raise IntentParseError(
            f"recipient must be an email address, got {recipient!r}"
        )
    return PaymentIntent(
        amount=str(value),
        currency=currency,
        recipient=recipient,
        purpose=str(purpose or "").strip(),
    )


def _parse_amount(text: str) -> tuple[str, str]:
    """Return (amount, currency). Amount needs a symbol or currency word."""
    for match in _AMOUNT_RE.finditer(text):
        if match.group("symbol"):
            return match.group("sym_num"), _SYMBOL_TO_CURRENCY[match.group("symbol")]
        word = match.group("word").lower()
        if word in _WORD_TO_CURRENCY:
            return match.group("word_num"), _WORD_TO_CURRENCY[word]
    raise IntentParseError(f"no amount found in: {text!r}")


def _parse_purpose(text: str) -> str:
    """Text after the first " for ", with any "to <email>" clause dropped."""
    idx = text.lower().find(" for ")
    if idx == -1:
        return ""
    purpose = _EMAIL_RE.sub("", text[idx + 5:])
    purpose = re.sub(r"\s+", " ", purpose).strip()
    purpose = re.sub(r"\bto\s*$", "", purpose).strip()
    return purpose


def parse_heuristic(instruction: str) -> PaymentIntent:
    """Offline fallback parser. Clearly labelled, never presented as AI."""
    if not instruction or not instruction.strip():
        raise IntentParseError("empty instruction")
    text = instruction.strip()
    amount, currency = _parse_amount(text)
    email = _EMAIL_RE.search(text)
    if not email:
        raise IntentParseError(f"no recipient email found in: {text!r}")
    return validate_intent(amount, currency, email.group(0), _parse_purpose(text))
