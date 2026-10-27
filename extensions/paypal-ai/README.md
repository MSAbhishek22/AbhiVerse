# Approve-to-Pay (PayPal AI Hackathon 2026 extension)

A small, standalone extension for Abhiverse — **not a rewrite of the core**.
It adds one feature: a natural-language payment instruction becomes a
PayPal **Sandbox** payment, only after an explicit approval step.

Hackathon fit, stated plainly:

- The hackathon allows existing projects *"as long as you make meaningful
  progress on the project during the course of the hackathon"* — this
  extension is that new progress, built in-window.
- The project *"must meaningfully use both PayPal and AI"*: the AI step
  parses the instruction, and the PayPal step moves (sandbox) money.
- Nothing here touches Abhiverse's core security invariants: no signature
  check, pairing approval, revocation, or permission gate was modified.
  The core is untouched; this lives in `extensions/paypal-ai/` only.

## Flow

```
"pay $25 to alice@example.com for the design work"
        │  1. parse (LLM if a key is set, else heuristic fallback)
        ▼
PaymentIntent { amount: "25.00", currency: "USD",
                recipient: "alice@example.com", purpose: "the design work" }
        │  2. APPROVAL — nothing moves without an explicit yes
        ▼  3. PayPal Sandbox: create order → capture order
PaymentResult { status: "COMPLETED", order_id, capture_id, ... }
```

The approval step is the security story, carried over from Abhiverse's
approval-based pairing: the parsed intent is shown for approval **before**
any PayPal call, and a rejection aborts with **zero** PayPal API calls
(covered by test).

## Quickstart (mock mode — no credentials, no network)

```bash
cd extensions/paypal-ai
python3 run_tests.py

python3 -m approve_to_pay.cli --mock --auto-approve \
    "pay $25 to alice@example.com for the design work"
```

Mock output uses clearly fake ids (`MOCK-ORDER-0001`,
`MOCK-CAPTURE-0001`). `--auto-approve` is demos/tests only; without it,
approval is asked on stdin.

## Sandbox mode (real PayPal Sandbox API, still no real money)

1. Create a **free PayPal developer account** and a **Sandbox** app
   (developer.paypal.com). This needs no card and no real money.
2. Export the sandbox app's client id/secret — env vars only, never files:

```bash
export PAYPAL_CLIENT_ID="..."
export PAYPAL_CLIENT_SECRET="..."
python3 -m approve_to_pay.cli --sandbox \
    "pay $25 to alice@example.com for the design work"
```

The client talks only to `https://api-m.sandbox.paypal.com`. The base URL
is hardcoded to the sandbox and the client refuses any non-sandbox URL,
so this code cannot move real money even by misconfiguration.

## Environment variables

| Variable | Required? | Purpose |
|---|---|---|
| `PAYPAL_CLIENT_ID` / `PAYPAL_CLIENT_SECRET` | Sandbox mode only | PayPal Sandbox app credentials |
| `APPROVE_TO_PAY_LLM_API_KEY` | No (LLM path only) | Key for the OpenAI-compatible LLM endpoint |
| `APPROVE_TO_PAY_LLM_BASE_URL` | No | LLM endpoint base (default `https://api.openai.com/v1`) |
| `APPROVE_TO_PAY_LLM_MODEL` | No | LLM model (default `gpt-4o-mini`) |

No secret is ever read from a file, printed, or committed.

## Mock vs sandbox

| | Mock (`--mode mock`) | Sandbox (`--mode sandbox`) |
|---|---|---|
| Network | None | PayPal Sandbox API only |
| Credentials | None | `PAYPAL_CLIENT_ID`/`PAYPAL_CLIENT_SECRET` |
| Money | None, ever | Sandbox test money only — never real |
| Ids | `MOCK-ORDER-*` / `MOCK-CAPTURE-*` | Real sandbox order/capture ids |

## Parser honesty: LLM vs heuristic

- **LLM path (the real AI path):** used when `APPROVE_TO_PAY_LLM_API_KEY`
  is set, or with `--parser llm`. It calls an OpenAI-compatible
  chat-completions endpoint and validates the returned JSON into a
  `PaymentIntent`. If the call fails, parsing fails loudly — it never
  silently downgrades to the heuristic parser.
- **Heuristic fallback (offline testing only):** a small regex parser
  (`intent.parse_heuristic`) for amounts (`$25`, `100 USD`, `500 rupees`),
  email recipients, and `for ...` purposes. It is used when no LLM key
  is set (`--parser auto`, the default) or with `--parser heuristic`.
  Every `PaymentResult` records which parser ran (`parser: "llm"` or
  `"heuristic"`), so a demo can never quietly present the fallback as AI.

## Approval: what is real, what is a stand-in

- Real: the gate itself — no PayPal call happens before approval, and
  rejection means zero calls. This is tested.
- Stand-in: `TerminalApprover` asks on stdin. In production this is
  replaced by a gate that sends the intent JSON to the **paired phone**
  over the Abhiverse protocol and waits for the controller's signed
  approval. That wiring is not in this prototype (see below).

## Honest limits

- **Sandbox only.** No live PayPal, no real money, by construction.
- **LLM key optional.** Without one, parsing is the heuristic fallback —
  useful, but not AI. Say so in any demo.
- **Not yet wired into the Rust agent or the Flutter controller.**
  The approval gate interface (`approval.ApprovalGate`) is the seam for
  that integration; the terminal approver is a placeholder for the
  paired-phone approval.
- **Currencies:** USD, EUR, GBP, INR only. Recipient must be an email
  (PayPal payee email). Amounts need a symbol or currency word — a bare
  number is refused rather than guessed.
- **Prototype status:** this directory is new, uncommitted working-tree
  code. Per the repo's `AGENTS.md`, a human merges it (agents do not
  merge or push to main), and the new third-party network calls
  (PayPal Sandbox, LLM endpoint) need a human-approved reason recorded
  in the merge request — this README is that proposal, not an approval.

## What still needs a human

1. **PayPal developer sandbox keys** (free, no card): create a sandbox
   app at developer.paypal.com, export `PAYPAL_CLIENT_ID` /
   `PAYPAL_CLIENT_SECRET`, run once in `--sandbox` mode to confirm a
   real sandbox capture.
2. **(Optional) LLM key** if the demo should show the real AI path
   instead of the heuristic fallback.
3. **Paired-phone approval wiring**: replace `TerminalApprover` with a
   gate over the Abhiverse pairing protocol (signed approval from the
   already-paired controller).
4. **Real-device demo video** (under 3 minutes, public YouTube) showing
   the flow working — the hackathon requires footage of the project
   functioning, and this sandbox cannot produce device footage.
5. **Merge + Devpost submission**: human review, merge, then submit with
   the repo link, the video, and an honest description (sandbox only,
   heuristic fallback labelled as such).

## Tests

```bash
cd extensions/paypal-ai
python3 run_tests.py   # stdlib unittest, no dependencies
```

## Layout

```
approve_to_pay/
  intent.py    PaymentIntent + validation + heuristic fallback parser
  llm.py       OpenAI-compatible LLM parser (the AI path)
  approval.py  Approval gates (mock / terminal; seam for paired-phone)
  paypal.py    PayPal Sandbox REST client + no-network mock
  pipeline.py  parse -> approve -> create + capture orchestration
  cli.py       CLI demo
tests/         unit tests (intent, llm, paypal, pipeline)
```
