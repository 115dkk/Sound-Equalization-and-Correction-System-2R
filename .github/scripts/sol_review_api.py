"""Make one bounded Responses API request without tools or an agent loop."""
import argparse
import json
import os
from pathlib import Path
import time
import urllib.error
import urllib.request

# The Responses API takes an input string of at most 10,485,760 characters.
MAX_INPUT_BYTES = 10_000_000
# gpt-6.1-sol's prompt ceiling. The token count OpenAI reports for the request
# decides what is sent, not its size in bytes (maintainer request 2026-11-02).
MAX_INPUT_TOKENS = 922_000
MAX_RESPONSE_BYTES = 2_000_000
MAX_OUTPUT_TOKENS = 128_000
# Prompts sol_review.py prepares, tried in this order.
PROMPTS = (("full", "prompt.txt"), ("diff", "prompt-diff.txt"))
# Request fields the input token count endpoint accepts; it takes no output
# limit or storage flag.
COUNTED_FIELDS = ("model", "instructions", "input", "reasoning", "text")
# Error codes OpenAI sends when the account has no usable credit or quota left.
QUOTA_CODES = {"insufficient_quota", "billing_hard_limit_reached", "billing_not_active"}
# Answers worth waiting out; only the side-effect-free token count is retried.
RETRY_CODES = {429, 500, 502, 503, 504}


def strict_json(raw):
    def pairs(items):
        result = {}
        for key, value in items:
            if key in result:
                raise ValueError("Duplicate JSON key")
            result[key] = value
        return result

    def invalid_constant(value):
        raise ValueError("Non-finite JSON constant")

    return json.loads(raw, object_pairs_hook=pairs, parse_constant=invalid_constant)


def bounded_int(value, minimum, maximum):
    result = int(value)
    if not minimum <= result <= maximum:
        raise ValueError("Configuration outside allowed bounds")
    return result


def instructions(policy, guide, rules):
    """The trusted instructions: the policy, the review guide and the
    repository rules, all from main's checkout. Nothing of the pull request
    goes here; it travels in the input as data."""
    text = policy + "\n\nREVIEW GUIDE\n\n" + guide + "\n\nTRUSTED REPOSITORY RULES (from main)\n"
    for name, content in rules:
        text += "\n=== " + name + " ===\n" + content
    return text


def build_request(prompt, trusted_instructions, schema, model, effort, output_tokens):
    if not prompt or len((prompt + trusted_instructions).encode()) > MAX_INPUT_BYTES:
        raise ValueError("Input missing or exceeds byte budget")
    if model != "gpt-6.1-sol":
        raise ValueError("Model change requires reviewing capabilities and pricing")
    if effort not in ("low", "medium", "high", "xhigh"):
        raise ValueError("Unsupported reasoning effort")
    return {
        "model": model,
        "instructions": trusted_instructions,
        "input": prompt,
        "reasoning": {"effort": effort},
        "max_output_tokens": bounded_int(output_tokens, 1024, MAX_OUTPUT_TOKENS),
        "store": False,
        "text": {"format": {"type": "json_schema", "name": "secs_pr_review",
                            "strict": True, "schema": schema}},
        # No tools or previous_response_id: untrusted data cannot start an agent loop.
    }


def extract_result(response):
    if response.get("status") != "completed" or response.get("error") or response.get("incomplete_details"):
        raise ValueError("Response failed or incomplete")
    messages = []
    for item in response.get("output", []):
        if item.get("type") == "reasoning":
            continue
        if item.get("type") != "message" or item.get("status") != "completed" or item.get("role") != "assistant":
            raise ValueError("Unexpected response output")
        for content in item.get("content", []):
            if content.get("type") != "output_text":
                raise ValueError("Refusal or unexpected output content")
            messages.append(content["text"])
    if len(messages) != 1 or len(messages[0].encode()) > 60_000:
        raise ValueError("Missing, ambiguous or oversized result")
    value = strict_json(messages[0])
    if not isinstance(value, dict):
        raise ValueError("Expected result object")
    return value


def _header(headers, name):
    return headers.get(name) if headers is not None else None


def _delay(error, attempt):
    reset = _header(error.headers, "x-ratelimit-reset")
    if reset is not None:
        try:
            remaining = float(reset) - time.time()
        except (TypeError, ValueError):
            remaining = 0
        if remaining > 120:
            raise error
    retry_after = _header(error.headers, "Retry-After")
    if retry_after is not None:
        try:
            return min(60.0, max(0.0, float(retry_after)))
        except (TypeError, ValueError):
            pass
    return float(2 ** attempt)


def post(path, body, timeout, retry=False, sleeper=time.sleep):
    # Only a call without side effects may pass retry=True: a lost response to
    # the review call may still have been billed. HTTP error bodies may contain
    # sensitive input; never print them.
    request = urllib.request.Request(
        "https://api.openai.com/v1/" + path,
        data=json.dumps(body).encode(),
        headers={"Authorization": "Bearer " + os.environ["OPENAI_API_KEY"],
                 "Content-Type": "application/json"},
    )
    for attempt in range(4):
        try:
            with urllib.request.urlopen(request, timeout=timeout) as response:
                raw = response.read(MAX_RESPONSE_BYTES + 1)
            if len(raw) > MAX_RESPONSE_BYTES:
                raise ValueError("API response exceeds limit")
            return strict_json(raw)
        except urllib.error.HTTPError as error:
            rate_limited = error.code == 403 and _header(error.headers, "x-ratelimit-remaining") == "0"
            if not retry or attempt == 3 or (error.code not in RETRY_CODES and not rate_limited):
                raise
            sleeper(_delay(error, attempt))
        except (urllib.error.URLError, TimeoutError):
            if not retry or attempt == 3:
                raise
            sleeper(float(2 ** attempt))
    raise AssertionError("unreachable")


def input_tokens(body, counter=None):
    """Return the side-effect-free count OpenAI reports for the request input."""
    counted = {key: body[key] for key in COUNTED_FIELDS}
    value = counter(counted) if counter else post("responses/input_tokens", counted, 120, retry=True)
    if not isinstance(value, dict) or value.get("object") != "response.input_tokens":
        raise ValueError("Invalid input token count")
    tokens = value.get("input_tokens")
    if type(tokens) is not int or tokens <= 0:
        raise ValueError("Invalid input token count")
    return tokens


def choose(directory, trusted_instructions, schema, counter=None):
    """Choose the full prompt when it fits, otherwise the prepared diff prompt."""
    for index, (mode, name) in enumerate(PROMPTS):
        path = directory / name
        if not path.is_file():
            continue
        prompt = path.read_text(encoding="utf-8")
        # prepare could bound only the prompt; the request also carries the
        # instructions, so a full prompt just under the ceiling still falls to diff.
        if index < len(PROMPTS) - 1 and len((prompt + trusted_instructions).encode()) > MAX_INPUT_BYTES:
            continue
        body = build_request(
            prompt, trusted_instructions, schema,
            os.environ.get("REVIEW_MODEL", "gpt-6.1-sol"),
            os.environ.get("REVIEW_EFFORT", "xhigh"),
            os.environ.get("REVIEW_MAX_OUTPUT_TOKENS", "64000"),
        )
        try:
            tokens = input_tokens(body, counter)
        except urllib.error.HTTPError as error:
            # With truncation off (the default) the endpoint refuses an input
            # past the model's context window with 400 instead of counting it.
            # The last prompt keeps the error, so a malformed request still shows.
            if error.code != 400 or index == len(PROMPTS) - 1:
                raise
            continue
        if tokens <= MAX_INPUT_TOKENS:
            return mode, body, tokens
    raise ValueError("PR exceeds input token budget")


def run(directory, trusted_instructions, schema, transport=None, counter=None):
    # An old output must never survive a failed retry.
    output = directory / "result.json"
    output.unlink(missing_ok=True)
    timeout = bounded_int(os.environ.get("REVIEW_TIMEOUT_SECONDS", "1800"), 30, 1800)
    started = time.monotonic()
    mode, body, tokens = choose(directory, trusted_instructions, schema, counter)
    # The workflow step's limit covers the token counts and the model call
    # together, so the counts come out of the model's wait.
    timeout = max(30, timeout - int(time.monotonic() - started))
    # The publisher says in the review body when only the diff was sent.
    metadata = directory / "metadata.json"
    meta = strict_json(metadata.read_text(encoding="utf-8"))
    meta["context_mode"], meta["input_tokens"] = mode, tokens
    metadata.write_text(json.dumps(meta), encoding="utf-8")
    value = transport(body, timeout) if transport else post("responses", body, timeout, retry=False)
    output.write_text(json.dumps(extract_result(value), ensure_ascii=False), encoding="utf-8")


def quota_code(error):
    """The quota code of a refused API call, or None. Only the code is read from
    the error body; the body itself is never logged or stored."""
    if not isinstance(error, urllib.error.HTTPError) or error.fp is None:
        return None
    try:
        value = strict_json(error.read(4096))
    except (ValueError, OSError):
        return None
    detail = value.get("error") if isinstance(value, dict) else None
    if not isinstance(detail, dict):
        return None
    code = detail.get("code") or detail.get("type")
    return code if code in QUOTA_CODES else None


def failure_record(error):
    """What failure.json says about an API failure: exhausted quota on its own,
    every other failure as the HTTP status or error type."""
    code = quota_code(error)
    if code is not None:
        return {"stage": "quota", "reason": code}
    detail = (f"HTTP {error.code}" if isinstance(error, urllib.error.HTTPError)
              else f"{type(error).__name__}: {str(error)[:300]}" if isinstance(error, ValueError)
              else type(error).__name__)
    return {"stage": "api", "reason": detail}


def main():
    parser = argparse.ArgumentParser()
    parser.add_argument("--directory", type=Path, required=True)
    parser.add_argument("--policy", type=Path, required=True)
    parser.add_argument("--guide", type=Path, required=True)
    parser.add_argument("--rules", type=Path, action="append", required=True)
    parser.add_argument("--schema", type=Path, required=True)
    args = parser.parse_args()
    try:
        trusted = instructions(
            args.policy.read_text(encoding="utf-8"), args.guide.read_text(encoding="utf-8"),
            [(path.name, path.read_text(encoding="utf-8")) for path in args.rules],
        )
        run(args.directory, trusted, json.loads(args.schema.read_text(encoding="utf-8")))
    except Exception as error:
        args.directory.mkdir(parents=True, exist_ok=True)
        (args.directory / "failure.json").write_text(
            json.dumps(failure_record(error)), encoding="utf-8")
        # No response body, model output or key is logged.
        raise SystemExit(
            f"Review API failed ({type(error).__name__}); publisher will submit a COMMENT notice"
        ) from None


if __name__ == "__main__":
    main()
