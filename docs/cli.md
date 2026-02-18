# CLI

httpxr includes an optional command-line HTTP client, similar to `httpx` and `curl`.

## Installation

The CLI requires the `cli` extra:

```bash
pip install "httpxr[cli]"
```

## Usage

```bash
httpxr [OPTIONS] URL
```

### Basic Examples

```bash
# GET request
httpxr https://httpbin.org/get

# POST with JSON
httpxr -m POST -j '{"name": "Alice"}' https://httpbin.org/post

# POST with raw content
httpxr -m POST -c "Hello!" https://httpbin.org/post

# Follow redirects
httpxr --follow-redirects https://httpbin.org/redirect/3

# Basic auth
httpxr --auth admin secret https://httpbin.org/basic-auth/admin/secret

# Download to file
httpxr --download output.json https://httpbin.org/get
```

### Custom Headers

Use `-H` (repeatable) to add headers, curl-style:

```bash
# Bearer token
httpxr -H "Authorization: Bearer mytoken" https://api.example.com/data

# Multiple headers
httpxr -H "Authorization: Bearer mytoken" \
       -H "Accept: application/json" \
       -H "X-Request-Id: abc123" \
       https://api.example.com/data
```

### Timing

Use `--timing` to see request duration:

```bash
httpxr --timing https://httpbin.org/get
```

```
HTTP/1.1 200 OK
content-type: application/json
...

{ ... }

⏱  Server: 42.3ms  Total: 45.1ms
```

## Options

| Option | Short | Description |
|:---|:---|:---|
| `--method` | `-m` | HTTP method (default: GET) |
| `--content` | `-c` | Raw content to send in request body |
| `--json-data` | `-j` | JSON data to send |
| `--header` | `-H` | Add a header (repeatable) |
| `--verbose` | `-v` | Verbose output |
| `--follow-redirects` | | Follow HTTP redirects |
| `--auth` | | Username and password (two arguments) |
| `--download` | | Download response body to file |
| `--timing` | | Show request timing breakdown |
| `--no-color` | | Disable colored output |

## Rich Output

When installed with the `cli` extra, httpxr uses [Rich](https://rich.readthedocs.io/)
for colorized, syntax-highlighted output:

- **Status codes** are color-coded: :green_circle: 2xx, :yellow_circle: 3xx, :red_circle: 4xx/5xx
- **JSON responses** are syntax-highlighted with the Monokai theme
- **Headers** use dimmed key names for visual hierarchy
- **Binary responses** show `<N bytes of binary data>`

!!! tip "Piping and CI"
    Color is automatically disabled when piping to another command or when
    `stdout` is not a terminal. Use `--no-color` to force plain text output.

## Output Format

The CLI displays:

1. HTTP status line (`HTTP/1.1 200 OK`)
2. Response headers
3. Response body (JSON is pretty-printed with 4-space indent)

Binary responses show `<N bytes of binary data>` instead of raw bytes.
