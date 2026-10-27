"""Orchestration: parse -> approve -> (sandbox) create + capture.

Parser selection for ``parser="auto"`` (the default):
- LLM key present  -> the LLM path (the real AI path).
- LLM key absent   -> the heuristic fallback, and the result says so.
"auto" never hides which parser ran: ``PaymentResult.parser`` records it.
"""

from __future__ import annotations

import os
from dataclasses import asdict, dataclass

from .approval import ApprovalGate
from .intent import PaymentIntent, parse_heuristic
from .llm import parse_with_llm


@dataclass
class PaymentResult:
    status: str  # COMPLETED | ABORTED_NOT_APPROVED
    mode: str  # mock | sandbox
    parser: str  # llm | heuristic
    intent: PaymentIntent | None = None
    order_id: str | None = None
    capture_id: str | None = None
    message: str = ""

    def to_dict(self) -> dict:
        data = asdict(self)
        data["intent"] = self.intent.to_dict() if self.intent else None
        return data


def run_payment(
    instruction: str,
    *,
    approver: ApprovalGate,
    paypal_client,
    mode: str,
    parser: str = "auto",
) -> PaymentResult:
    """Run one payment end to end. Returns a result; never raises on reject."""
    if parser == "auto":
        chosen = "llm" if os.environ.get("APPROVE_TO_PAY_LLM_API_KEY") else "heuristic"
    elif parser in ("llm", "heuristic"):
        chosen = parser
    else:
        raise ValueError(f"unknown parser: {parser!r} (want auto|llm|heuristic)")

    intent = (
        parse_with_llm(instruction) if chosen == "llm" else parse_heuristic(instruction)
    )

    # Approval gate: a rejection means zero PayPal calls, by construction —
    # create_order/capture_order are only reachable below this check.
    if not approver.request_approval(intent):
        return PaymentResult(
            status="ABORTED_NOT_APPROVED",
            mode=mode,
            parser=chosen,
            intent=intent,
            message="Payment rejected at approval; no PayPal call was made.",
        )

    order = paypal_client.create_order(intent)
    capture = paypal_client.capture_order(order["id"])
    return PaymentResult(
        status=capture.get("status", "UNKNOWN"),
        mode=mode,
        parser=chosen,
        intent=intent,
        order_id=order["id"],
        capture_id=capture.get("id"),
        message=f"Order {order['id']} captured: {capture.get('status')}.",
    )
