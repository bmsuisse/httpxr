from __future__ import annotations

import ipaddress
import re
import typing

import idna

from ._httpr import InvalidURL

MAX_URL_LENGTH = 65536

UNRESERVED_CHARACTERS = (
    "ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789-._~"
)
SUB_DELIMS = "!$&'()*+,;="

PERCENT_ENCODED_REGEX = re.compile("%[A-Fa-f0-9]{2}")

FRAG_SAFE = "".join(
    [chr(i) for i in range(0x20, 0x7F) if i not in (0x20, 0x22, 0x3C, 0x3E, 0x60)]
)

QUERY_SAFE = "".join(
    [chr(i) for i in range(0x20, 0x7F) if i not in (0x20, 0x22, 0x23, 0x3C, 0x3E)]
)

PATH_SAFE = "".join(
    [
        chr(i)
        for i in range(0x20, 0x7F)
        if i not in (0x20, 0x22, 0x23, 0x3C, 0x3E) + (0x3F, 0x60, 0x7B, 0x7D)
    ]
)

USERNAME_SAFE = "".join(
    [
        chr(i)
        for i in range(0x20, 0x7F)
        if i
        not in (0x20, 0x22, 0x23, 0x3C, 0x3E)
        + (0x3F, 0x60, 0x7B, 0x7D)
        + (0x2F, 0x3A, 0x3B, 0x3D, 0x40, 0x5B, 0x5C, 0x5D, 0x5E, 0x7C)
    ]
)
PASSWORD_SAFE = "".join(
    [
        chr(i)
        for i in range(0x20, 0x7F)
        if i
        not in (0x20, 0x22, 0x23, 0x3C, 0x3E)
        + (0x3F, 0x60, 0x7B, 0x7D)
        + (0x2F, 0x3A, 0x3B, 0x3D, 0x40, 0x5B, 0x5C, 0x5D, 0x5E, 0x7C)
    ]
)
USERINFO_SAFE = "".join(
    [
        chr(i)
        for i in range(0x20, 0x7F)
        if i
        not in (0x20, 0x22, 0x23, 0x3C, 0x3E)
        + (0x3F, 0x60, 0x7B, 0x7D)
        + (0x2F, 0x3B, 0x3D, 0x40, 0x5B, 0x5C, 0x5D, 0x5E, 0x7C)
    ]
)


URL_REGEX = re.compile(
    (
        r"(?:(?P<scheme>{scheme}):)?"
        r"(?://(?P<authority>{authority}))?"
        r"(?P<path>{path})"
        r"(?:\?(?P<query>{query}))?"
        r"(?:#(?P<fragment>{fragment}))?"
    ).format(
        scheme="([a-zA-Z][a-zA-Z0-9+.-]*)?",
        authority="[^/?#]*",
        path="[^?#]*",
        query="[^#]*",
        fragment=".*",
    )
)

AUTHORITY_REGEX = re.compile(
    (
        r"(?:(?P<userinfo>{userinfo})@)?" r"(?P<host>{host})" r":?(?P<port>{port})?"
    ).format(
        userinfo=".*",
        host="(\\[.*\\]|[^:@]*)",
        port=".*",
    )
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
        return "".join(
            [
                f"{self.userinfo}@" if self.userinfo else "",
                f"[{self.host}]" if ":" in self.host else self.host,
                f":{self.port}" if self.port is not None else "",
            ]
        )

    @property
    def netloc(self) -> str:
        return "".join(
            [
                f"[{self.host}]" if ":" in self.host else self.host,
                f":{self.port}" if self.port is not None else "",
            ]
        )

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
        return "".join(
            [
                f"{self.scheme}:" if self.scheme else "",
                f"//{authority}" if authority else "",
                self.path,
                f"?{self.query}" if self.query is not None else "",
                f"#{self.fragment}" if self.fragment is not None else "",
            ]
        )


def urlparse(url: str = "", **kwargs: str | None) -> ParseResult:
    if len(url) > MAX_URL_LENGTH:
        raise InvalidURL("URL too long")

    if any(char.isascii() and not char.isprintable() for char in url):
        char = next(char for char in url if char.isascii() and not char.isprintable())
        idx = url.find(char)
        error = (
            f"Invalid non-printable ASCII character in URL, {char!r} at position {idx}."
        )
        raise InvalidURL(error)

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
        kwargs["path"], seperator, kwargs["query"] = raw_path.partition("?")
        if not seperator:
            kwargs["query"] = None

    if "host" in kwargs:
        host = kwargs.get("host") or ""
        if ":" in host and not (host.startswith("[") and host.endswith("]")):
            kwargs["host"] = f"[{host}]"

    for key, value in kwargs.items():
        if value is not None:
            if len(value) > MAX_URL_LENGTH:
                raise InvalidURL(f"URL component '{key}' too long")

            if any(char.isascii() and not char.isprintable() for char in value):
                char = next(
                    char for char in value if char.isascii() and not char.isprintable()
                )
                idx = value.find(char)
                error = (
                    f"Invalid non-printable ASCII character in URL {key} component, "
                    f"{char!r} at position {idx}."
                )
                raise InvalidURL(error)

            if not COMPONENT_REGEX[key].fullmatch(value):
                raise InvalidURL(f"Invalid URL component '{key}'")

    url_match = URL_REGEX.match(url)
    assert url_match is not None
    url_dict = url_match.groupdict()

    scheme = kwargs.get("scheme", url_dict["scheme"]) or ""
    authority = kwargs.get("authority", url_dict["authority"]) or ""
    path = kwargs.get("path", url_dict["path"]) or ""
    query = kwargs.get("query", url_dict["query"])
    frag = kwargs.get("fragment", url_dict["fragment"])

    authority_match = AUTHORITY_REGEX.match(authority)
    assert authority_match is not None
    authority_dict = authority_match.groupdict()

    userinfo = kwargs.get("userinfo", authority_dict["userinfo"]) or ""
    host = kwargs.get("host", authority_dict["host"]) or ""
    port = kwargs.get("port", authority_dict["port"])

    parsed_scheme: str = scheme.lower()
    parsed_userinfo: str = quote(userinfo, safe=USERINFO_SAFE)
    parsed_host: str = encode_host(host)
    parsed_port: int | None = normalize_port(port, scheme)

    has_scheme = parsed_scheme != ""
    has_authority = (
        parsed_userinfo != "" or parsed_host != "" or parsed_port is not None
    )
    validate_path(path, has_scheme=has_scheme, has_authority=has_authority)
    if has_scheme or has_authority:
        path = normalize_path(path)

    parsed_path: str = quote(path, safe=PATH_SAFE)
    parsed_query: str | None = None if query is None else quote(query, safe=QUERY_SAFE)
    parsed_frag: str | None = None if frag is None else quote(frag, safe=FRAG_SAFE)

    return ParseResult(
        parsed_scheme,
        parsed_userinfo,
        parsed_host,
        parsed_port,
        parsed_path,
        parsed_query,
        parsed_frag,
    )


def encode_host(host: str) -> str:
    if not host:
        return ""

    elif IPv4_STYLE_HOSTNAME.match(host):
        try:
            ipaddress.IPv4Address(host)
        except ipaddress.AddressValueError:
            raise InvalidURL(f"Invalid IPv4 address: {host!r}")
        return host

    elif IPv6_STYLE_HOSTNAME.match(host):
        try:
            ipaddress.IPv6Address(host[1:-1])
        except ipaddress.AddressValueError:
            raise InvalidURL(f"Invalid IPv6 address: {host!r}")
        return host[1:-1]

    elif host.isascii():
        WHATWG_SAFE = '"`{}%|\\'
        return quote(host.lower(), safe=SUB_DELIMS + WHATWG_SAFE)

    try:
        return idna.encode(host.lower()).decode("ascii")
    except idna.IDNAError:
        raise InvalidURL(f"Invalid IDNA hostname: {host!r}")


def normalize_port(port: str | int | None, scheme: str) -> int | None:
    if port is None or port == "":
        return None

    try:
        port_as_int = int(port)
    except ValueError:
        raise InvalidURL(f"Invalid port: {port!r}")

    default_port = {"ftp": 21, "http": 80, "https": 443, "ws": 80, "wss": 443}.get(
        scheme
    )
    if port_as_int == default_port:
        return None
    return port_as_int


def validate_path(path: str, has_scheme: bool, has_authority: bool) -> None:
    if has_authority:
        if path and not path.startswith("/"):
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
        if component == ".":
            pass
        elif component == "..":
            if output and output != [""]:
                output.pop()
        else:
            output.append(component)
    return "/".join(output)


def PERCENT(string: str) -> str:
    return "".join([f"%{byte:02X}" for byte in string.encode("utf-8")])


def percent_encoded(string: str, safe: str) -> str:
    NON_ESCAPED_CHARS = UNRESERVED_CHARACTERS + safe

    if not string.rstrip(NON_ESCAPED_CHARS):
        return string

    return "".join(
        [char if char in NON_ESCAPED_CHARS else PERCENT(char) for char in string]
    )


def quote(string: str, safe: str) -> str:
    parts: list[str] = []
    current_position = 0
    for match in re.finditer(PERCENT_ENCODED_REGEX, string):
        start_position, end_position = match.start(), match.end()
        matched_text = match.group(0)
        if start_position != current_position:
            leading_text = string[current_position:start_position]
            parts.append(percent_encoded(leading_text, safe=safe))

        parts.append(matched_text)
        current_position = end_position

    if current_position != len(string):
        trailing_text = string[current_position:]
        parts.append(percent_encoded(trailing_text, safe=safe))

    return "".join(parts)
