import pytest

import httpxr


class CustomClient(httpxr.Client):
    pass


class CustomAsyncClient(httpxr.AsyncClient):
    pass


def test_custom_client_instantiation():
    client = CustomClient()
    assert isinstance(client, httpxr.Client)
    assert isinstance(client, CustomClient)
    client.close()


@pytest.mark.asyncio
async def test_custom_async_client_instantiation():
    async_client = CustomAsyncClient()
    assert isinstance(async_client, httpxr.AsyncClient)
    assert isinstance(async_client, CustomAsyncClient)
    await async_client.aclose()
