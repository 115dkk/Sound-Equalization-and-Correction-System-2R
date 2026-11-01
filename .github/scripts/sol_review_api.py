"""One bounded Responses API request. No tools, agent loop, retries or PR execution."""
import argparse
import json
import os
from pathlib import Path
import urllib.request
import urllib.error

MAX_INPUT_BYTES = 300_000
MAX_RESPONSE_BYTES = 2_000_000
MAX_OUTPUT_TOKENS = 32_000


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


def build_request(prompt, policy, schema, model, effort, output_tokens):
    if not prompt or len((prompt + policy).encode()) > MAX_INPUT_BYTES:
        raise ValueError("Input missing or exceeds byte budget")
    if model != "gpt-6.1-sol":
        raise ValueError("Model change requires reviewing capabilities and pricing")
    if effort not in ("low", "medium", "high"):
        raise ValueError("Unsupported reasoning effort")
    return {
        "model": model, "instructions": policy,
        "input": prompt, "reasoning": {"effort": effort},
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


def run(directory, policy, schema, transport=None):
    # An old output must never survive a failed retry.
    output = directory / "result.json"
    output.unlink(missing_ok=True)
    body = build_request(
        (directory / "prompt.txt").read_text(), policy, schema,
        os.environ.get("REVIEW_MODEL", "gpt-6.1-sol"),
        os.environ.get("REVIEW_EFFORT", "high"),
        os.environ.get("REVIEW_MAX_OUTPUT_TOKENS", "16000"),
    )
    timeout = bounded_int(os.environ.get("REVIEW_TIMEOUT_SECONDS", "600"), 30, 900)
    if transport is None:
        request = urllib.request.Request(
            "https://api.openai.com/v1/responses", data=json.dumps(body).encode(),
            headers={"Authorization": "Bearer " + os.environ["OPENAI_API_KEY"],
                     "Content-Type": "application/json"},
        )
        # One attempt only. HTTP error bodies may contain sensitive input; never print them.
        with urllib.request.urlopen(request, timeout=timeout) as response:
            raw = response.read(MAX_RESPONSE_BYTES + 1)
        if len(raw) > MAX_RESPONSE_BYTES:
            raise ValueError("API response exceeds limit")
        result = strict_json(raw)
    else:
        result = transport(body, timeout)
    value = extract_result(result)
    output.write_text(json.dumps(value, ensure_ascii=False))


def main():
    parser = argparse.ArgumentParser()
    parser.add_argument("--directory", type=Path, required=True)
    parser.add_argument("--policy", type=Path, required=True)
    parser.add_argument("--schema", type=Path, required=True)
    args = parser.parse_args()
    try:
        run(args.directory, args.policy.read_text(), json.loads(args.schema.read_text()))
    except Exception as error:
        args.directory.mkdir(parents=True, exist_ok=True)
        detail = f"HTTP {error.code}" if isinstance(error, urllib.error.HTTPError) else f"{type(error).__name__}: {str(error)[:300]}" if isinstance(error, ValueError) else type(error).__name__
        (args.directory / "failure.json").write_text(json.dumps({"stage": "api", "reason": detail}))
        # No response body, model output or key is logged.
        raise SystemExit(f"Review API failed ({type(error).__name__}); publisher will submit a COMMENT notice") from None


if __name__ == "__main__":
    main()
