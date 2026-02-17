import httpxr


def hello_world(request: httpxr.Request) -> httpxr.Response:
    return httpxr.Response(200, text="Hello, world")


def test_client_queryparams():
    client = httpxr.Client(params={"a": "b"})
    assert isinstance(client.params, httpxr.QueryParams)
    assert client.params["a"] == "b"


def test_client_queryparams_string():
    client = httpxr.Client(params="a=b")
    assert isinstance(client.params, httpxr.QueryParams)
    assert client.params["a"] == "b"

    client = httpxr.Client()
    client.params = "a=b"
    assert isinstance(client.params, httpxr.QueryParams)
    assert client.params["a"] == "b"


def test_client_queryparams_echo():
    url = "http://example.org/echo_queryparams"
    client_queryparams = "first=str"
    request_queryparams = {"second": "dict"}
    client = httpxr.Client(
        transport=httpxr.MockTransport(hello_world), params=client_queryparams
    )
    response = client.get(url, params=request_queryparams)

    assert response.status_code == 200
    assert response.url == "http://example.org/echo_queryparams?first=str&second=dict"
