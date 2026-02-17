"""
Additional SSL/TLS tests for httpxr.

These are separate from the ported httpx test suite and cover
sync/async client HTTPS usage, custom SSLContext, untrusted cert
rejection, POST over TLS, streaming, and redirects.
"""

from __future__ import annotations

import pytest

import httpxr


# ---------------------------------------------------------------------------
# Sync client – basic HTTPS
# ---------------------------------------------------------------------------


def test_sync_client_https_get(https_server, cert_pem_file):
    """Sync Client can make a GET request over HTTPS when given the CA cert."""
    with httpxr.Client(verify=cert_pem_file) as client:
        response = client.get(https_server.url)
    assert response.status_code == 200
    assert response.text == "Hello, world!"


def test_sync_client_https_verify_false(https_server):
    """Sync Client with verify=False skips certificate verification."""
    with httpxr.Client(verify=False) as client:
        response = client.get(https_server.url)
    assert response.status_code == 200
    assert response.text == "Hello, world!"


def test_sync_client_ssl_context_verify(https_server, cert_pem_file):
    """Sync Client accepts a pre-built ssl.SSLContext as the verify parameter."""
    ctx = httpxr.create_ssl_context()
    ctx.load_verify_locations(cert_pem_file)
    with httpxr.Client(verify=ctx) as client:
        response = client.get(https_server.url)
    assert response.status_code == 200
    assert response.text == "Hello, world!"


def test_sync_client_https_untrusted_cert_fails(https_server):
    """Sync Client without the CA cert should fail to connect."""
    with httpxr.Client() as client:
        with pytest.raises(httpxr.ConnectError):
            client.get(https_server.url)


def test_sync_client_https_post(https_server, cert_pem_file):
    """Sync Client can POST a body over HTTPS."""
    url = https_server.url.copy_with(path="/echo_body")
    with httpxr.Client(verify=cert_pem_file) as client:
        response = client.post(url, content=b"secure payload")
    assert response.status_code == 200
    assert response.content == b"secure payload"


def test_sync_client_https_stream(https_server, cert_pem_file):
    """Sync Client can stream a response over HTTPS."""
    with httpxr.Client(verify=cert_pem_file) as client:
        with client.stream("GET", https_server.url) as response:
            body = response.read()
    assert response.status_code == 200
    assert body == b"Hello, world!"


def test_sync_client_https_redirect(https_server, cert_pem_file):
    """Sync Client follows a 301 redirect over HTTPS."""
    url = https_server.url.copy_with(path="/redirect_301")
    with httpxr.Client(verify=cert_pem_file, follow_redirects=True) as client:
        response = client.get(url)
    assert response.status_code == 200
    assert len(response.history) == 1
    assert response.history[0].status_code == 301


# ---------------------------------------------------------------------------
# Async client – basic HTTPS
# ---------------------------------------------------------------------------


@pytest.mark.anyio
async def test_async_client_https_get(https_server, cert_pem_file):
    """AsyncClient can make a GET request over HTTPS when given the CA cert."""
    async with httpxr.AsyncClient(verify=cert_pem_file) as client:
        response = await client.get(https_server.url)
    assert response.status_code == 200
    assert response.text == "Hello, world!"


@pytest.mark.anyio
async def test_async_client_https_verify_false(https_server):
    """AsyncClient with verify=False skips certificate verification."""
    async with httpxr.AsyncClient(verify=False) as client:
        response = await client.get(https_server.url)
    assert response.status_code == 200
    assert response.text == "Hello, world!"


@pytest.mark.anyio
async def test_async_client_ssl_context_verify(https_server, cert_pem_file):
    """AsyncClient accepts a pre-built ssl.SSLContext as the verify parameter."""
    ctx = httpxr.create_ssl_context()
    ctx.load_verify_locations(cert_pem_file)
    async with httpxr.AsyncClient(verify=ctx) as client:
        response = await client.get(https_server.url)
    assert response.status_code == 200
    assert response.text == "Hello, world!"


@pytest.mark.anyio
async def test_async_client_https_untrusted_cert_fails(https_server):
    """AsyncClient without the CA cert should fail to connect."""
    async with httpxr.AsyncClient() as client:
        with pytest.raises(httpxr.ConnectError):
            await client.get(https_server.url)


@pytest.mark.anyio
async def test_async_client_https_post(https_server, cert_pem_file):
    """AsyncClient can POST a body over HTTPS."""
    url = https_server.url.copy_with(path="/echo_body")
    async with httpxr.AsyncClient(verify=cert_pem_file) as client:
        response = await client.post(url, content=b"secure payload")
    assert response.status_code == 200
    assert response.content == b"secure payload"


@pytest.mark.anyio
async def test_async_client_https_stream(https_server, cert_pem_file):
    """AsyncClient can stream a response over HTTPS."""
    async with httpxr.AsyncClient(verify=cert_pem_file) as client:
        async with client.stream("GET", https_server.url) as response:
            body = await response.aread()
    assert response.status_code == 200
    assert body == b"Hello, world!"


@pytest.mark.anyio
async def test_async_client_https_redirect(https_server, cert_pem_file):
    """AsyncClient follows a 301 redirect over HTTPS."""
    url = https_server.url.copy_with(path="/redirect_301")
    async with httpxr.AsyncClient(verify=cert_pem_file, follow_redirects=True) as client:
        response = await client.get(url)
    assert response.status_code == 200
    assert response.text == "Hello, world!"
