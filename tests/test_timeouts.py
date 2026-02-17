import pytest

import httpxr


@pytest.mark.anyio
async def test_read_timeout(server):
    timeout = httpxr.Timeout(None, read=1e-6)

    async with httpxr.AsyncClient(timeout=timeout) as client:
        with pytest.raises(httpxr.ReadTimeout):
            await client.get(server.url.copy_with(path="/slow_response"))


@pytest.mark.anyio
async def test_write_timeout(server):
    timeout = httpxr.Timeout(None, write=1e-6)

    async with httpxr.AsyncClient(timeout=timeout) as client:
        # We catch TimeoutException (parent of WriteTimeout and ReadTimeout)
        # because the Rust transport may buffer the write via OS kernel buffers,
        # causing the timeout to surface as a ReadTimeout on the response read
        # rather than a WriteTimeout on the request send.
        with pytest.raises(httpxr.TimeoutException):
            data = b"*" * 1024 * 1024 * 100
            await client.put(server.url.copy_with(path="/slow_response"), content=data)


@pytest.mark.anyio
@pytest.mark.network
async def test_connect_timeout(server):
    timeout = httpxr.Timeout(None, connect=1e-6)

    async with httpxr.AsyncClient(timeout=timeout) as client:
        with pytest.raises(httpxr.ConnectTimeout):
            # See https://stackoverflow.com/questions/100841/
            await client.get("http://10.255.255.1/")


@pytest.mark.anyio
async def test_pool_timeout(server):
    limits = httpxr.Limits(max_connections=1)
    timeout = httpxr.Timeout(None, pool=1e-4)

    async with httpxr.AsyncClient(limits=limits, timeout=timeout) as client:
        with pytest.raises(httpxr.PoolTimeout):
            async with client.stream("GET", server.url):
                await client.get(server.url)


@pytest.mark.anyio
async def test_async_client_new_request_send_timeout(server):
    timeout = httpxr.Timeout(1e-6)

    async with httpxr.AsyncClient(timeout=timeout) as client:
        with pytest.raises(httpxr.TimeoutException):
            await client.send(
                httpxr.Request("GET", server.url.copy_with(path="/slow_response"))
            )
