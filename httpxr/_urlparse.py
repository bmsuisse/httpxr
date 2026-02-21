from __future__ import annotations

import ipaddress
import re
import typing

import idna

from ._httpxr import InvalidURL

MAX_URL_LENGTH = 65536

UNRESERVED_CHARACTERS = "ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789-._~"
SUB_DELIMS = "!$&'()*+,;="

PERCENT_ENCODED_REGEX = re.compile("%[A-Fa-f0-9]{2}")

_ALWAYS_EXCLUDED = (0x20, 0x22, 0x3C, 0x3E)
_PATH_EXCLUDED = _ALWAYS_EXCLUDED + (0x23, 0x3F, 0x60, 0x7B, 0x7D)
_USERINFO_EXTRA = (0x2F, 0x3B, 0x3D, 0x40, 0x5B, 0x5C, 0x5D, 0x5E, 0x7C)
_CREDENTIAL_EXTRA = _USERINFO_EXTRA + (0x3A,)


def _safe_chars(*excluded: int) -> str:
    excluded_set = set(excluded)
    return "".join(chr(i) for i in range(0x20, 0x7F) if i not in excluded_set)


FRAG_SAFE = _safe_chars(*_ALWAYS_EXCLUDED, 0x60)
QUERY_SAFE = _safe_chars(*_ALWAYS_EXCLUDED, 0x23)
PATH_SAFE = _safe_chars(*_PATH_EXCLUDED)
USERNAME_SAFE = _safe_chars(*_PATH_EXCLUDED, *_CREDENTIAL_EXTRA)
PASSWORD_SAFE = USERNAME_SAFE
USERINFO_SAFE = _safe_chars(*_PATH_EXCLUDED, *_USERINFO_EXTRA)

URL_REGEX = re.compile(
    r"(?:(?P<scheme>([a-zA-Z][a-zA-Z0-9+.-]*)?):\)?"
    r"(?://(?P<authority>[^/?#]*))?"
    r"(?P<path>[^?#]*)"
    r"(?:\?(?P<query>[^#]*))?"
    r"(?:#(?P<fragment>.*))?"
)

AUTHORITY_REGEX = re.compile(
    r"(?:(?P<userinfo>.*)@)?(?P<host>(\[.*\]|[^:@]*)):?(?P<port>.*)?"
)

COMPONENT_REGEX = {
    "scheme": re.compile("([a-zA-Z][a-zA-Z0-9+.-]*)?"),
    "authority": re.compile("[^/?#]*"),
    "path": re.compile("[^?#]*"),
    "query": re.compile("[^#]*"),
    "fragment": re.compile(".*"),
    "userinfo": re.compile("[^@]*"),
    "host": re.compile("(\\[.*\\]|[^:]*)"),
    "port": re.compile(".*"),
}

IPv4_STYLE_HOSTNAME = re.compile(r"^[0-9]+\.[0-9]+\.[0-9]+\.[0-9]+$")
IPv6_STYLE_HOSTNAME = re.compile(r"^\[.*\]$")


class ParseResult(typing.NamedTuple):
    scheme: str
    userinfo: str
    host: str
    port: int | None
    path: str
    query: str | None
    fragment: str | None

    @property
    def authority(self) -> str:
        host = f"[{self.host}]" if ":" in self.host else self.host
        return "".join([
            f"{self.userinfo}@" if self.userinfo else "",
            host,
            f":{self.port}" if self.port is not None else "",
        ])

    @property
    def netloc(self) -> str:
        host = f"[{self.host}]" if ":" in self.host else self.host
        return host + (f":{self.port}" if self.port is not None else "")

    def copy_with(self, **kwargs: str | None) -> ParseResult:
        if not kwargs:
            return self
        defaults = {
            "scheme": self.scheme,
            "authority": self.authority,
            "path": self.path,
            "query": self.query,
            "fragment": self.fragment,
        }
        defaults.update(kwargs)
        return urlparse("", **defaults)

    def __str__(self) -> str:
        authority = self.authority
        return "".join([
            f"{self.scheme}:" if self.scheme else "",
            f"//{authority}" if authority else "",
            self.path,
            f"?{self.query}" if self.query is not None else "",
            f"#{self.fragment}" if self.fragment is not None else "",
        ])


def _validate_non_printable(value: str, label: str) -> None:
    if any(char.isascii() and not char.isprintable() for char in value):
        char = next(c for c in value if c.isascii() and not c.isprintable())
        raise InvalidURL(f"Invalid non-printable ASCII character in {label}, {char!r} at position {value.find(char)}.")


def urlparse(url: str = "", **kwargs: str | None) -> ParseResult:
    if len(url) > MAX_URL_LENGTH:
        raise InvalidURL("URL too long")

    _validate_non_printable(url, "URL")

    if "port" in kwargs:
        port = kwargs["port"]
        kwargs["port"] = str(port) if isinstance(port, int) else port

    if "netloc" in kwargs:
        netloc = kwargs.pop("netloc") or ""
        kwargs["host"], _, kwargs["port"] = netloc.partition(":")

    if "username" in kwargs or "password" in kwargs:
        username = quote(kwargs.pop("username", "") or "", safe=USERNAME_SAFE)
        password = quote(kwargs.pop("password", "") or "", safe=PASSWORD_SAFE)
        kwargs["userinfo"] = f"{username}:{password}" if password else username

    if "raw_path" in kwargs:
        raw_path = kwargs.pop("raw_path") or ""
        kwargs["path"], sep, kwargs["query"] = raw_path.partition("?")
        if not sep:
            kwargs["query"] = None

    if "host" in kwargs:
        host = kwargs.get("host") or ""
        if ":" in host and not (host.startswith("[") and host.endswith("]")):
            kwargs["host"] = f"[{host}]"

    for key, value in kwargs.items():
        if value is not None:
            if len(value) > MAX_URL_LENGTH:
                raise InvalidURL(f"URL component '{key}' too long")
            _validate_non_printable(value, f"URL {key} component")
            if not COMPONENT_REGEX[key].fullmatch(value):
                raise InvalidURL(f"Invalid URL component '{key}'")

    url_dict = URL_REGEX.match(url).groupdict()  # type: ignore[union-attr]

    scheme = kwargs.get("scheme", url_dict["scheme"]) or ""
    authority = kwargs.get("authority", url_dict["authority"]) or ""
    path = kwargs.get("path", url_dict["path"]) or ""
    query = kwargs.get("query", url_dict["query"])
    frag = kwargs.get("fragment", url_dict["fragment"])

    authority_dict = AUTHORITY_REGEX.match(authority).groupdict()  # type: ignore[union-attr]

    userinfo = kwargs.get("userinfo", authority_dict["userinfo"]) or ""
    host = kwargs.get("host", authority_dict["host"]) or ""
    port = kwargs.get("port", authority_dict["port"])

    parsed_scheme = scheme.lower()
    parsed_userinfo = quote(userinfo, safe=USERINFO_SAFE)
    parsed_host = encode_host(host)
    parsed_port = normalize_port(port, scheme)

    has_scheme = bool(parsed_scheme)
    has_authority = bool(parsed_userinfo or parsed_host or parsed_port is not None)

    validate_path(path, has_scheme=has_scheme, has_authority=has_authority)
    if has_scheme or has_authority:
        path = normalize_path(path)

    return ParseResult(
        parsed_scheme,
        parsed_userinfo,
        parsed_host,
        parsed_port,
        quote(path, safe=PATH_SAFE),
        None if query is None else quote(query, safe=QUERY_SAFE),
        None if frag is None else quote(frag, safe=FRAG_SAFE),
    )


def encode_host(host: str) -> str:
    if not host:
        return ""

    if IPv4_STYLE_HOSTNAME.match(host):
        try:
            ipaddress.IPv4Address(host)
        except ipaddress.AddressValueError:
            raise InvalidURL(f"Invalid IPv4 address: {host!r}")
        return host

    if IPv6_STYLE_HOSTNAME.match(host):
        try:
            ipaddress.IPv6Address(host[1:-1])
        except ipaddress.AddressValueError:
            raise InvalidURL(f"Invalid IPv6 address: {host!r}")
        return host[1:-1]

    if host.isascii():
        WHATWG_SAFE = '"`{}%|\\'
        return quote(host.lower(), safe=SUB_DELIMS + WHATWG_SAFE)

    try:
        return idna.encode(host.lower()).decode("ascii")
    except idna.IDNAError:
        raise InvalidURL(f"Invalid IDNA hostname: {host!r}")


def normalize_port(port: str | int | None, scheme: str) -> int | None:
    if not port and port != 0:
        return None
    try:
        port_as_int = int(port)  # type: ignore[arg-type]
    except ValueError:
        raise InvalidURL(f"Invalid port: {port!r}")
    default = {"ftp": 21, "http": 80, "https": 443, "ws": 80, "wss": 443}.get(scheme)
    return None if port_as_int == default else port_as_int


def validate_path(path: str, has_scheme: bool, has_authority: bool) -> None:
    if has_authority and path and not path.startswith("/"):
        raise InvalidURL("For absolute URLs, path must be empty or begin with '/'")
    if not has_scheme and not has_authority:
        if path.startswith("//"):
            raise InvalidURL("Relative URLs cannot have a path starting with '//'")
        if path.startswith(":"):
            raise InvalidURL("Relative URLs cannot have a path starting with ':'")


def normalize_path(path: str) -> str:
    if "." not in path:
        return path
    components = path.split("/")
    if "." not in components and ".." not in components:
        return path
    output: list[str] = []
    for component in components:
        if component == "..":
            if output and output != [""]:
                output.pop()
        elif component != ".":
            output.append(component)
    return "/".join(output)


def _percent_encode(string: str) -> str:
    return "".join(f"%{byte:02X}" for byte in string.encode("utf-8"))


def percent_encoded(string: str, safe: str) -> str:
    non_escaped = UNRESERVED_CHARACTERS + safe
    if not string.rstrip(non_escaped):
        return string
    return "".join(c if c in non_escaped else _percent_encode(c) for c in string)


def quote(string: str, safe: str) -> str:
    parts: list[str] = []
    pos = 0
    for match in re.finditer(PERCENT_ENCODED_REGEX, string):
        start, end = match.start(), match.end()
        if start != pos:
            parts.append(percent_encoded(string[pos:start], safe=safe))
        parts.append(match.group(0))
        pos = end
    if pos != len(string):
        parts.append(percent_encoded(string[pos:], safe=safe))
    return "".join(parts)
