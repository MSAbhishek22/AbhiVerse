"""Approval gates: nothing moves without an explicit approval.

This is the Abhiverse security story carried into payments: the parsed
intent is presented for approval BEFORE any PayPal call is made, and a
rejection (or no answer) aborts the payment with zero PayPal API calls.

Production seam: ``TerminalApprover`` stands in for the paired phone.
The real integration replaces it with a gate that sends the intent JSON
to the paired controller over the Abhiverse protocol and waits for the
controller's signed approval. That wiring is NOT in this prototype —
see README "What still needs a human".
"""

from __future__ import annotations

from .intent import PaymentIntent


class ApprovalGate:
    """Something that can approve or reject a parsed payment intent."""

    def request_approval(self, intent: PaymentIntent) -> bool:
        raise NotImplementedError


class MockApprover(ApprovalGate):
    """Always answers the configured way. Tests and demos ONLY."""

    def __init__(self, approve: bool = True):
        self.approve = approve
        self.seen: list[PaymentIntent] = []

    def request_approval(self, intent: PaymentIntent) -> bool:
        self.seen.append(intent)
        return self.approve


class TerminalApprover(ApprovalGate):
    """Asks a human on stdin. Stands in for the paired phone's approval."""

    def request_approval(self, intent: PaymentIntent) -> bool:
        print("Approve this payment?")
        print(f"  Amount:    {intent.amount} {intent.currency}")
        print(f"  Recipient: {intent.recipient}")
        print(f"  Purpose:   {intent.purpose or '(none)'}")
        answer = input("Type 'yes' to approve, anything else to reject: ")
        return answer.strip().lower() in {"y", "yes"}
