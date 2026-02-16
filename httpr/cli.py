from __future__ import annotations

import json
import sys
import typing

import click


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


def format_response(
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


@click.command(help="A next generation HTTP client.")
@click.argument("url")
@click.option("-m", "--method", default="GET", help="HTTP method.")
@click.option("-c", "--content", default=None, help="Content to send in the request body.")
@click.option("-j", "--json-data", "json_body", default=None, help="JSON data to send.")
@click.option("-v", "--verbose", is_flag=True, default=False, help="Verbose output.")
@click.option("--follow-redirects", is_flag=True, default=False, help="Follow redirects.")
@click.option("--auth", nargs=2, default=None, help="Username and password.", type=str)
@click.option("--download", default=None, help="Download to file.")
def main(
    url: str,
    method: str,
    content: str | None,
    json_body: str | None,
    verbose: bool,
    follow_redirects: bool,
    auth: tuple[str, str] | None,
    download: str | None,
) -> None:
    import httpr as _httpr_mod

    try:
        with _httpr_mod.Client() as client:  # type: ignore[attr-defined]
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

            response = client.request(**kwargs)

            if download is not None:
                with open(download, "wb") as f:
                    f.write(response.content)
                return

            for hist_resp in getattr(response, "history", []):
                click.echo(format_response(hist_resp, verbose=verbose))
                click.echo()

            click.echo(format_response(response, verbose=verbose))

            if response.status_code >= 300:
                sys.exit(1)

    except _httpr_mod.HTTPError as exc:  # type: ignore[attr-defined]
        click.echo(f"{type(exc).__name__}: {exc}", err=False)
        sys.exit(1)
