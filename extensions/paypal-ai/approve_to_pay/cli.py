"""CLI demo for Approve-to-Pay.

Mock mode (default, no credentials, no network)::

    cd extensions/paypal-ai
    python3 -m approve_to_pay.cli --mock --auto-approve \\
        "pay $25 to alice@example.com for the design work"

Sandbox mode (needs free PayPal developer sandbox keys in env)::

    PAYPAL_CLIENT_ID=... PAYPAL_CLIENT_SECRET=... \\
        python3 -m approve_to_pay.cli --sandbox \\
        "pay $25 to alice@example.com for the design work"

Without --auto-approve, approval is asked on stdin (standing in for the
paired phone). Exit code 0 on COMPLETED, 1 otherwise.
"""

from __future__ import annotations

import argparse
import json
import sys

from .approval import MockApprover, TerminalApprover
from .paypal import MockPayPalClient, PayPalClient
from .pipeline import run_payment


def build_parser() -> argparse.ArgumentParser:
    parser = argparse.ArgumentParser(
        description="Approve-to-Pay: natural-language instruction to PayPal Sandbox payment."
    )
    parser.add_argument("instruction", help='e.g. "pay $25 to alice@example.com for the design work"')
    parser.add_argument(
        "--mode",
        choices=["mock", "sandbox"],
        default="mock",
        help="mock = no network, fake ids (default); sandbox = real PayPal Sandbox API",
    )
    parser.add_argument(
        "--mock",
        action="store_true",
        help="shorthand for --mode mock",
    )
    parser.add_argument(
        "--sandbox",
        action="store_true",
        help="shorthand for --mode sandbox (needs PAYPAL_CLIENT_ID/SECRET)",
    )
    parser.add_argument(
        "--parser",
        choices=["auto", "llm", "heuristic"],
        default="auto",
        help="auto = LLM if a key is set, else heuristic (default)",
    )
    parser.add_argument(
        "--auto-approve",
        action="store_true",
        help="approve without asking (demos/tests only; never the real flow)",
    )
    return parser


def main(argv=None) -> int:
    args = build_parser().parse_args(argv)
    if args.mock and args.sandbox:
        print("error: --mock and --sandbox are mutually exclusive", file=sys.stderr)
        return 2
    mode = "sandbox" if args.sandbox else "mock" if args.mock else args.mode
    approver = MockApprover(approve=True) if args.auto_approve else TerminalApprover()
    client = MockPayPalClient() if mode == "mock" else PayPalClient()
    result = run_payment(
        args.instruction,
        approver=approver,
        paypal_client=client,
        mode=mode,
        parser=args.parser,
    )
    print(json.dumps(result.to_dict(), indent=2))
    return 0 if result.status == "COMPLETED" else 1


if __name__ == "__main__":
    sys.exit(main())
