import ssl
import typing
from pathlib import Path

import certifi
import pytest

import httpr


def test_load_ssl_config():
    context = httpr.create_ssl_context()
    assert context.verify_mode == ssl.VerifyMode.CERT_REQUIRED
    assert context.check_hostname is True


def test_load_ssl_config_verify_non_existing_file():
    with pytest.raises(IOError):
        context = httpr.create_ssl_context()
        context.load_verify_locations(cafile="/path/to/nowhere")


def test_load_ssl_with_keylog(monkeypatch: typing.Any) -> None:
    monkeypatch.setenv("SSLKEYLOGFILE", "test")
    context = httpr.create_ssl_context()
    assert context.keylog_filename == "test"


def test_load_ssl_config_verify_existing_file():
    context = httpr.create_ssl_context()
    context.load_verify_locations(capath=certifi.where())
    assert context.verify_mode == ssl.VerifyMode.CERT_REQUIRED
    assert context.check_hostname is True


def test_load_ssl_config_verify_directory():
    context = httpr.create_ssl_context()
    context.load_verify_locations(capath=Path(certifi.where()).parent)
    assert context.verify_mode == ssl.VerifyMode.CERT_REQUIRED
    assert context.check_hostname is True


def test_load_ssl_config_cert_and_key(cert_pem_file, cert_private_key_file):
    context = httpr.create_ssl_context()
    context.load_cert_chain(cert_pem_file, cert_private_key_file)
    assert context.verify_mode == ssl.VerifyMode.CERT_REQUIRED
    assert context.check_hostname is True


@pytest.mark.parametrize("password", [b"password", "password"])
def test_load_ssl_config_cert_and_encrypted_key(
    cert_pem_file, cert_encrypted_private_key_file, password
):
    context = httpr.create_ssl_context()
    context.load_cert_chain(cert_pem_file, cert_encrypted_private_key_file, password)
    assert context.verify_mode == ssl.VerifyMode.CERT_REQUIRED
    assert context.check_hostname is True


def test_load_ssl_config_cert_and_key_invalid_password(
    cert_pem_file, cert_encrypted_private_key_file
):
    with pytest.raises(ssl.SSLError):
        context = httpr.create_ssl_context()
        context.load_cert_chain(
            cert_pem_file, cert_encrypted_private_key_file, "password1"
        )


def test_load_ssl_config_cert_without_key_raises(cert_pem_file):
    with pytest.raises(ssl.SSLError):
        context = httpr.create_ssl_context()
        context.load_cert_chain(cert_pem_file)


def test_load_ssl_config_no_verify():
    context = httpr.create_ssl_context(verify=False)
    assert context.verify_mode == ssl.VerifyMode.CERT_NONE
    assert context.check_hostname is False


def test_SSLContext_with_get_request(https_server, cert_pem_file):
    context = httpr.create_ssl_context()
    context.load_verify_locations(cert_pem_file)
    response = httpr.get(https_server.url, verify=context)
    assert response.status_code == 200


def test_limits_repr():
    limits = httpr.Limits(max_connections=100)
    expected = (
        "Limits(max_connections=100, max_keepalive_connections=None,"
        " keepalive_expiry=5.0)"
    )
    assert repr(limits) == expected


def test_limits_eq():
    limits = httpr.Limits(max_connections=100)
    assert limits == httpr.Limits(max_connections=100)


def test_timeout_eq():
    timeout = httpr.Timeout(timeout=5.0)
    assert timeout == httpr.Timeout(timeout=5.0)


def test_timeout_all_parameters_set():
    timeout = httpr.Timeout(connect=5.0, read=5.0, write=5.0, pool=5.0)
    assert timeout == httpr.Timeout(timeout=5.0)


def test_timeout_from_nothing():
    timeout = httpr.Timeout(None)
    assert timeout.connect is None
    assert timeout.read is None
    assert timeout.write is None
    assert timeout.pool is None


def test_timeout_from_none():
    timeout = httpr.Timeout(timeout=None)
    assert timeout == httpr.Timeout(None)


def test_timeout_from_one_none_value():
    timeout = httpr.Timeout(None, read=None)
    assert timeout == httpr.Timeout(None)


def test_timeout_from_one_value():
    timeout = httpr.Timeout(None, read=5.0)
    assert timeout == httpr.Timeout(timeout=(None, 5.0, None, None))


def test_timeout_from_one_value_and_default():
    timeout = httpr.Timeout(5.0, pool=60.0)
    assert timeout == httpr.Timeout(timeout=(5.0, 5.0, 5.0, 60.0))


def test_timeout_missing_default():
    # In the Rust implementation, Timeout(pool=60.0) is treated as
    # Timeout(None, pool=60.0) since PyO3 can't distinguish missing from explicit None.
    timeout = httpr.Timeout(pool=60.0)
    assert timeout.pool == 60.0


def test_timeout_from_tuple():
    timeout = httpr.Timeout(timeout=(5.0, 5.0, 5.0, 5.0))
    assert timeout == httpr.Timeout(timeout=5.0)


def test_timeout_from_config_instance():
    timeout = httpr.Timeout(timeout=5.0)
    assert httpr.Timeout(timeout) == httpr.Timeout(timeout=5.0)


def test_timeout_repr():
    timeout = httpr.Timeout(timeout=5.0)
    assert repr(timeout) == "Timeout(timeout=5.0)"

    timeout = httpr.Timeout(None, read=5.0)
    assert repr(timeout) == "Timeout(connect=None, read=5.0, write=None, pool=None)"


def test_proxy_from_url():
    proxy = httpr.Proxy("https://example.com")

    assert str(proxy.url) == "https://example.com"
    assert proxy.auth is None
    assert proxy.headers == {}
    assert repr(proxy) == "Proxy('https://example.com')"


def test_proxy_with_auth_from_url():
    proxy = httpr.Proxy("https://username:password@example.com")

    assert str(proxy.url) == "https://example.com"
    assert proxy.auth == ("username", "password")
    assert proxy.headers == {}
    assert repr(proxy) == "Proxy('https://example.com', auth=('username', '********'))"


def test_invalid_proxy_scheme():
    with pytest.raises(ValueError):
        httpr.Proxy("invalid://example.com")
