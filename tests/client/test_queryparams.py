import httpr


def hello_world(request: httpr.Request) -> httpr.Response:
    return httpr.Response(200, text="Hello, world")


def test_client_queryparams():
    client = httpr.Client(params={"a": "b"})
    assert isinstance(client.params, httpr.QueryParams)
    assert client.params["a"] == "b"


def test_client_queryparams_string():
    client = httpr.Client(params="a=b")
    assert isinstance(client.params, httpr.QueryParams)
    assert client.params["a"] == "b"

    client = httpr.Client()
    client.params = "a=b"
    assert isinstance(client.params, httpr.QueryParams)
    assert client.params["a"] == "b"


def test_client_queryparams_echo():
    url = "http://example.org/echo_queryparams"
    client_queryparams = "first=str"
    request_queryparams = {"second": "dict"}
    client = httpr.Client(
        transport=httpr.MockTransport(hello_world), params=client_queryparams
    )
    response = client.get(url, params=request_queryparams)

    assert response.status_code == 200
    assert response.url == "http://example.org/echo_queryparams?first=str&second=dict"
