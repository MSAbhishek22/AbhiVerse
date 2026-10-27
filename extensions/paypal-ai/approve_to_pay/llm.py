"""OpenAI-compatible LLM parsing — the real AI path.

Enabled only when APPROVE_TO_PAY_LLM_API_KEY is set. When it is set and
the call fails, parsing fails loudly: we never silently downgrade to
the heuristic parser, so a demo cannot quietly pretend to be AI.

Any OpenAI-compatible endpoint works: set APPROVE_TO_PAY_LLM_BASE_URL
(default https://api.openai.com/v1) and APPROVE_TO_PAY_LLM_MODEL.
"""

from __future__ import annotations

import json
import os
import urllib.error
import urllib.request

from .intent import IntentParseError, PaymentIntent, validate_intent

SYSTEM_PROMPT = (
    "Parse a natural-language payment instruction into a JSON object with "
    "exactly these keys: amount (string, decimal with two places, e.g. "
    '"25.00"), currency (ISO 4217 code, e.g. USD), recipient (email '
    "address), purpose (short description, may be an empty string). "
    "Return ONLY the JSON object, no other text."
)


class LLMNotConfigured(RuntimeError):
    """Raised when the LLM path is requested but no API key is set."""


class LLMError(RuntimeError):
    """Raised when the LLM call fails or returns an unusable intent."""


def _post_json(url, payload, headers, timeout=30.0):
    req = urllib.request.Request(
        url, data=json.dumps(payload).encode("utf-8"), headers=headers, method="POST"
    )
    try:
        with urllib.request.urlopen(req, timeout=timeout) as resp:
            return json.loads(resp.read().decode("utf-8"))
    except urllib.error.HTTPError as exc:
        body = exc.read().decode("utf-8", "replace")[:500]
        raise LLMError(f"LLM request failed ({exc.code}): {body}") from exc
    except (urllib.error.URLError, TimeoutError, OSError) as exc:
        raise LLMError(f"LLM request failed: {exc}") from exc


def parse_with_llm(
    instruction: str,
    *,
    base_url: str | None = None,
    api_key: str | None = None,
    model: str | None = None,
    request_fn=None,
) -> PaymentIntent:
    """Parse via an OpenAI-compatible chat-completions endpoint.

    ``request_fn`` (for tests) takes (url, payload, headers) and returns
    the parsed JSON response dict.
    """
    base_url = (
        base_url
        or os.environ.get("APPROVE_TO_PAY_LLM_BASE_URL")
        or "https://api.openai.com/v1"
    )
    api_key = api_key or os.environ.get("APPROVE_TO_PAY_LLM_API_KEY", "")
    model = model or os.environ.get("APPROVE_TO_PAY_LLM_MODEL", "gpt-4o-mini")
    if not api_key:
        raise LLMNotConfigured(
            "APPROVE_TO_PAY_LLM_API_KEY is not set; use the heuristic parser "
            "explicitly for offline testing, or set a key for the AI path."
        )
    url = base_url.rstrip("/") + "/chat/completions"
    payload = {
        "model": model,
        "messages": [
            {"role": "system", "content": SYSTEM_PROMPT},
            {"role": "user", "content": instruction},
        ],
        "temperature": 0,
    }
    headers = {
        "Authorization": f"Bearer {api_key}",
        "Content-Type": "application/json",
    }
    data = (
        request_fn(url, payload, headers)
        if request_fn is not None
        else _post_json(url, payload, headers)
    )
    try:
        content = data["choices"][0]["message"]["content"]
    except (KeyError, IndexError, TypeError) as exc:
        raise LLMError(f"unexpected LLM response shape: {data!r}") from exc
    content = content.strip()
    if content.startswith("```"):
        # Strip a fenced block, e.g. ```json {...} ```
        content = content.strip("`").strip()
        if content.lower().startswith("json"):
            content = content[4:].strip()
    try:
        parsed = json.loads(content)
    except json.JSONDecodeError as exc:
        raise LLMError(f"LLM did not return valid JSON: {content!r}") from exc
    try:
        return validate_intent(
            parsed.get("amount"),
            parsed.get("currency"),
            parsed.get("recipient"),
            parsed.get("purpose", ""),
        )
    except IntentParseError as exc:
        raise LLMError(f"LLM returned an invalid intent: {exc}") from exc
