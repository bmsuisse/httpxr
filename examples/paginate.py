"""
Example: httpr.Client.paginate() — automatic pagination

paginate() automatically follows pagination links across multiple API
responses. It supports three strategies for discovering the next page URL:

  1. next_url:    Extract from a JSON key in the response body
  2. next_header: Parse a Link header (e.g. GitHub-style pagination)
  3. next_func:   Use a custom callable to extract the next URL

This is an httpr extension — not available in httpx.
"""

import httpr


def paginate_json_key() -> None:
    """Follow @odata.nextLink in JSON body (common in Microsoft Graph APIs)."""
    with httpr.Client() as client:
        pages = client.paginate(
            "GET",
            "https://graph.microsoft.com/v1.0/users",
            next_url="@odata.nextLink",  # JSON key containing the next page URL
            max_pages=5,
            headers={"Authorization": "Bearer YOUR_TOKEN"},
        )

        print(f"Fetched {len(pages)} pages")
        for i, page in enumerate(pages):
            data = page.json()
            print(f"  Page {i + 1}: {len(data.get('value', []))} items")


def paginate_link_header() -> None:
    """Follow Link header (GitHub API style).

    GitHub returns pagination info like:
        Link: <https://api.github.com/repos/org/repo/issues?page=2>; rel="next",
              <https://api.github.com/repos/org/repo/issues?page=5>; rel="last"
    """
    with httpr.Client() as client:
        pages = client.paginate(
            "GET",
            "https://api.github.com/repos/python/cpython/issues",
            next_header="link",  # HTTP header name to parse for rel="next"
            max_pages=3,
            params={"per_page": "10", "state": "open"},
        )

        total_issues = 0
        for i, page in enumerate(pages):
            issues = page.json()
            total_issues += len(issues)
            print(f"  Page {i + 1}: {len(issues)} issues")

        print(f"Total issues fetched: {total_issues}")


def paginate_custom_function() -> None:
    """Use a custom function to extract the next page URL.

    Useful for APIs with non-standard pagination schemes.
    """

    def get_next_url(response: httpr.Response) -> str | None:
        """Extract next page from a custom response format."""
        data = response.json()
        # Example: API returns { "pagination": { "next": "https://..." } }
        pagination = data.get("pagination", {})
        return pagination.get("next")

    with httpr.Client() as client:
        pages = client.paginate(
            "GET",
            "https://api.example.com/items",
            next_func=get_next_url,
            max_pages=10,
        )

        print(f"Fetched {len(pages)} pages via custom extractor")


def paginate_with_max_pages() -> None:
    """Limit the number of pages fetched to avoid runaway loops."""
    with httpr.Client() as client:
        pages = client.paginate(
            "GET",
            "https://api.github.com/repos/python/cpython/commits",
            next_header="link",
            max_pages=2,  # Stop after 2 pages even if more are available
        )

        print(f"Fetched exactly {len(pages)} pages (max_pages=2)")


if __name__ == "__main__":
    print("=== Paginate with Link Header (GitHub) ===")
    paginate_link_header()

    print("\n=== Paginate with max_pages ===")
    paginate_with_max_pages()

    # These require valid API credentials:
    # print("\n=== Paginate with JSON Key (OData) ===")
    # paginate_json_key()
    #
    # print("\n=== Paginate with Custom Function ===")
    # paginate_custom_function()
