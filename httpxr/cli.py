from __future__ import annotations

import json
import sys
import time
import typing

import click

# ---------------------------------------------------------------------------
# Rich output helpers (graceful fallback when rich is not installed)
# ---------------------------------------------------------------------------

try:
    from rich.console import Console
    from rich.syntax import Syntax
    from rich.text import Text

    HAS_RICH = True
except ImportError:  # pragma: no cover
    HAS_RICH = False


def _status_color(status_code: int) -> str:
    """Return a rich color name based on HTTP status category."""
    if status_code < 200:
        return "cyan"
    elif status_code < 300:
        return "green"
    elif status_code < 400:
        return "yellow"
    elif status_code < 500:
        return "red"
    else:
        return "bold red"


def is_binary_content(content: bytes) -> bool:
    return b"\0" in content


def is_binary_content_type(content_type: str) -> bool:
    text_types = (
        "text/",
        "application/json",
        "application/xml",
        "application/javascript",
        "application/ecmascript",
    )
    ct = content_type.lower().split(";")[0].strip()
    return not any(ct.startswith(t) for t in text_types) and ct != ""


# ---------------------------------------------------------------------------
# Plain-text formatter (used with --no-color or when rich is missing)
# ---------------------------------------------------------------------------


def format_response_plain(
    response: typing.Any,
    verbose: bool = False,
) -> str:
    http_version = getattr(response, "http_version", "HTTP/1.1")
    status_code = response.status_code
    reason = getattr(response, "reason_phrase", "")

    status_line = f"{http_version} {status_code} {reason}".rstrip()
    lines: list[str] = [status_line]

    headers = response.headers
    for key, value in headers.items():
        lines.append(f"{key}: {value}")

    lines.append("")

    content = response.content
    if content:
        text = getattr(response, "text", None)
        content_type = headers.get("content-type", "")

        if is_binary_content_type(content_type) or is_binary_content(content):
            lines.append(f"<{len(content)} bytes of binary data>")
        elif "application/json" in content_type and text:
            try:
                data = json.loads(text)
                formatted = json.dumps(data, indent=4, ensure_ascii=False)
                lines.append(formatted)
            except (json.JSONDecodeError, TypeError):
                lines.append(text or "")
        elif text:
            lines.append(text)

    return "\n".join(lines)


# ---------------------------------------------------------------------------
# Rich formatter
# ---------------------------------------------------------------------------


def print_response_rich(
    console: Console,
    response: typing.Any,
    verbose: bool = False,
) -> None:
    """Pretty-print a response using rich."""
    http_version = getattr(response, "http_version", "HTTP/1.1")
    status_code = response.status_code
    reason = getattr(response, "reason_phrase", "")
    color = _status_color(status_code)

    # Status line
    status_line = Text()
    status_line.append(f"{http_version} ", style="bold dim")
    status_line.append(f"{status_code}", style=f"bold {color}")
    if reason:
        status_line.append(f" {reason}", style=color)
    console.print(status_line)

    # Headers
    headers = response.headers
    for key, value in headers.items():
        header_text = Text()
        header_text.append(f"{key}", style="dim cyan")
        header_text.append(": ", style="dim")
        header_text.append(value)
        console.print(header_text)

    console.print()

    # Body
    content = response.content
    if content:
        text = getattr(response, "text", None)
        content_type = headers.get("content-type", "")

        if is_binary_content_type(content_type) or is_binary_content(content):
            console.print(
                f"[dim]<{len(content)} bytes of binary data>[/dim]"
            )
        elif "application/json" in content_type and text:
            try:
                data = json.loads(text)
                formatted = json.dumps(data, indent=4, ensure_ascii=False)
                syntax = Syntax(formatted, "json", theme="monokai")
                console.print(syntax)
            except (json.JSONDecodeError, TypeError):
                console.print(text or "")
        elif text:
            console.print(text)


# ---------------------------------------------------------------------------
# Header parsing helper (curl-style -H "Key: Value")
# ---------------------------------------------------------------------------


def parse_header(header: str) -> tuple[str, str]:
    """Parse a 'Key: Value' header string."""
    if ":" not in header:
        raise click.BadParameter(
            f"Invalid header format: '{header}'. Expected 'Key: Value'."
        )
    key, _, value = header.partition(":")
    return key.strip(), value.strip()


# ---------------------------------------------------------------------------
# CLI command
# ---------------------------------------------------------------------------


@click.command(help="A next generation HTTP client. ⚡")
@click.argument("url")
@click.option("-m", "--method", default="GET", help="HTTP method.")
@click.option(
    "-c", "--content", default=None, help="Content to send in the request body."
)
@click.option(
    "-j", "--json-data", "json_body", default=None, help="JSON data to send."
)
@click.option("-v", "--verbose", is_flag=True, default=False, help="Verbose output.")
@click.option(
    "--follow-redirects", is_flag=True, default=False, help="Follow redirects."
)
@click.option("--auth", nargs=2, default=None, help="Username and password.", type=str)
@click.option("--download", default=None, help="Download to file.")
@click.option(
    "-H",
    "--header",
    "headers",
    multiple=True,
    help='Add a header, e.g. -H "Authorization: Bearer token".',
)
@click.option(
    "--timing", is_flag=True, default=False, help="Show request timing breakdown."
)
@click.option(
    "--no-color", is_flag=True, default=False, help="Disable colored output."
)
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
                kwargs["content"] = content.encode("utf-8")

            if auth is not None:
                kwargs["auth"] = auth

            # Parse -H headers
            if headers:
                header_dict: dict[str, str] = {}
                for h in headers:
                    key, value = parse_header(h)
                    header_dict[key] = value
                kwargs["headers"] = header_dict

            start_time = time.monotonic()
            response = client.request(**kwargs)
            elapsed_ms = (time.monotonic() - start_time) * 1000

            if download is not None:
                with open(download, "wb") as f:
                    f.write(response.content)

                if use_rich:
                    console = Console()
                    size = len(response.content)
                    console.print(
                        f"[green]✓[/green] Downloaded [bold]{size:,}[/bold] bytes "
                        f"to [cyan]{download}[/cyan]"
                    )
                return

            if use_rich:
                console = Console()

                # Print redirect history
                for hist_resp in getattr(response, "history", []):
                    print_response_rich(console, hist_resp, verbose=verbose)
                    console.print()

                # Print main response
                print_response_rich(console, response, verbose=verbose)

                # Timing
                if timing:
                    elapsed = getattr(response, "elapsed", None)
                    if elapsed is not None:
                        server_ms = elapsed.total_seconds() * 1000
                        console.print()
                        console.print(
                            f"[dim]⏱  Server: {server_ms:.1f}ms  "
                            f"Total: {elapsed_ms:.1f}ms[/dim]"
                        )
                    else:
                        console.print()
                        console.print(
                            f"[dim]⏱  Total: {elapsed_ms:.1f}ms[/dim]"
                        )
            else:
                # Plain text output
                for hist_resp in getattr(response, "history", []):
                    click.echo(format_response_plain(hist_resp, verbose=verbose))
                    click.echo()

                click.echo(format_response_plain(response, verbose=verbose))

                if timing:
                    click.echo()
                    click.echo(f"Total: {elapsed_ms:.1f}ms")

            if response.status_code >= 300:
                sys.exit(1)

    except _httpxr_mod.HTTPError as exc:  # type: ignore[attr-defined]
        if use_rich:
            console = Console(stderr=True)
            console.print(f"[bold red]{type(exc).__name__}[/bold red]: {exc}")
        else:
            click.echo(f"{type(exc).__name__}: {exc}", err=False)
        sys.exit(1)
