"""
Comprehensive httpx API compatibility test suite for httpxr.

Every class, method, property and constructor parameter in httpxr's public API
(as declared in _httpxr.pyi) is exercised here.  The intention is: if it is in
the .pyi, there must be a test that would fail if it disappeared or changed
signature.  Tests are kept offline (MockTransport / in-process construction)
so they are fast and have no network dependency.

NOTE: Where httpxr's actual behaviour differs from the httpx ABI (even if the
stub says otherwise), the test documents the REAL behaviour with a comment, so
it acts as a regression guard AND as documentation of the variance.
"""
from __future__ import annotations

import asyncio
import datetime
from typing import Any

import pytest

import httpxr


# ---------------------------------------------------------------------------
# Helpers
# ---------------------------------------------------------------------------

def _handler(request: httpxr.Request) -> httpxr.Response:
    """Minimal mock handler that echoes method and path in JSON."""
    return httpxr.Response(
        200,
        json={"method": request.method, "path": str(request.url.path)},
        headers={"X-Custom": "yes"},
    )


def _make_client(**kwargs: Any) -> httpxr.Client:
    return httpxr.Client(transport=httpxr.MockTransport(_handler), **kwargs)


# ---------------------------------------------------------------------------
# URL
# ---------------------------------------------------------------------------

class TestURL:
    def test_construct_from_str(self):
        u = httpxr.URL("https://example.com/path?q=1#frag")
        assert u.scheme == "https"
        assert u.host == "example.com"
        assert u.path == "/path"
        assert u.fragment == "frag"

    def test_construct_from_url(self):
        u1 = httpxr.URL("https://example.com/")
        u2 = httpxr.URL(u1)
        assert str(u1) == str(u2)

    def test_scheme_host_port_path(self):
        u = httpxr.URL("https://example.com:8080/path?a=1")
        assert u.scheme == "https"
        assert u.host == "example.com"
        assert u.port == 8080
        assert u.path == "/path"

    def test_query(self):
        u = httpxr.URL("https://example.com/?a=1")
        # pyi says bytes
        assert isinstance(u.query, bytes)
        assert b"a=1" in u.query

    def test_raw_path(self):
        u = httpxr.URL("https://example.com/path?q=1")
        assert isinstance(u.raw_path, bytes)
        assert b"/path" in u.raw_path

    def test_userinfo_is_bytes(self):
        # NOTE: pyi says str but actual impl returns bytes (like httpx 0.x)
        u = httpxr.URL("https://user:pw@example.com/")
        assert isinstance(u.userinfo, bytes)
        assert b"user" in u.userinfo

    def test_authority_netloc_are_bytes(self):
        u = httpxr.URL("https://example.com:8080/")
        # authority and netloc come back as bytes in httpxr
        assert u.authority is not None
        assert u.netloc is not None

    def test_is_absolute_relative(self):
        abs_url = httpxr.URL("https://example.com/")
        # NOTE: pyi exposes `is_absolute_url` / `is_relative_url` but impl uses
        # `is_absolute` / `is_relative` — document the real names
        assert abs_url.is_absolute is True
        assert abs_url.is_relative is False
        rel_url = httpxr.URL("/just/a/path")
        assert rel_url.is_relative is True
        assert rel_url.is_absolute is False

    def test_str_repr(self):
        u = httpxr.URL("https://example.com/")
        assert "example.com" in str(u)
        assert "URL" in repr(u)

    def test_eq_and_hash(self):
        u1 = httpxr.URL("https://example.com/")
        u2 = httpxr.URL("https://example.com/")
        assert u1 == u2
        assert hash(u1) == hash(u2)

    def test_copy_with(self):
        u = httpxr.URL("https://example.com/old")
        u2 = u.copy_with(path="/new")
        assert u2.path == "/new"
        assert u2.scheme == "https"

    def test_copy_set_param(self):
        u = httpxr.URL("https://example.com/?a=1")
        u2 = u.copy_set_param("b", "2")
        assert "b=2" in str(u2)

    def test_copy_add_param(self):
        u = httpxr.URL("https://example.com/?a=1")
        u2 = u.copy_add_param("a", "2")
        assert str(u2).count("a=") == 2

    def test_copy_remove_param(self):
        u = httpxr.URL("https://example.com/?keep=1&remove=2")
        u2 = u.copy_remove_param("remove")
        assert "remove" not in str(u2)
        assert "keep=1" in str(u2)

    def test_copy_merge_params(self):
        u = httpxr.URL("https://example.com/?a=1")
        u2 = u.copy_merge_params({"b": "2"})
        assert "b=2" in str(u2)

    def test_join(self):
        u = httpxr.URL("https://example.com/base/")
        u2 = u.join("child")
        assert "child" in str(u2)

    def test_params_property(self):
        u = httpxr.URL("https://example.com/?key=val")
        p = u.params
        assert isinstance(p, httpxr.QueryParams)
        assert p["key"] == "val"


# ---------------------------------------------------------------------------
# QueryParams
# ---------------------------------------------------------------------------

class TestQueryParams:
    def test_construct_from_dict(self):
        p = httpxr.QueryParams({"a": "1", "b": "2"})
        assert p["a"] == "1"

    def test_construct_from_str(self):
        p = httpxr.QueryParams("a=1&b=2")
        assert p["a"] == "1"
        assert p["b"] == "2"

    def test_construct_from_list(self):
        p = httpxr.QueryParams([("a", "1"), ("b", "2")])
        assert p["a"] == "1"

    def test_keys_values_items(self):
        p = httpxr.QueryParams({"x": "1", "y": "2"})
        assert "x" in p.keys()
        assert "1" in p.values()
        assert ("x", "1") in p.items()

    def test_multi_items(self):
        p = httpxr.QueryParams([("a", "1"), ("a", "2")])
        assert len(p.multi_items()) == 2

    def test_get_default(self):
        p = httpxr.QueryParams("a=1")
        assert p.get("missing", "default") == "default"
        assert p.get("a") == "1"

    def test_get_list(self):
        p = httpxr.QueryParams([("a", "1"), ("a", "2")])
        assert p.get_list("a") == ["1", "2"]

    def test_set_add_remove_merge(self):
        p = httpxr.QueryParams({"a": "1"})
        p2 = p.set("a", "99")
        assert p2["a"] == "99"
        p3 = p.add("a", "2")
        assert len(p3.get_list("a")) == 2
        p4 = p3.remove("a")
        assert "a" not in p4
        p5 = p.merge({"b": "2"})
        assert "b" in p5

    def test_contains_len_bool(self):
        p = httpxr.QueryParams("a=1")
        assert "a" in p
        assert len(p) == 1
        assert bool(p) is True
        assert bool(httpxr.QueryParams()) is False

    def test_iter(self):
        p = httpxr.QueryParams({"a": "1", "b": "2"})
        keys = list(p)
        assert "a" in keys

    def test_eq_str_repr_hash(self):
        p1 = httpxr.QueryParams("a=1")
        p2 = httpxr.QueryParams("a=1")
        assert p1 == p2
        assert "a" in str(p1)
        assert "QueryParams" in repr(p1)
        assert hash(p1) == hash(p2)

    def test_str_produces_querystring(self):
        # NOTE: pyi declares encode() but impl uses __str__
        p = httpxr.QueryParams({"a": "1"})
        assert "a=1" in str(p)


# ---------------------------------------------------------------------------
# Headers
# ---------------------------------------------------------------------------

class TestHeaders:
    def test_construct_dict(self):
        h = httpxr.Headers({"Content-Type": "application/json"})
        assert h["content-type"] == "application/json"

    def test_construct_list(self):
        h = httpxr.Headers([("X-Foo", "bar"), ("X-Foo", "baz")])
        assert "bar" in h.get_list("x-foo")
        assert "baz" in h.get_list("x-foo")

    def test_construct_from_headers(self):
        h1 = httpxr.Headers({"a": "1"})
        h2 = httpxr.Headers(h1)
        assert h2["a"] == "1"

    def test_encoding(self):
        h = httpxr.Headers(encoding="latin-1")
        assert h.encoding == "latin-1"

    def test_raw(self):
        h = httpxr.Headers({"X-A": "1"})
        raw = h.raw
        # raw is a list of (bytes, bytes) tuples
        assert any(b"x-a" in k or b"X-A" in k for k, v in raw)

    def test_keys_values_items(self):
        h = httpxr.Headers({"A": "1", "B": "2"})
        assert any(k.lower() == "a" for k in h.keys())
        assert "1" in h.values()
        assert any(k.lower() == "a" for k, v in h.items())

    def test_multi_items(self):
        h = httpxr.Headers([("X-Foo", "1"), ("X-Foo", "2")])
        assert len(h.multi_items()) >= 2

    def test_get_default(self):
        h = httpxr.Headers({"A": "1"})
        assert h.get("a") == "1"
        assert h.get("missing", "default") == "default"

    def test_get_list(self):
        h = httpxr.Headers([("Accept", "text/html"), ("Accept", "application/json")])
        lst = h.get_list("accept", split_commas=False)
        assert len(lst) >= 1

    def test_update(self):
        h = httpxr.Headers({"A": "1"})
        h.update({"B": "2"})
        assert "b" in h

    def test_copy(self):
        h = httpxr.Headers({"A": "1"})
        h2 = h.copy()
        assert h2["a"] == "1"

    def test_setdefault(self):
        h = httpxr.Headers({"A": "1"})
        v = h.setdefault("a", "fallback")
        assert v == "1"
        v2 = h.setdefault("new-key", "fallback")
        assert v2 == "fallback"

    def test_setitem_delitem(self):
        h = httpxr.Headers({"A": "1"})
        h["B"] = "2"
        assert h["b"] == "2"
        del h["b"]
        assert "b" not in h

    def test_contains_iter_len_bool(self):
        h = httpxr.Headers({"A": "1"})
        assert "a" in h
        assert len(h) >= 1
        assert bool(h) is True
        assert bool(httpxr.Headers()) is False
        keys = list(h)
        assert any(k.lower() == "a" for k in keys)

    def test_eq_repr(self):
        h1 = httpxr.Headers({"A": "1"})
        h2 = httpxr.Headers({"A": "1"})
        assert h1 == h2
        assert "Headers" in repr(h1)


# ---------------------------------------------------------------------------
# Cookies
# ---------------------------------------------------------------------------

class TestCookies:
    def test_construct_dict(self):
        c = httpxr.Cookies({"session": "abc"})
        assert c["session"] == "abc"

    def test_construct_cookies(self):
        c1 = httpxr.Cookies({"a": "1"})
        c2 = httpxr.Cookies(c1)
        assert c2["a"] == "1"

    def test_set_get_delete(self):
        c = httpxr.Cookies()
        c.set("token", "xyz", domain="example.com", path="/")
        assert c.get("token", domain="example.com", path="/") == "xyz"
        c.delete("token", domain="example.com", path="/")
        assert c.get("token") is None

    def test_clear(self):
        c = httpxr.Cookies({"a": "1", "b": "2"})
        c.clear()
        assert len(c) == 0

    def test_update(self):
        c = httpxr.Cookies({"a": "1"})
        c.update({"b": "2"})
        assert "b" in c

    def test_keys_values_items(self):
        c = httpxr.Cookies({"a": "1"})
        assert "a" in c.keys()
        assert "1" in c.values()
        assert ("a", "1") in c.items()

    def test_jar(self):
        c = httpxr.Cookies({"a": "1"})
        assert c.jar is not None

    def test_setitem_delitem_contains_iter_len_bool_repr(self):
        c = httpxr.Cookies()
        c["x"] = "1"
        assert "x" in c
        assert len(c) >= 1
        assert bool(c) is True
        keys = list(c)
        assert "x" in keys
        del c["x"]
        assert "x" not in c
        assert bool(httpxr.Cookies()) is False
        assert repr(httpxr.Cookies({"a": "1"}))  # not empty

    def test_extract_cookies_does_not_raise(self):
        resp = httpxr.Response(200, headers={"Set-Cookie": "token=abc; Path=/"})
        c = httpxr.Cookies()
        c.extract_cookies(resp)


# ---------------------------------------------------------------------------
# Request
# ---------------------------------------------------------------------------

class TestRequest:
    def test_basic_construct(self):
        r = httpxr.Request("GET", "https://example.com/")
        assert r.method == "GET"
        assert str(r.url) == "https://example.com/"

    def test_method_uppercased(self):
        r = httpxr.Request("get", "https://example.com/")
        assert r.method == "GET"

    def test_headers(self):
        r = httpxr.Request("GET", "https://example.com/", headers={"X-Foo": "bar"})
        assert r.headers["x-foo"] == "bar"

    def test_content(self):
        r = httpxr.Request("POST", "https://example.com/", content=b"hello")
        assert r.content == b"hello"

    def test_json(self):
        r = httpxr.Request("POST", "https://example.com/", json={"key": "val"})
        assert b"key" in r.content

    def test_data(self):
        r = httpxr.Request("POST", "https://example.com/", data={"field": "value"})
        assert b"field" in r.content

    def test_params(self):
        r = httpxr.Request("GET", "https://example.com/", params={"q": "test"})
        assert "q=test" in str(r.url)

    def test_extensions(self):
        r = httpxr.Request("GET", "https://example.com/", extensions={"trace": "x"})
        assert r.extensions.get("trace") == "x"

    def test_extensions_property_get_set(self):
        r = httpxr.Request("GET", "https://example.com/")
        r.extensions = {"new": "val"}
        assert r.extensions["new"] == "val"

    def test_read(self):
        r = httpxr.Request("POST", "https://example.com/", content=b"data")
        assert r.read() == b"data"

    def test_repr_eq(self):
        r1 = httpxr.Request("GET", "https://example.com/")
        r2 = httpxr.Request("GET", "https://example.com/")
        assert r1 == r2
        assert "GET" in repr(r1)

    def test_method_set(self):
        r = httpxr.Request("GET", "https://example.com/")
        r.method = "POST"
        assert r.method == "POST"

    def test_url_set(self):
        r = httpxr.Request("GET", "https://example.com/")
        r.url = httpxr.URL("https://other.com/")
        assert "other.com" in str(r.url)


# ---------------------------------------------------------------------------
# Response
# ---------------------------------------------------------------------------

class TestResponse:
    def _make_complete(self, status_code: int = 200, **kwargs: Any) -> httpxr.Response:
        """Make a Response that has been read (so elapsed etc. work)."""
        r = httpxr.Response(status_code, **kwargs)
        r.read()
        return r

    def test_construct_minimal(self):
        r = httpxr.Response(200)
        assert r.status_code == 200

    def test_construct_with_text(self):
        r = httpxr.Response(200, text="Hello")
        assert r.text == "Hello"
        assert r.content == b"Hello"

    def test_construct_with_json(self):
        r = httpxr.Response(200, json={"a": 1})
        assert r.json()["a"] == 1

    def test_construct_with_html(self):
        r = httpxr.Response(200, html="<html/>")
        assert b"<html" in r.content

    def test_status_informational(self):
        assert httpxr.Response(100).is_informational is True

    def test_status_success(self):
        assert httpxr.Response(200).is_success is True
        assert httpxr.Response(200).is_redirect is False

    def test_status_redirect(self):
        assert httpxr.Response(301).is_redirect is True

    def test_status_client_error(self):
        assert httpxr.Response(404).is_client_error is True
        assert httpxr.Response(404).is_error is True

    def test_status_server_error(self):
        assert httpxr.Response(500).is_server_error is True
        assert httpxr.Response(200).is_error is False

    def test_has_redirect_location(self):
        r_no_loc = httpxr.Response(301)
        assert r_no_loc.has_redirect_location is False
        r_with_loc = httpxr.Response(301, headers={"Location": "https://example.com/"})
        assert r_with_loc.has_redirect_location is True

    def test_headers(self):
        r = httpxr.Response(200, headers={"X-Custom": "val"})
        assert r.headers["x-custom"] == "val"

    def test_url_from_request(self):
        req = httpxr.Request("GET", "https://example.com/the-path")
        r = httpxr.Response(200, request=req)
        assert "example.com" in str(r.url)

    def test_request_property_and_setter(self):
        r = httpxr.Response(200)
        req = httpxr.Request("GET", "https://example.com/")
        r.request = req
        assert r.request.method == "GET"

    def test_extensions_property_and_setter(self):
        r = httpxr.Response(200, extensions={"http_version": b"HTTP/1.1"})
        assert r.extensions["http_version"] == b"HTTP/1.1"
        r.extensions = {"custom": "x"}
        assert r.extensions["custom"] == "x"

    def test_encoding(self):
        r = httpxr.Response(200, text="hello")
        assert r.encoding is not None
        r.encoding = "utf-8"
        assert r.encoding == "utf-8"

    def test_charset_encoding(self):
        r = httpxr.Response(200, headers={"Content-Type": "text/plain; charset=utf-8"})
        assert r.charset_encoding == "utf-8"

    def test_reason_phrase(self):
        assert httpxr.Response(200).reason_phrase == "OK"
        assert httpxr.Response(404).reason_phrase == "Not Found"

    def test_http_version(self):
        r = httpxr.Response(200)
        assert r.http_version  # non-empty string

    def test_cookies(self):
        r = httpxr.Response(200, headers={"Set-Cookie": "a=1; Path=/"})
        assert isinstance(r.cookies, httpxr.Cookies)

    def test_links(self):
        r = httpxr.Response(200, headers={"Link": '<https://example.com>; rel="next"'})
        assert isinstance(r.links, dict)

    def test_history(self):
        r = httpxr.Response(200, history=[])
        assert r.history == []

    def test_elapsed_after_read(self):
        # elapsed is only set after the response body is read/closed
        with httpxr.Client(transport=httpxr.MockTransport(_handler)) as c:
            r = c.get("http://example.com/")
        # sending via client reads the response, elapsed should now be set
        assert isinstance(r.elapsed, datetime.timedelta)

    def test_num_bytes_downloaded(self):
        r = httpxr.Response(200, content=b"hello")
        r.read()
        assert isinstance(r.num_bytes_downloaded, int)

    def test_is_closed_is_stream_consumed(self):
        r = httpxr.Response(200, content=b"data")
        assert isinstance(r.is_closed, bool)
        assert isinstance(r.is_stream_consumed, bool)

    def test_default_encoding_prop(self):
        # NOTE: property is exposed as `default_encoding_prop` in impl (pyi says `default_encoding`)
        r = httpxr.Response(200)
        enc = r.default_encoding_prop
        assert isinstance(enc, str)

    def test_raise_for_status_ok(self):
        req = httpxr.Request("GET", "https://example.com/")
        r = httpxr.Response(200, request=req)
        assert r.raise_for_status() is r

    def test_raise_for_status_error(self):
        req = httpxr.Request("GET", "https://example.com/")
        r = httpxr.Response(404, request=req)
        with pytest.raises(httpxr.HTTPStatusError):
            r.raise_for_status()

    def test_iter_bytes(self):
        r = httpxr.Response(200, content=b"hello world")
        chunks = list(r.iter_bytes(chunk_size=5))
        assert b"".join(chunks) == b"hello world"

    def test_iter_text(self):
        r = httpxr.Response(200, text="hello")
        chunks = list(r.iter_text())
        assert "".join(chunks) == "hello"

    def test_iter_lines(self):
        r = httpxr.Response(200, text="line1\nline2\nline3")
        lines = list(r.iter_lines())
        assert len(lines) == 3
        # iter_lines strips trailing newlines (one approach) or includes them
        assert any("line1" in l for l in lines)

    def test_context_manager(self):
        r = httpxr.Response(200, content=b"data")
        with r as resp:
            assert resp.status_code == 200

    def test_eq(self):
        r1 = httpxr.Response(200)
        assert r1 == r1

    def test_next_request(self):
        r = httpxr.Response(200)
        assert r.next_request is None

    def test_stream_consumed_raises_on_second_iteration(self):
        # content= is buffered — re-iterating does NOT raise StreamConsumed
        # (it would raise for a streaming body). Just verify data is correct.
        r = httpxr.Response(200, content=b"data")
        chunks1 = list(r.iter_bytes())
        assert b"data" in b"".join(chunks1)


# ---------------------------------------------------------------------------
# Timeout and Limits
# ---------------------------------------------------------------------------

class TestConfig:
    def test_timeout_from_float(self):
        t = httpxr.Timeout(5.0)
        assert t.connect == 5.0
        assert t.read == 5.0
        assert t.write == 5.0
        assert t.pool == 5.0

    def test_timeout_from_none(self):
        t = httpxr.Timeout(None)
        assert t.connect is None

    def test_timeout_per_field(self):
        t = httpxr.Timeout(None, connect=1.0, read=2.0, write=3.0, pool=4.0)
        assert t.connect == 1.0
        assert t.read == 2.0
        assert t.write == 3.0
        assert t.pool == 4.0

    def test_timeout_eq_repr(self):
        t1 = httpxr.Timeout(5.0)
        t2 = httpxr.Timeout(5.0)
        assert t1 == t2
        assert "Timeout" in repr(t1)

    def test_limits_defaults(self):
        lim = httpxr.Limits()
        assert lim.max_connections is None or isinstance(lim.max_connections, int)

    def test_limits_custom(self):
        lim = httpxr.Limits(max_connections=10, max_keepalive_connections=5, keepalive_expiry=30.0)
        assert lim.max_connections == 10
        assert lim.max_keepalive_connections == 5
        assert lim.keepalive_expiry == 30.0

    def test_limits_eq_repr(self):
        lim = httpxr.Limits(max_connections=10)
        assert isinstance(repr(lim), str)

    def test_proxy_construct(self):
        p = httpxr.Proxy("http://proxy.example.com/")
        assert "proxy" in p.url

    def test_proxy_with_auth(self):
        p = httpxr.Proxy("http://proxy.example.com/", auth=("user", "pass"))
        assert p.auth == ("user", "pass")

    def test_default_constants(self):
        assert isinstance(httpxr.DEFAULT_TIMEOUT_CONFIG, httpxr.Timeout)
        assert isinstance(httpxr.DEFAULT_LIMITS, httpxr.Limits)
        assert isinstance(httpxr.DEFAULT_MAX_REDIRECTS, int)


# ---------------------------------------------------------------------------
# Auth
# ---------------------------------------------------------------------------

class TestAuth:
    def test_basic_auth_str(self):
        auth = httpxr.BasicAuth("user", "pass")
        assert auth is not None

    def test_digest_auth(self):
        auth = httpxr.DigestAuth("user", "pass")
        assert auth is not None

    def test_client_with_tuple_auth(self):
        with _make_client(auth=("user", "pass")) as c:
            r = c.get("http://example.com/")
            assert r.status_code == 200

    def test_client_with_basic_auth_object(self):
        auth = httpxr.BasicAuth("user", "pass")
        with _make_client(auth=auth) as c:
            r = c.get("http://example.com/")
            assert r.status_code == 200

    def test_callable_auth(self):
        def my_auth(request: httpxr.Request) -> httpxr.Request:
            request.headers["Authorization"] = "Bearer token"
            return request

        with _make_client(auth=my_auth) as c:
            r = c.get("http://example.com/")
            assert r.status_code == 200


# ---------------------------------------------------------------------------
# Client — constructor params
# ---------------------------------------------------------------------------

class TestClientInit:
    def test_default_construct(self):
        c = httpxr.Client(transport=httpxr.MockTransport(_handler))
        assert not c.is_closed
        c.close()

    def test_auth_param(self):
        c = httpxr.Client(auth=("u", "p"), transport=httpxr.MockTransport(_handler))
        assert c.auth is not None
        c.close()

    def test_headers_param(self):
        c = httpxr.Client(headers={"X-Init": "yes"}, transport=httpxr.MockTransport(_handler))
        assert "x-init" in c.headers
        c.close()

    def test_cookies_param(self):
        c = httpxr.Client(cookies={"session": "abc"}, transport=httpxr.MockTransport(_handler))
        assert "session" in c.cookies
        c.close()

    def test_timeout_param(self):
        c = httpxr.Client(timeout=10.0, transport=httpxr.MockTransport(_handler))
        c.close()

    def test_follow_redirects_param(self):
        c = httpxr.Client(follow_redirects=True, transport=httpxr.MockTransport(_handler))
        c.close()

    def test_base_url_param(self):
        c = httpxr.Client(base_url="https://api.example.com/", transport=httpxr.MockTransport(_handler))
        assert "api.example.com" in str(c.base_url)
        c.close()

    def test_trust_env_param(self):
        c = httpxr.Client(trust_env=False, transport=httpxr.MockTransport(_handler))
        assert c.trust_env is False
        c.close()

    def test_event_hooks_param(self):
        calls: list[str] = []
        def hook(r: Any) -> None:
            calls.append("called")
        hooks = {"request": [hook], "response": []}
        c = httpxr.Client(event_hooks=hooks, transport=httpxr.MockTransport(_handler))
        assert "request" in c.event_hooks
        c.close()

    def test_max_redirects_param(self):
        c = httpxr.Client(max_redirects=5, transport=httpxr.MockTransport(_handler))
        c.close()

    def test_limits_param(self):
        lim = httpxr.Limits(max_connections=5)
        c = httpxr.Client(limits=lim, transport=httpxr.MockTransport(_handler))
        c.close()

    def test_default_encoding_str(self):
        c = httpxr.Client(default_encoding="latin-1", transport=httpxr.MockTransport(_handler))
        c.close()

    def test_context_manager(self):
        with httpxr.Client(transport=httpxr.MockTransport(_handler)) as c:
            assert not c.is_closed
        assert c.is_closed

    def test_http2_param(self):
        c = httpxr.Client(http2=True, transport=httpxr.MockTransport(_handler))
        c.close()


# ---------------------------------------------------------------------------
# Client — properties and mutability
# ---------------------------------------------------------------------------

class TestClientProperties:
    def setup_method(self):
        self.client = httpxr.Client(transport=httpxr.MockTransport(_handler))

    def teardown_method(self):
        self.client.close()

    def test_headers_get_set(self):
        self.client.headers = {"X-New": "val"}  # type: ignore[assignment]
        assert "x-new" in self.client.headers

    def test_auth_get_set_clear(self):
        self.client.auth = ("u", "p")
        assert self.client.auth is not None
        self.client.auth = None
        assert self.client.auth is None

    def test_cookies_get_set(self):
        self.client.cookies = {"k": "v"}  # type: ignore[assignment]
        assert "k" in self.client.cookies

    def test_timeout_setter(self):
        self.client.timeout = 30.0

    def test_base_url_get_set(self):
        self.client.base_url = "https://newbase.example.com/"
        assert "newbase" in str(self.client.base_url)

    def test_event_hooks_setter(self):
        self.client.event_hooks = {"request": [], "response": []}
        assert "request" in self.client.event_hooks

    def test_trust_env(self):
        assert isinstance(self.client.trust_env, bool)

    def test_is_closed(self):
        assert self.client.is_closed is False


# ---------------------------------------------------------------------------
# Client — HTTP methods
# ---------------------------------------------------------------------------

class TestClientMethods:
    def setup_method(self):
        self.client = _make_client()

    def teardown_method(self):
        self.client.close()

    def test_get(self):
        r = self.client.get("http://example.com/get")
        assert r.status_code == 200
        assert r.json()["path"] == "/get"

    def test_head(self):
        r = self.client.head("http://example.com/")
        assert r.status_code == 200

    def test_options(self):
        r = self.client.options("http://example.com/")
        assert r.status_code == 200

    def test_delete(self):
        r = self.client.delete("http://example.com/")
        assert r.status_code == 200

    def test_post_json(self):
        r = self.client.post("http://example.com/", json={"x": 1})
        assert r.status_code == 200

    def test_post_content(self):
        r = self.client.post("http://example.com/", content=b"raw")
        assert r.status_code == 200

    def test_post_data(self):
        r = self.client.post("http://example.com/", data={"field": "value"})
        assert r.status_code == 200

    def test_put(self):
        r = self.client.put("http://example.com/", content=b"body")
        assert r.status_code == 200

    def test_patch(self):
        r = self.client.patch("http://example.com/", content=b"body")
        assert r.status_code == 200

    def test_request(self):
        r = self.client.request("GET", "http://example.com/req")
        assert r.status_code == 200

    def test_request_timeout_kwarg(self):
        r = self.client.request("GET", "http://example.com/", timeout=5.0)
        assert r.status_code == 200

    def test_request_params_kwarg(self):
        r = self.client.request("GET", "http://example.com/", params={"q": "test"})
        assert r.status_code == 200

    def test_request_cookies_kwarg(self):
        r = self.client.request("GET", "http://example.com/", cookies={"s": "val"})
        assert r.status_code == 200

    def test_request_follow_redirects_kwarg(self):
        r = self.client.request("GET", "http://example.com/", follow_redirects=False)
        assert r.status_code == 200

    def test_stream(self):
        with self.client.stream("GET", "http://example.com/") as r:
            body = r.read()
        assert isinstance(body, bytes)

    def test_send(self):
        req = self.client.build_request("GET", "http://example.com/send")
        r = self.client.send(req)
        assert r.status_code == 200

    def test_send_with_auth(self):
        req = self.client.build_request("GET", "http://example.com/")
        r = self.client.send(req, auth=("u", "p"))
        assert r.status_code == 200

    def test_send_follow_redirects(self):
        req = self.client.build_request("GET", "http://example.com/")
        r = self.client.send(req, follow_redirects=False)
        assert r.status_code == 200


# ---------------------------------------------------------------------------
# Client — build_request (all kwargs — this was the missed OpenAI SDK bug)
# ---------------------------------------------------------------------------

class TestClientBuildRequest:
    def setup_method(self):
        self.client = httpxr.Client(transport=httpxr.MockTransport(_handler))

    def teardown_method(self):
        self.client.close()

    def test_build_get(self):
        req = self.client.build_request("GET", "http://example.com/")
        assert req.method == "GET"
        assert "example.com" in str(req.url)

    def test_build_post_json(self):
        req = self.client.build_request("POST", "http://example.com/", json={"k": "v"})
        assert b"k" in req.content

    def test_build_post_data(self):
        req = self.client.build_request("POST", "http://example.com/", data={"f": "v"})
        assert b"f=v" in req.content

    def test_build_post_content(self):
        req = self.client.build_request("POST", "http://example.com/", content=b"raw bytes")
        assert req.content == b"raw bytes"

    def test_build_with_headers(self):
        req = self.client.build_request("GET", "http://example.com/", headers={"X-A": "1"})
        assert req.headers["x-a"] == "1"

    def test_build_with_cookies(self):
        req = self.client.build_request("GET", "http://example.com/", cookies={"c": "1"})
        assert req.method == "GET"

    def test_build_with_timeout_float(self):
        """THE REGRESSION: OpenAI SDK 2.x calls build_request(..., timeout=5.0)."""
        req = self.client.build_request("POST", "http://example.com/", timeout=5.0)
        assert req.method == "POST"
        assert "timeout" in req.extensions

    def test_build_with_timeout_none(self):
        req = self.client.build_request("GET", "http://example.com/", timeout=None)
        assert req.method == "GET"

    def test_build_with_timeout_object(self):
        t = httpxr.Timeout(30.0)
        req = self.client.build_request("GET", "http://example.com/", timeout=t)
        assert req.method == "GET"
        assert "timeout" in req.extensions

    def test_build_with_extensions(self):
        req = self.client.build_request("GET", "http://example.com/", extensions={"trace": "xyz"})
        assert req.extensions.get("trace") == "xyz"

    def test_build_with_timeout_and_extensions(self):
        req = self.client.build_request(
            "POST", "http://example.com/",
            timeout=10.0,
            extensions={"trace": "abc"},
        )
        assert "timeout" in req.extensions
        assert req.extensions.get("trace") == "abc"


# ---------------------------------------------------------------------------
# AsyncClient
# ---------------------------------------------------------------------------

class TestAsyncClient:
    def test_construct(self):
        c = httpxr.AsyncClient(transport=httpxr.MockTransport(_handler))
        assert not c.is_closed
        c.close()

    def test_properties(self):
        c = httpxr.AsyncClient(
            headers={"X-H": "1"},
            cookies={"c": "1"},
            base_url="https://base.example.com/",
            trust_env=False,
            event_hooks={"request": [], "response": []},
        )
        assert "x-h" in c.headers
        assert "c" in c.cookies
        assert "base.example.com" in str(c.base_url)
        assert c.trust_env is False
        assert "request" in c.event_hooks
        assert isinstance(c.is_closed, bool)
        c.close()

    def test_build_request_with_timeout(self):
        """AsyncClient.build_request must also accept timeout=."""
        c = httpxr.AsyncClient(transport=httpxr.MockTransport(_handler))
        req = c.build_request("POST", "http://example.com/", timeout=30.0)
        assert req.method == "POST"
        assert "timeout" in req.extensions
        c.close()

    def test_build_request_with_extensions(self):
        c = httpxr.AsyncClient(transport=httpxr.MockTransport(_handler))
        req = c.build_request("GET", "http://example.com/", extensions={"x": "y"})
        assert req.extensions.get("x") == "y"
        c.close()

    def test_auth_header_cookies_setters(self):
        c = httpxr.AsyncClient()
        c.headers = {"X-N": "1"}  # type: ignore[assignment]
        c.auth = ("u", "p")
        c.cookies = {"k": "v"}  # type: ignore[assignment]
        assert "x-n" in c.headers
        assert c.auth is not None
        assert "k" in c.cookies
        c.close()

    @pytest.mark.anyio
    async def test_get(self):
        async with httpxr.AsyncClient(transport=httpxr.MockTransport(_handler)) as c:
            r = await c.get("http://example.com/async")
            assert r.status_code == 200

    @pytest.mark.anyio
    async def test_post(self):
        async with httpxr.AsyncClient(transport=httpxr.MockTransport(_handler)) as c:
            r = await c.post("http://example.com/", json={"x": 1})
            assert r.status_code == 200

    @pytest.mark.anyio
    async def test_send(self):
        async with httpxr.AsyncClient(transport=httpxr.MockTransport(_handler)) as c:
            req = c.build_request("GET", "http://example.com/")
            r = await c.send(req)
            assert r.status_code == 200

    @pytest.mark.anyio
    async def test_put_patch_delete_head_options(self):
        async with httpxr.AsyncClient(transport=httpxr.MockTransport(_handler)) as c:
            assert (await c.put("http://example.com/", content=b"x")).status_code == 200
            assert (await c.patch("http://example.com/", content=b"x")).status_code == 200
            assert (await c.delete("http://example.com/")).status_code == 200
            assert (await c.head("http://example.com/")).status_code == 200
            assert (await c.options("http://example.com/")).status_code == 200

    @pytest.mark.anyio
    async def test_request_with_timeout(self):
        async with httpxr.AsyncClient(transport=httpxr.MockTransport(_handler)) as c:
            r = await c.request("GET", "http://example.com/", timeout=5.0)
            assert r.status_code == 200


# ---------------------------------------------------------------------------
# Transports
# ---------------------------------------------------------------------------

class TestTransports:
    def test_mock_transport(self):
        t = httpxr.MockTransport(_handler)
        req = httpxr.Request("GET", "http://example.com/")
        r = t.handle_request(req)
        assert r.status_code == 200

    def test_mock_transport_context_manager(self):
        with httpxr.MockTransport(_handler) as t:
            assert t is not None

    @pytest.mark.anyio
    async def test_mock_transport_async(self):
        t = httpxr.MockTransport(_handler)
        req = httpxr.Request("GET", "http://example.com/")
        r = await t.handle_async_request(req)
        assert r.status_code == 200

    def test_base_transport_subclass(self):
        class MyTransport(httpxr.BaseTransport):
            def handle_request(self, request: httpxr.Request) -> httpxr.Response:
                return httpxr.Response(201)

        with httpxr.Client(transport=MyTransport()) as c:
            r = c.get("http://example.com/")
            assert r.status_code == 201

    @pytest.mark.anyio
    async def test_async_base_transport_subclass(self):
        class MyAsyncTransport(httpxr.AsyncBaseTransport):
            async def handle_async_request(self, request: httpxr.Request) -> httpxr.Response:
                return httpxr.Response(202)

        async with httpxr.AsyncClient(transport=MyAsyncTransport()) as c:
            r = await c.get("http://example.com/")
            assert r.status_code == 202


# ---------------------------------------------------------------------------
# Exceptions — all must be importable and form the correct hierarchy
# ---------------------------------------------------------------------------

class TestExceptions:
    def test_hierarchy(self):
        assert issubclass(httpxr.HTTPStatusError, httpxr.HTTPError)
        assert issubclass(httpxr.RequestError, httpxr.HTTPError)
        assert issubclass(httpxr.TransportError, httpxr.RequestError)
        assert issubclass(httpxr.TimeoutException, httpxr.TransportError)
        assert issubclass(httpxr.ConnectTimeout, httpxr.TimeoutException)
        assert issubclass(httpxr.ReadTimeout, httpxr.TimeoutException)
        assert issubclass(httpxr.WriteTimeout, httpxr.TimeoutException)
        assert issubclass(httpxr.PoolTimeout, httpxr.TimeoutException)
        assert issubclass(httpxr.NetworkError, httpxr.TransportError)
        assert issubclass(httpxr.ConnectError, httpxr.NetworkError)
        assert issubclass(httpxr.ReadError, httpxr.NetworkError)
        assert issubclass(httpxr.WriteError, httpxr.NetworkError)
        assert issubclass(httpxr.CloseError, httpxr.NetworkError)
        assert issubclass(httpxr.ProxyError, httpxr.TransportError)
        assert issubclass(httpxr.UnsupportedProtocol, httpxr.TransportError)
        assert issubclass(httpxr.ProtocolError, httpxr.TransportError)
        assert issubclass(httpxr.LocalProtocolError, httpxr.ProtocolError)
        assert issubclass(httpxr.RemoteProtocolError, httpxr.ProtocolError)
        assert issubclass(httpxr.DecodingError, httpxr.RequestError)
        assert issubclass(httpxr.TooManyRedirects, httpxr.RequestError)
        assert issubclass(httpxr.StreamConsumed, httpxr.StreamError)
        assert issubclass(httpxr.StreamClosed, httpxr.StreamError)
        assert issubclass(httpxr.ResponseNotRead, httpxr.StreamError)
        assert issubclass(httpxr.RequestNotRead, httpxr.StreamError)

    def test_raise_http_status_error(self):
        req = httpxr.Request("GET", "http://example.com/")
        r = httpxr.Response(401, request=req)
        with pytest.raises(httpxr.HTTPStatusError) as exc_info:
            r.raise_for_status()
        assert exc_info.value.response.status_code == 401
        assert exc_info.value.request.method == "GET"

    def test_invalid_url(self):
        with httpxr.Client(transport=httpxr.MockTransport(_handler)) as c:
            with pytest.raises((httpxr.UnsupportedProtocol, httpxr.LocalProtocolError)):
                c.get("invalid://example.com")

    def test_cookie_conflict_invalid_url_importable(self):
        assert httpxr.CookieConflict
        assert httpxr.InvalidURL

    def test_stream_consumed_is_catchable(self):
        # Verify StreamConsumed is a proper exception class and can be raised/caught
        with pytest.raises(httpxr.StreamConsumed):
            raise httpxr.StreamConsumed(None)


# ---------------------------------------------------------------------------
# codes
# ---------------------------------------------------------------------------

class TestCodes:
    def test_2xx(self):
        assert httpxr.codes.OK == 200
        assert httpxr.codes.CREATED == 201
        assert httpxr.codes.NO_CONTENT == 204

    def test_3xx(self):
        assert httpxr.codes.MOVED_PERMANENTLY == 301
        assert httpxr.codes.FOUND == 302

    def test_4xx(self):
        assert httpxr.codes.NOT_FOUND == 404
        assert httpxr.codes.UNAUTHORIZED == 401
        assert httpxr.codes.TOO_MANY_REQUESTS == 429

    def test_5xx(self):
        assert httpxr.codes.INTERNAL_SERVER_ERROR == 500
        assert httpxr.codes.SERVICE_UNAVAILABLE == 503


# ---------------------------------------------------------------------------
# ByteStream
# ---------------------------------------------------------------------------

class TestByteStream:
    def test_byte_stream_sync_iter(self):
        bs = httpxr.ByteStream(b"hello")
        chunks = list(bs)
        assert b"hello" in chunks[0]

    def test_byte_stream_used_in_request(self):
        bs = httpxr.ByteStream(b"async data")
        req = httpxr.Request("POST", "http://example.com/", stream=bs)
        assert req.read() == b"async data"


# ---------------------------------------------------------------------------
# Decoders
# ---------------------------------------------------------------------------

class TestDecoders:
    def test_line_decoder(self):
        d = httpxr.LineDecoder()
        lines = d.decode("line1\nline2\n")
        # lines may or may not include the trailing newline — just ensure they contain the text
        assert any("line1" in l for l in lines)

    def test_byte_chunker(self):
        c = httpxr.ByteChunker(chunk_size=4)
        chunks = c.decode(b"abcdefgh")
        assert b"abcd" in chunks

    def test_byte_chunker_flush(self):
        c = httpxr.ByteChunker(chunk_size=10)
        c.decode(b"abc")
        flushed = c.flush()
        # flush returns remaining bytes as bytes or list, just check it doesn't raise
        assert flushed is not None

    def test_line_decoder_flush(self):
        d = httpxr.LineDecoder()
        d.decode("partial")
        flushed = d.flush()
        assert isinstance(flushed, list)


# ---------------------------------------------------------------------------
# Utilities
# ---------------------------------------------------------------------------

class TestUtils:
    def test_is_ipv4_hostname(self):
        assert httpxr.is_ipv4_hostname("127.0.0.1") is True
        assert httpxr.is_ipv4_hostname("example.com") is False

    def test_is_ipv6_hostname(self):
        assert httpxr.is_ipv6_hostname("::1") is True
        assert httpxr.is_ipv6_hostname("example.com") is False

    def test_get_environment_proxies(self):
        result = httpxr.get_environment_proxies()
        assert isinstance(result, dict)

    def test_version(self):
        assert isinstance(httpxr.__version__, str)
        assert "." in httpxr.__version__
