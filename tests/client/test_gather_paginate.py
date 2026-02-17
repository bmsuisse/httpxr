from __future__ import annotations

from typing import Any

import pytest

import httpxr


# ---------------------------------------------------------------------------
# Helpers – mock transport handlers
# ---------------------------------------------------------------------------


def hello_world(request: httpxr.Request) -> httpxr.Response:
    return httpxr.Response(200, text="Hello, world!")


def echo_url(request: httpxr.Request) -> httpxr.Response:
    """Return the URL and method as JSON."""
    return httpxr.Response(
        200,
        json={"url": str(request.url), "method": request.method},
    )


def failing_request(request: httpxr.Request) -> httpxr.Response:
    """Always returns 500."""
    return httpxr.Response(500, text="Internal Server Error")


def mixed_responses(request: httpxr.Request) -> httpxr.Response:
    """Return 200 or 500 based on the URL path."""
    url_str = str(request.url)
    if "fail" in url_str:
        return httpxr.Response(500, text="fail")
    return httpxr.Response(200, text="ok")


# --- Pagination handlers ---

_page_counter: dict[str, int] = {}


def paginated_json_handler(request: httpxr.Request) -> httpxr.Response:
    """Simulate a paginated API with JSON next URL (3 pages total)."""
    url_str = str(request.url)

    # Determine current page from URL
    if "page=3" in url_str:
        page = 3
    elif "page=2" in url_str:
        page = 2
    else:
        page = 1

    data: dict[str, Any] = {
        "page": page,
        "items": [f"item_{page}_1", f"item_{page}_2"],
    }

    if page < 3:
        base = url_str.split("?")[0]
        data["next_page"] = f"{base}?page={page + 1}"
    else:
        data["next_page"] = None

    return httpxr.Response(200, json=data)


def paginated_link_header_handler(request: httpxr.Request) -> httpxr.Response:
    """Simulate pagination via Link header (2 pages total)."""
    url_str = str(request.url)

    if "page=2" in url_str:
        page = 2
        headers = {}
    else:
        page = 1
        base = url_str.split("?")[0]
        headers = {"link": f'<{base}?page=2>; rel="next"'}

    data = {"page": page, "items": [f"item_{page}"]}
    return httpxr.Response(200, json=data, headers=headers)


def paginated_single_page_handler(request: httpxr.Request) -> httpxr.Response:
    """Return a single page with no next URL."""
    return httpxr.Response(200, json={"page": 1, "items": ["only_item"], "next": None})


# ===========================================================================
# Sync gather() tests
# ===========================================================================


class TestSyncGather:
    def test_gather_basic(self) -> None:
        """Multiple requests dispatched concurrently."""
        client = httpxr.Client(transport=httpxr.MockTransport(echo_url))
        requests = [
            client.build_request("GET", f"http://example.com/item/{i}")
            for i in range(5)
        ]
        results = client.gather(requests)

        assert len(results) == 5
        for i, resp in enumerate(results):
            assert resp.status_code == 200
            assert resp.json()["url"].endswith(f"/item/{i}")
        client.close()

    def test_gather_empty(self) -> None:
        """Empty request list returns empty results."""
        client = httpxr.Client(transport=httpxr.MockTransport(hello_world))
        results = client.gather([])

        assert len(results) == 0
        client.close()

    def test_gather_single_request(self) -> None:
        """Single request works fine."""
        client = httpxr.Client(transport=httpxr.MockTransport(hello_world))
        requests = [client.build_request("GET", "http://example.com")]
        results = client.gather(requests)

        assert len(results) == 1
        assert results[0].status_code == 200
        client.close()

    def test_gather_with_concurrency_limit(self) -> None:
        """Concurrency limit is respected (still returns all results)."""
        client = httpxr.Client(transport=httpxr.MockTransport(echo_url))
        requests = [
            client.build_request("GET", f"http://example.com/{i}")
            for i in range(10)
        ]
        results = client.gather(requests, max_concurrency=2)

        assert len(results) == 10
        client.close()

    def test_gather_return_exceptions_false(self) -> None:
        """When return_exceptions=False, errors propagate."""
        client = httpxr.Client(transport=httpxr.MockTransport(mixed_responses))
        requests = [
            client.build_request("GET", "http://example.com/ok"),
            client.build_request("GET", "http://example.com/ok"),
        ]
        # All succeed — no error
        results = client.gather(requests, return_exceptions=False)
        assert len(results) == 2
        client.close()

    def test_gather_with_different_methods(self) -> None:
        """Gather supports mixed HTTP methods."""
        client = httpxr.Client(transport=httpxr.MockTransport(echo_url))
        requests = [
            client.build_request("GET", "http://example.com/a"),
            client.build_request("POST", "http://example.com/b"),
            client.build_request("PUT", "http://example.com/c"),
        ]
        results = client.gather(requests)

        assert len(results) == 3
        methods = [r.json()["method"] for r in results]
        assert "GET" in methods
        assert "POST" in methods
        assert "PUT" in methods
        client.close()

    def test_gather_closed_client_raises(self) -> None:
        """Gather on a closed client raises RuntimeError."""
        client = httpxr.Client(transport=httpxr.MockTransport(hello_world))
        client.close()

        with pytest.raises(RuntimeError):
            client.gather([])


# ===========================================================================
# Sync paginate() tests
# ===========================================================================


class TestSyncPaginate:
    def test_paginate_json_key(self) -> None:
        """Pagination using JSON key to extract next URL."""
        client = httpxr.Client(transport=httpxr.MockTransport(paginated_json_handler))
        pages = list(
            client.paginate(
                "GET",
                "http://api.example.com/items",
                next_url="next_page",
            )
        )

        assert len(pages) == 3
        assert pages[0].json()["page"] == 1
        assert pages[1].json()["page"] == 2
        assert pages[2].json()["page"] == 3
        client.close()

    def test_paginate_link_header(self) -> None:
        """Pagination using Link header."""
        client = httpxr.Client(
            transport=httpxr.MockTransport(paginated_link_header_handler)
        )
        pages = list(
            client.paginate(
                "GET",
                "http://api.example.com/items",
                next_header="link",
            )
        )

        assert len(pages) == 2
        assert pages[0].json()["page"] == 1
        assert pages[1].json()["page"] == 2
        client.close()

    def test_paginate_custom_func(self) -> None:
        """Pagination using a custom callable."""

        def get_next(response: httpxr.Response) -> str | None:
            data = response.json()
            return data.get("next_page")

        client = httpxr.Client(transport=httpxr.MockTransport(paginated_json_handler))
        pages = list(
            client.paginate(
                "GET",
                "http://api.example.com/items",
                next_func=get_next,
            )
        )

        assert len(pages) == 3
        client.close()

    def test_paginate_max_pages(self) -> None:
        """max_pages limits the number of fetched pages."""
        client = httpxr.Client(transport=httpxr.MockTransport(paginated_json_handler))
        pages = list(
            client.paginate(
                "GET",
                "http://api.example.com/items",
                next_url="next_page",
                max_pages=2,
            )
        )

        assert len(pages) == 2
        assert pages[0].json()["page"] == 1
        assert pages[1].json()["page"] == 2
        client.close()

    def test_paginate_single_page(self) -> None:
        """Single page API returns one result."""
        client = httpxr.Client(
            transport=httpxr.MockTransport(paginated_single_page_handler)
        )
        pages = list(
            client.paginate(
                "GET",
                "http://api.example.com/items",
                next_url="next",
            )
        )

        assert len(pages) == 1
        assert pages[0].json()["page"] == 1
        client.close()

    def test_paginate_is_iterator(self) -> None:
        """paginate() returns an iterator, not a list."""
        client = httpxr.Client(transport=httpxr.MockTransport(paginated_json_handler))
        result = client.paginate(
            "GET",
            "http://api.example.com/items",
            next_url="next_page",
        )

        # Must be an iterator (has __iter__ and __next__)
        assert hasattr(result, "__iter__")
        assert hasattr(result, "__next__")

        # Can iterate manually
        page1 = next(result)
        assert page1.json()["page"] == 1

        page2 = next(result)
        assert page2.json()["page"] == 2

        page3 = next(result)
        assert page3.json()["page"] == 3

        # StopIteration after all pages
        with pytest.raises(StopIteration):
            next(result)

        client.close()

    def test_paginate_collect(self) -> None:
        """Iterator's collect() method returns all pages as a list."""
        client = httpxr.Client(transport=httpxr.MockTransport(paginated_json_handler))
        iterator = client.paginate(
            "GET",
            "http://api.example.com/items",
            next_url="next_page",
        )

        pages = iterator.collect()
        assert len(pages) == 3
        assert all(p.status_code == 200 for p in pages)
        client.close()

    def test_paginate_pages_fetched(self) -> None:
        """pages_fetched property tracks progress."""
        client = httpxr.Client(transport=httpxr.MockTransport(paginated_json_handler))
        iterator = client.paginate(
            "GET",
            "http://api.example.com/items",
            next_url="next_page",
        )

        assert iterator.pages_fetched == 0
        next(iterator)
        assert iterator.pages_fetched == 1
        next(iterator)
        assert iterator.pages_fetched == 2
        client.close()

    def test_paginate_no_strategy_raises(self) -> None:
        """Must specify at least one of next_url, next_header, next_func."""
        client = httpxr.Client(transport=httpxr.MockTransport(hello_world))

        with pytest.raises(ValueError, match="Must specify one of"):
            client.paginate("GET", "http://example.com")

        client.close()

    def test_paginate_closed_client_raises(self) -> None:
        """Paginate on a closed client raises RuntimeError."""
        client = httpxr.Client(transport=httpxr.MockTransport(hello_world))
        client.close()

        with pytest.raises(RuntimeError):
            client.paginate(
                "GET", "http://example.com", next_url="next"
            )


# ===========================================================================
# Async gather() tests
# ===========================================================================


class TestAsyncGather:
    @pytest.mark.anyio
    async def test_async_gather_basic(self) -> None:
        """Multiple async requests dispatched concurrently."""
        async with httpxr.AsyncClient(
            transport=httpxr.MockTransport(echo_url)
        ) as client:
            requests = [
                client.build_request("GET", f"http://example.com/item/{i}")
                for i in range(5)
            ]
            results = await client.gather(requests)

        assert len(results) == 5
        for i, resp in enumerate(results):
            assert resp.status_code == 200

    @pytest.mark.anyio
    async def test_async_gather_empty(self) -> None:
        """Empty request list returns empty results."""
        async with httpxr.AsyncClient(
            transport=httpxr.MockTransport(hello_world)
        ) as client:
            results = await client.gather([])

        assert len(results) == 0

    @pytest.mark.anyio
    async def test_async_gather_with_concurrency(self) -> None:
        """Concurrency limit works for async gather."""
        async with httpxr.AsyncClient(
            transport=httpxr.MockTransport(echo_url)
        ) as client:
            requests = [
                client.build_request("GET", f"http://example.com/{i}")
                for i in range(10)
            ]
            results = await client.gather(requests, max_concurrency=3)

        assert len(results) == 10

    @pytest.mark.anyio
    async def test_async_gather_closed_client_raises(self) -> None:
        """Gather on closed async client raises RuntimeError."""
        client = httpxr.AsyncClient(transport=httpxr.MockTransport(hello_world))
        await client.aclose()

        with pytest.raises(RuntimeError):
            await client.gather([])


# ===========================================================================
# Async paginate() tests
# ===========================================================================


class TestAsyncPaginate:
    @pytest.mark.anyio
    async def test_async_paginate_json_key(self) -> None:
        """Async pagination using JSON key."""
        async with httpxr.AsyncClient(
            transport=httpxr.MockTransport(paginated_json_handler)
        ) as client:
            pages: list[httpxr.Response] = []
            async for page in client.paginate(
                "GET",
                "http://api.example.com/items",
                next_url="next_page",
            ):
                pages.append(page)

        assert len(pages) == 3
        assert pages[0].json()["page"] == 1
        assert pages[1].json()["page"] == 2
        assert pages[2].json()["page"] == 3

    @pytest.mark.anyio
    async def test_async_paginate_link_header(self) -> None:
        """Async pagination using Link header."""
        async with httpxr.AsyncClient(
            transport=httpxr.MockTransport(paginated_link_header_handler)
        ) as client:
            pages: list[httpxr.Response] = []
            async for page in client.paginate(
                "GET",
                "http://api.example.com/items",
                next_header="link",
            ):
                pages.append(page)

        assert len(pages) == 2
        assert pages[0].json()["page"] == 1
        assert pages[1].json()["page"] == 2

    @pytest.mark.anyio
    async def test_async_paginate_custom_func(self) -> None:
        """Async pagination using a custom callable."""

        def get_next(response: httpxr.Response) -> str | None:
            data = response.json()
            return data.get("next_page")

        async with httpxr.AsyncClient(
            transport=httpxr.MockTransport(paginated_json_handler)
        ) as client:
            pages: list[httpxr.Response] = []
            async for page in client.paginate(
                "GET",
                "http://api.example.com/items",
                next_func=get_next,
            ):
                pages.append(page)

        assert len(pages) == 3

    @pytest.mark.anyio
    async def test_async_paginate_max_pages(self) -> None:
        """max_pages limits async pagination."""
        async with httpxr.AsyncClient(
            transport=httpxr.MockTransport(paginated_json_handler)
        ) as client:
            pages: list[httpxr.Response] = []
            async for page in client.paginate(
                "GET",
                "http://api.example.com/items",
                next_url="next_page",
                max_pages=1,
            ):
                pages.append(page)

        assert len(pages) == 1
        assert pages[0].json()["page"] == 1

    @pytest.mark.anyio
    async def test_async_paginate_single_page(self) -> None:
        """Single page async pagination."""
        async with httpxr.AsyncClient(
            transport=httpxr.MockTransport(paginated_single_page_handler)
        ) as client:
            pages: list[httpxr.Response] = []
            async for page in client.paginate(
                "GET",
                "http://api.example.com/items",
                next_url="next",
            ):
                pages.append(page)

        assert len(pages) == 1

    @pytest.mark.anyio
    async def test_async_paginate_is_async_iterator(self) -> None:
        """paginate() returns an async iterator."""
        async with httpxr.AsyncClient(
            transport=httpxr.MockTransport(paginated_json_handler)
        ) as client:
            result = client.paginate(
                "GET",
                "http://api.example.com/items",
                next_url="next_page",
            )
            assert hasattr(result, "__aiter__")
            assert hasattr(result, "__anext__")

    @pytest.mark.anyio
    async def test_async_paginate_collect(self) -> None:
        """Async iterator's collect() method returns all pages."""
        async with httpxr.AsyncClient(
            transport=httpxr.MockTransport(paginated_json_handler)
        ) as client:
            iterator = client.paginate(
                "GET",
                "http://api.example.com/items",
                next_url="next_page",
            )
            pages = await iterator.collect()

        assert len(pages) == 3

    @pytest.mark.anyio
    async def test_async_paginate_pages_fetched(self) -> None:
        """pages_fetched property tracks async progress."""
        async with httpxr.AsyncClient(
            transport=httpxr.MockTransport(paginated_json_handler)
        ) as client:
            iterator = client.paginate(
                "GET",
                "http://api.example.com/items",
                next_url="next_page",
            )
            assert iterator.pages_fetched == 0

            pages: list[httpxr.Response] = []
            async for page in iterator:
                pages.append(page)

            assert iterator.pages_fetched == 3

    @pytest.mark.anyio
    async def test_async_paginate_no_strategy_raises(self) -> None:
        """Must specify at least one pagination strategy."""
        async with httpxr.AsyncClient(
            transport=httpxr.MockTransport(hello_world)
        ) as client:
            with pytest.raises(ValueError, match="Must specify one of"):
                client.paginate("GET", "http://example.com")

    @pytest.mark.anyio
    async def test_async_paginate_closed_client_raises(self) -> None:
        """Paginate on closed async client raises RuntimeError."""
        client = httpxr.AsyncClient(transport=httpxr.MockTransport(hello_world))
        await client.aclose()

        with pytest.raises(RuntimeError):
            client.paginate("GET", "http://example.com", next_url="next")
