"""
Authentication
==============

Using httpr's built-in authentication classes: BasicAuth, DigestAuth,
and custom auth via FunctionAuth.
"""

import httpr


def main() -> None:
    # ── Basic Auth ───────────────────────────────────────────────────────
    print("── Basic Auth ─────────────────────────────────────────────────")
    response = httpr.get(
        "https://httpbin.org/basic-auth/user/pass",
        auth=httpr.BasicAuth("user", "pass"),
    )
    print(f"  Status: {response.status_code}")
    print(f"  Authenticated: {response.json().get('authenticated')}")
    print()

    # ── Basic Auth with tuple shorthand ──────────────────────────────────
    print("── Basic Auth (tuple shorthand) ────────────────────────────────")
    response = httpr.get(
        "https://httpbin.org/basic-auth/admin/secret",
        auth=("admin", "secret"),
    )
    print(f"  Status: {response.status_code}")
    print(f"  User: {response.json().get('user')}")
    print()

    # ── Auth on the client (applied to every request) ────────────────────
    print("── Client-level auth ──────────────────────────────────────────")
    with httpr.Client(auth=httpr.BasicAuth("user", "pass")) as client:
        response = client.get("https://httpbin.org/basic-auth/user/pass")
        print(f"  Status: {response.status_code}")
        print(f"  Authenticated: {response.json().get('authenticated')}")
    print()

    # ── Custom auth with FunctionAuth ────────────────────────────────────
    print("── FunctionAuth (custom) ──────────────────────────────────────")

    def add_api_key(request: httpr.Request) -> httpr.Request:
        """Inject an API key header into every request."""
        request.headers["X-API-Key"] = "my-secret-api-key"
        return request

    auth = httpr.FunctionAuth(add_api_key)
    with httpr.Client(auth=auth) as client:
        response = client.get("https://httpbin.org/headers")
        headers = response.json()["headers"]
        print(f"  X-API-Key header: {headers.get('X-Api-Key')}")
    print()

    # ── Using auth programmatically ───────────────────────────────────────
    print("── Programmatic auth check ─────────────────────────────────────")
    # You can inspect auth-related headers on a Request object:
    request = httpr.Request(
        "GET",
        "https://example.com/protected",
        headers={"Authorization": "Bearer my-token-123"},
    )
    print(f"  Auth header: {request.headers['Authorization']}")


if __name__ == "__main__":
    main()
