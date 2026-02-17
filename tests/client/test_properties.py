import httpxr


def test_client_base_url():
    client = httpxr.Client()
    client.base_url = "https://www.example.org/"
    assert isinstance(client.base_url, httpxr.URL)
    assert client.base_url == "https://www.example.org/"


def test_client_base_url_without_trailing_slash():
    client = httpxr.Client()
    client.base_url = "https://www.example.org/path"
    assert isinstance(client.base_url, httpxr.URL)
    assert client.base_url == "https://www.example.org/path/"


def test_client_base_url_with_trailing_slash():
    client = httpxr.Client()
    client.base_url = "https://www.example.org/path/"
    assert isinstance(client.base_url, httpxr.URL)
    assert client.base_url == "https://www.example.org/path/"


def test_client_headers():
    client = httpxr.Client()
    client.headers = {"a": "b"}
    assert isinstance(client.headers, httpxr.Headers)
    assert client.headers["A"] == "b"


def test_client_cookies():
    client = httpxr.Client()
    client.cookies = {"a": "b"}
    assert isinstance(client.cookies, httpxr.Cookies)
    mycookies = list(client.cookies.jar)
    assert len(mycookies) == 1
    assert mycookies[0].name == "a" and mycookies[0].value == "b"


def test_client_timeout():
    expected_timeout = 12.0
    client = httpxr.Client()

    client.timeout = expected_timeout

    assert isinstance(client.timeout, httpxr.Timeout)
    assert client.timeout.connect == expected_timeout
    assert client.timeout.read == expected_timeout
    assert client.timeout.write == expected_timeout
    assert client.timeout.pool == expected_timeout


def test_client_event_hooks():
    def on_request(request):
        pass  # pragma: no cover

    client = httpxr.Client()
    client.event_hooks = {"request": [on_request]}
    assert client.event_hooks == {"request": [on_request], "response": []}


def test_client_trust_env():
    client = httpxr.Client()
    assert client.trust_env

    client = httpxr.Client(trust_env=False)
    assert not client.trust_env
