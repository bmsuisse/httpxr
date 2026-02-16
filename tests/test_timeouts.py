import pytest

import httpr


@pytest.mark.anyio
async def test_read_timeout(server):
    timeout = httpr.Timeout(None, read=1e-6)

    async with httpr.AsyncClient(timeout=timeout) as client:
        with pytest.raises(httpr.ReadTimeout):
            await client.get(server.url.copy_with(path="/slow_response"))


@pytest.mark.anyio
async def test_write_timeout(server):
    timeout = httpr.Timeout(None, write=1e-6)

    async with httpr.AsyncClient(timeout=timeout) as client:
        with pytest.raises(httpr.WriteTimeout):
            data = b"*" * 1024 * 1024 * 100
            await client.put(server.url.copy_with(path="/slow_response"), content=data)


@pytest.mark.anyio
@pytest.mark.network
async def test_connect_timeout(server):
    timeout = httpr.Timeout(None, connect=1e-6)

    async with httpr.AsyncClient(timeout=timeout) as client:
        with pytest.raises(httpr.ConnectTimeout):
            # See https://stackoverflow.com/questions/100841/
            await client.get("http://10.255.255.1/")


@pytest.mark.anyio
async def test_pool_timeout(server):
    limits = httpr.Limits(max_connections=1)
    timeout = httpr.Timeout(None, pool=1e-4)

    async with httpr.AsyncClient(limits=limits, timeout=timeout) as client:
        with pytest.raises(httpr.PoolTimeout):
            async with client.stream("GET", server.url):
                await client.get(server.url)


@pytest.mark.anyio
async def test_async_client_new_request_send_timeout(server):
    timeout = httpr.Timeout(1e-6)

    async with httpr.AsyncClient(timeout=timeout) as client:
        with pytest.raises(httpr.TimeoutException):
            await client.send(
                httpr.Request("GET", server.url.copy_with(path="/slow_response"))
            )
