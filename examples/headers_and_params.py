"""
Headers, Query Parameters, and Cookies
=======================================

Working with httpxr's rich model objects: Headers, QueryParams, Cookies, and URL.
"""

import httpxr


def main() -> None:
    # ── Custom headers ───────────────────────────────────────────────────
    response = httpxr.get(
        "https://httpbin.org/headers",
        headers={
            "X-Token": "secret-123",
            "Accept-Language": "en-US",
        },
    )
    echoed = response.json()["headers"]
    print("Custom headers echoed back:")
    for key in ("X-Token", "Accept-Language"):
        print(f"  {key}: {echoed.get(key)}")
    print()

    # ── Query parameters ─────────────────────────────────────────────────
    response = httpxr.get(
        "https://httpbin.org/get",
        params={"search": "httpxr", "page": "1", "sort": "stars"},
    )
    print(f"URL with params: {response.url}")
    print(f"  args echoed: {response.json()['args']}")
    print()

    # ── Working with the Headers object ──────────────────────────────────
    headers = httpxr.Headers({"Content-Type": "text/html", "X-Custom": "value"})
    print(f"Headers object: {dict(headers)}")
    print()

    # ── Working with QueryParams ─────────────────────────────────────────
    params = httpxr.QueryParams({"key": "value", "list": ["a", "b"]})
    print(f"QueryParams: {params}")
    print()

    # ── Working with URL ─────────────────────────────────────────────────
    url = httpxr.URL("https://www.example.com/path?query=1#frag")
    print(f"URL object:  {url}")
    print(f"  scheme:    {url.scheme}")
    print(f"  host:      {url.host}")
    print(f"  path:      {url.path}")
    print(f"  query:     {url.query}")
    print(f"  fragment:  {url.fragment}")
    print()

    # ── Cookies ──────────────────────────────────────────────────────────
    response = httpxr.get(
        "https://httpbin.org/cookies/set/sessionid/abc123",
        follow_redirects=True,
    )
    print(f"Cookies in response: {dict(response.cookies)}")

    # ── Sending cookies ──────────────────────────────────────────────────
    cookies = httpxr.Cookies({"session": "xyz789"})
    response = httpxr.get("https://httpbin.org/cookies", cookies=cookies)
    print(f"Cookies echoed: {response.json()['cookies']}")


if __name__ == "__main__":
    main()
