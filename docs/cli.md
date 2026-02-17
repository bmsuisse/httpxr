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

## Options

| Option | Short | Description |
|:---|:---|:---|
| `--method` | `-m` | HTTP method (default: GET) |
| `--content` | `-c` | Raw content to send in request body |
| `--json-data` | `-j` | JSON data to send |
| `--verbose` | `-v` | Verbose output |
| `--follow-redirects` | | Follow HTTP redirects |
| `--auth` | | Username and password (two arguments) |
| `--download` | | Download response body to file |

## Output Format

The CLI displays:

1. HTTP status line (`HTTP/1.1 200 OK`)
2. Response headers
3. Response body (JSON is pretty-printed with 4-space indent)

Binary responses show `<N bytes of binary data>` instead of raw bytes.
