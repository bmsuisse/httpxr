from __future__ import annotations

import ipaddress
import re
import typing
from urllib.request import getproxies

if typing.TYPE_CHECKING:
    from ._httpr import URL


def get_environment_proxies() -> dict[str, str | None]:
    proxy_info = getproxies()
    mounts: dict[str, str | None] = {}

    for scheme in ("http", "https", "all"):
        if proxy_info.get(scheme):
            hostname = proxy_info[scheme]
            mounts[f"{scheme}://"] = (
                hostname if "://" in hostname else f"http://{hostname}"
            )

    no_proxy_hosts = [host.strip() for host in proxy_info.get("no", "").split(",")]
    for hostname in no_proxy_hosts:
        if hostname == "*":
            return {}
        elif hostname:
            if "://" in hostname:
                mounts[hostname] = None
            elif _is_ip_hostname(hostname, ipaddress.IPv4Address):
                mounts[f"all://{hostname}"] = None
            elif _is_ip_hostname(hostname, ipaddress.IPv6Address):
                mounts[f"all://[{hostname}]"] = None
            elif hostname.lower() == "localhost":
                mounts[f"all://{hostname}"] = None
            else:
                mounts[f"all://*{hostname}"] = None

    return mounts


def _is_ip_hostname(
    hostname: str,
    address_class: type[ipaddress.IPv4Address] | type[ipaddress.IPv6Address],
) -> bool:
    try:
        address_class(hostname.split("/")[0])
    except ValueError:
        return False
    return True


class URLPattern:
    def __init__(self, pattern: str) -> None:
        from ._httpr import URL

        if pattern and ":" not in pattern:
            raise ValueError(
                f"Proxy keys should use proper URL forms rather "
                f"than plain scheme strings. "
                f'Instead of "{pattern}", use "{pattern}://"'
            )

        url = URL(pattern)
        self.pattern = pattern
        self.scheme = "" if url.scheme == "all" else url.scheme
        self.host = "" if url.host == "*" else url.host
        self.port = url.port
        if not url.host or url.host == "*":
            self.host_regex: typing.Pattern[str] | None = None
        elif url.host.startswith("*."):
            domain = re.escape(url.host[2:])
            self.host_regex = re.compile(f"^.+\\.{domain}$")
        elif url.host.startswith("*"):
            domain = re.escape(url.host[1:])
            self.host_regex = re.compile(f"^(.+\\.)?{domain}$")
        else:
            domain = re.escape(url.host)
            self.host_regex = re.compile(f"^{domain}$")

    def matches(self, other: URL) -> bool:
        if self.scheme and self.scheme != other.scheme:
            return False
        if (
            self.host
            and self.host_regex is not None
            and not self.host_regex.match(other.host)
        ):
            return False
        if self.port is not None and self.port != other.port:
            return False
        return True

    @property
    def priority(self) -> tuple[int, int, int]:
        port_priority = 0 if self.port is not None else 1
        host_priority = -len(self.host)
        scheme_priority = -len(self.scheme)
        return (port_priority, host_priority, scheme_priority)

    def __hash__(self) -> int:
        return hash(self.pattern)

    def __lt__(self, other: URLPattern) -> bool:
        return self.priority < other.priority

    def __eq__(self, other: typing.Any) -> bool:
        return isinstance(other, URLPattern) and self.pattern == other.pattern
