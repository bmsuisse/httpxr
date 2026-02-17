"""
Example: httpxr.Client.paginate() — automatic pagination

paginate() returns a lazy **iterator** that auto-follows pagination links.
Each call to ``next()`` fetches exactly one page, keeping memory usage low
even for APIs with thousands of pages.

Three strategies for discovering the next page URL:

  1. next_url:    Extract from a JSON key in the response body
  2. next_header: Parse a Link header (e.g. GitHub-style pagination)
  3. next_func:   Use a custom callable to extract the next URL

This is an httpxr extension — not available in httpx.
"""

import httpxr


def paginate_json_key() -> None:
    """Follow @odata.nextLink in JSON body (common in Microsoft Graph APIs)."""
    with httpxr.Client() as client:
        # Returns an iterator — pages are fetched lazily
        for page in client.paginate(
            "GET",
            "https://graph.microsoft.com/v1.0/users",
            next_url="@odata.nextLink",  # JSON key containing the next URL
            max_pages=5,
            headers={"Authorization": "Bearer YOUR_TOKEN"},
        ):
            data = page.json()
            print(f"  Page: {len(data.get('value', []))} items")


def paginate_link_header() -> None:
    """Follow Link header (GitHub API style).

    GitHub returns pagination info like:
        Link: <https://api.github.com/repos/org/repo/issues?page=2>; rel="next",
              <https://api.github.com/repos/org/repo/issues?page=5>; rel="last"
    """
    with httpxr.Client() as client:
        total_issues = 0
        for page in client.paginate(
            "GET",
            "https://api.github.com/repos/python/cpython/issues",
            next_header="link",  # HTTP header name to parse for rel="next"
            max_pages=3,
            params={"per_page": "10", "state": "open"},
        ):
            issues = page.json()
            total_issues += len(issues)
            print(f"  Page: {len(issues)} issues")

        print(f"Total issues fetched: {total_issues}")


def paginate_custom_function() -> None:
    """Use a custom function to extract the next page URL.

    Useful for APIs with non-standard pagination schemes.
    """

    def get_next_url(response: httpxr.Response) -> str | None:
        """Extract next page from a custom response format."""
        data = response.json()
        # Example: API returns { "pagination": { "next": "https://..." } }
        pagination = data.get("pagination", {})
        return pagination.get("next")

    with httpxr.Client() as client:
        for page in client.paginate(
            "GET",
            "https://api.example.com/items",
            next_func=get_next_url,
            max_pages=10,
        ):
            print(f"  Got page: {page.status_code}")


def paginate_collect() -> None:
    """Use collect() to gather all pages into a list at once."""
    with httpxr.Client() as client:
        pages = client.paginate(
            "GET",
            "https://api.github.com/repos/python/cpython/issues",
            next_header="link",
            max_pages=2,
        ).collect()

        print(f"Collected {len(pages)} pages")


def paginate_with_progress() -> None:
    """Track pagination progress with pages_fetched."""
    with httpxr.Client() as client:
        paginator = client.paginate(
            "GET",
            "https://api.github.com/repos/python/cpython/commits",
            next_header="link",
            max_pages=3,
        )

        for page in paginator:
            print(f"  Page {paginator.pages_fetched}: {page.status_code}")

        print(f"Total pages fetched: {paginator.pages_fetched}")


if __name__ == "__main__":
    print("=== Paginate with Link Header (GitHub) ===")
    paginate_link_header()

    print("\n=== Paginate with collect() ===")
    paginate_collect()

    print("\n=== Paginate with progress tracking ===")
    paginate_with_progress()

    # These require valid API credentials:
    # print("\n=== Paginate with JSON Key (OData) ===")
    # paginate_json_key()
    #
    # print("\n=== Paginate with Custom Function ===")
    # paginate_custom_function()
