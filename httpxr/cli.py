from __future__ import annotations

import json
import sys
import time
import typing

import click

try:
    from rich.console import Console  # type: ignore[import-not-found]
    from rich.syntax import Syntax  # type: ignore[import-not-found]
    from rich.text import Text  # type: ignore[import-not-found]

    HAS_RICH = True
except ImportError:  # pragma: no cover
    HAS_RICH = False


def _status_color(status_code: int) -> str:
    if status_code < 200:
        return "cyan"
    if status_code < 300:
        return "green"
    if status_code < 400:
        return "yellow"
    if status_code < 500:
        return "red"
    return "bold red"


def _is_binary(content: bytes, content_type: str) -> bool:
    text_types = ("text/", "application/json", "application/xml", "application/javascript", "application/ecmascript")
    ct = content_type.lower().split(";")[0].strip()
    return (bool(ct) and not any(ct.startswith(t) for t in text_types)) or b"\0" in content


def _format_json(text: str) -> str | None:
    try:
        return json.dumps(json.loads(text), indent=4, ensure_ascii=False)
    except (json.JSONDecodeError, TypeError):
        return None


def parse_header(header: str) -> tuple[str, str]:
    if ":" not in header:
        raise click.BadParameter(f"Invalid header format: '{header}'. Expected 'Key: Value'.")
    key, _, value = header.partition(":")
    return key.strip(), value.strip()


def format_response_plain(response: typing.Any) -> str:
    http_version = getattr(response, "http_version", "HTTP/1.1")
    reason = getattr(response, "reason_phrase", "")
    lines: list[str] = [f"{http_version} {response.status_code} {reason}".rstrip()]

    headers = response.headers
    for key, value in headers.items():
        lines.append(f"{key}: {value}")
    lines.append("")

    content = response.content
    if content:
        content_type = headers.get("content-type", "")
        text = getattr(response, "text", None)
        if _is_binary(content, content_type):
            lines.append(f"<{len(content)} bytes of binary data>")
        elif "application/json" in content_type and text:
            lines.append(_format_json(text) or text)
        elif text:
            lines.append(text)

    return "\n".join(lines)


def print_response_rich(console: Console, response: typing.Any) -> None:
    http_version = getattr(response, "http_version", "HTTP/1.1")
    reason = getattr(response, "reason_phrase", "")
    color = _status_color(response.status_code)

    status = Text()  # type: ignore[possibly-undefined]
    status.append(f"{http_version} ", style="bold dim")
    status.append(str(response.status_code), style=f"bold {color}")
    if reason:
        status.append(f" {reason}", style=color)
    console.print(status)

    headers = response.headers
    for key, value in headers.items():
        t = Text()  # type: ignore[possibly-undefined]
        t.append(key, style="dim cyan")
        t.append(": ", style="dim")
        t.append(value)
        console.print(t)
    console.print()

    content = response.content
    if content:
        content_type = headers.get("content-type", "")
        text = getattr(response, "text", None)
        if _is_binary(content, content_type):
            console.print(f"[dim]<{len(content)} bytes of binary data>[/dim]")
        elif "application/json" in content_type and text:
            formatted = _format_json(text)
            if formatted:
                console.print(Syntax(formatted, "json", theme="monokai"))  # type: ignore[possibly-undefined]
            else:
                console.print(text)
        elif text:
            console.print(text)


@click.command(help="A next generation HTTP client. ⚡")
@click.argument("url")
@click.option("-m", "--method", default="GET", help="HTTP method.")
@click.option("-c", "--content", default=None, help="Content to send in the request body.")
@click.option("-j", "--json-data", "json_body", default=None, help="JSON data to send.")
@click.option("-v", "--verbose", is_flag=True, default=False, help="Verbose output.")
@click.option("--follow-redirects", is_flag=True, default=False, help="Follow redirects.")
@click.option("--auth", nargs=2, default=None, help="Username and password.", type=str)
@click.option("--download", default=None, help="Download to file.")
@click.option("-H", "--header", "headers", multiple=True, help='Add a header, e.g. -H "Authorization: Bearer token".')
@click.option("--timing", is_flag=True, default=False, help="Show request timing breakdown.")
@click.option("--no-color", is_flag=True, default=False, help="Disable colored output.")
def main(
    url: str,
    method: str,
    content: str | None,
    json_body: str | None,
    verbose: bool,
    follow_redirects: bool,
    auth: tuple[str, str] | None,
    download: str | None,
    headers: tuple[str, ...],
    timing: bool,
    no_color: bool,
) -> None:
    import httpxr as _httpxr_mod

    use_rich = HAS_RICH and not no_color and sys.stdout.isatty()

    try:
        with _httpxr_mod.Client() as client:  # type: ignore[attr-defined]
            kwargs: dict[str, typing.Any] = {
                "method": method,
                "url": url,
                "follow_redirects": follow_redirects,
            }
            if json_body is not None:
                kwargs["json"] = json.loads(json_body)
            if content is not None:
                kwargs["content"] = content.encode()
            if auth is not None:
                kwargs["auth"] = auth
            if headers:
                kwargs["headers"] = dict(parse_header(h) for h in headers)

            t0 = time.monotonic()
            response = client.request(**kwargs)
            elapsed_ms = (time.monotonic() - t0) * 1000

            if download is not None:
                with open(download, "wb") as f:
                    f.write(response.content)
                if use_rich:
                    console = Console()  # type: ignore[possibly-undefined]
                    console.print(f"[green]✓[/green] Downloaded [bold]{len(response.content):,}[/bold] bytes to [cyan]{download}[/cyan]")
                return

            if use_rich:
                console = Console()  # type: ignore[possibly-undefined]
                for hist in getattr(response, "history", []):
                    print_response_rich(console, hist)
                    console.print()
                print_response_rich(console, response)
                if timing:
                    elapsed = getattr(response, "elapsed", None)
                    server = f"Server: {elapsed.total_seconds() * 1000:.1f}ms  " if elapsed else ""
                    console.print()
                    console.print(f"[dim]⏱  {server}Total: {elapsed_ms:.1f}ms[/dim]")
            else:
                for hist in getattr(response, "history", []):
                    click.echo(format_response_plain(hist))
                    click.echo()
                click.echo(format_response_plain(response))
                if timing:
                    click.echo()
                    click.echo(f"Total: {elapsed_ms:.1f}ms")

            if response.status_code >= 300:
                sys.exit(1)

    except _httpxr_mod.HTTPError as exc:  # type: ignore[attr-defined]
        if use_rich:
            Console(stderr=True).print(f"[bold red]{type(exc).__name__}[/bold red]: {exc}")  # type: ignore[possibly-undefined]
        else:
            click.echo(f"{type(exc).__name__}: {exc}", err=False)
        sys.exit(1)
