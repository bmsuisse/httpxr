"""
Example: response.json_bytes() and response.iter_json()

json_bytes() returns the raw response body as bytes, bypassing Python's
UTF-8 string decode. Perfect for feeding into fast JSON parsers like
orjson or msgspec.

iter_json() parses NDJSON (newline-delimited JSON) and SSE (Server-Sent
Events) responses. It automatically handles blank lines, SSE prefixes
(data:, event:, id:, retry:), and [DONE] sentinels.

These are httpxr extensions — not available in httpx.
"""

import json

import httpxr


# ---------------------------------------------------------------------------
# json_bytes() — raw JSON bytes for fast parsing
# ---------------------------------------------------------------------------


def json_bytes_basic() -> None:
    """Use json_bytes() with the standard json module."""
    with httpxr.Client() as client:
        response = client.get("https://httpbin.org/json")

        # Standard: response.json()  (bytes → str → parse)
        standard = response.json()

        # Fast path: response.json_bytes()  (bytes → parse)
        raw = response.json_bytes()
        fast = json.loads(raw)

        assert standard == fast
        print(f"json_bytes() returned {len(raw):,} bytes")
        print(f"Parsed: {list(fast.keys())}")


def json_bytes_with_orjson() -> None:
    """Use json_bytes() with orjson for maximum performance.

    orjson accepts bytes directly, so json_bytes() avoids the
    UTF-8 decode overhead entirely.
    """
    try:
        import orjson
    except ImportError:
        print("orjson not installed — skipping")
        return

    with httpxr.Client() as client:
        response = client.get("https://httpbin.org/json")
        raw = response.json_bytes()
        data = orjson.loads(raw)  # bytes → dict directly
        print(f"orjson parsed: {list(data.keys())}")


# ---------------------------------------------------------------------------
# iter_json() — NDJSON and SSE parsing
# ---------------------------------------------------------------------------


def iter_json_ndjson() -> None:
    """Parse NDJSON (one JSON object per line).

    Many APIs return newline-delimited JSON for streaming data:
        {"id": 1, "event": "click"}
        {"id": 2, "event": "scroll"}
        {"id": 3, "event": "submit"}
    """
    # Simulate an NDJSON response
    ndjson_content = "\n".join(
        json.dumps({"id": i, "event": f"event_{i}"}) for i in range(5)
    )
    response = httpxr.Response(200, content=ndjson_content.encode())

    print("NDJSON items:")
    for item in response.iter_json():
        print(f"  {item}")


def iter_json_sse() -> None:
    """Parse Server-Sent Events (SSE).

    SSE responses look like:
        event: message
        data: {"id": 1}

        event: message
        data: {"id": 2}

        data: [DONE]

    iter_json() extracts the JSON from "data:" lines and skips [DONE].
    """
    sse_content = (
        "event: message\n"
        'data: {"id": 1, "content": "Hello"}\n'
        "\n"
        "event: message\n"
        'data: {"id": 2, "content": "World"}\n'
        "\n"
        "data: [DONE]\n"
    )
    response = httpxr.Response(200, content=sse_content.encode())

    print("SSE events:")
    for event in response.iter_json():
        print(f"  {event}")


def iter_json_openai_style() -> None:
    """Simulate parsing OpenAI-style streaming responses.

    OpenAI's chat completion streaming uses SSE with JSON payloads:
        data: {"choices": [{"delta": {"content": "Hi"}}]}
        data: {"choices": [{"delta": {"content": "!"}}]}
        data: [DONE]
    """
    chunks = [
        {"choices": [{"delta": {"role": "assistant"}, "index": 0}]},
        {"choices": [{"delta": {"content": "Hello"}, "index": 0}]},
        {"choices": [{"delta": {"content": ", "}, "index": 0}]},
        {"choices": [{"delta": {"content": "world!"}, "index": 0}]},
    ]
    sse_lines = [f"data: {json.dumps(c)}" for c in chunks]
    sse_lines.append("data: [DONE]")
    sse_content = "\n\n".join(sse_lines)

    response = httpxr.Response(200, content=sse_content.encode())

    print("OpenAI stream: ", end="")
    for chunk in response.iter_json():
        delta = chunk["choices"][0]["delta"]
        print(delta.get("content", ""), end="", flush=True)
    print()


if __name__ == "__main__":
    print("=== json_bytes() — Basic ===")
    json_bytes_basic()

    print("\n=== json_bytes() — orjson ===")
    json_bytes_with_orjson()

    print("\n=== iter_json() — NDJSON ===")
    iter_json_ndjson()

    print("\n=== iter_json() — SSE ===")
    iter_json_sse()

    print("\n=== iter_json() — OpenAI-style ===")
    iter_json_openai_style()
