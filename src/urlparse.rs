use pyo3::prelude::*;
use pyo3::types::PyDict;

/// RFC3986 URL parsing implemented in Rust.

// Character sets for percent-encoding
fn frag_safe() -> String {
    (0x20u8..0x7Fu8)
        .filter(|&i| !matches!(i, 0x20 | 0x22 | 0x3C | 0x3E | 0x60))
        .map(|i| i as char)
        .collect()
}

fn query_safe() -> String {
    (0x20u8..0x7Fu8)
        .filter(|&i| !matches!(i, 0x20 | 0x22 | 0x23 | 0x3C | 0x3E))
        .map(|i| i as char)
        .collect()
}

pub fn encode_query_smart(query: &str) -> String {
    // quote function already checks for input being safe (including ' and valid %XX)
    // We just need to ensure correct safe set.
    quote(query, &query_safe())
}

pub fn path_safe() -> String {
    (0x20u8..0x7Fu8)
        .filter(|&i| {
            !matches!(
                i,
                0x20 | 0x22 | 0x23 | 0x3C | 0x3E | 0x3F | 0x60 | 0x7B | 0x7D
            )
        })
        .map(|i| i as char)
        .collect()
}

pub fn userinfo_safe() -> String {
    (0x20u8..0x7Fu8)
        .filter(|&i| {
            !matches!(
                i,
                0x20 | 0x22
                    | 0x23
                    | 0x3C
                    | 0x3E
                    | 0x3F
                    | 0x60
                    | 0x7B
                    | 0x7D
                    | 0x2F
                    | 0x3B
                    | 0x3D
                    | 0x40
                    | 0x5B
                    | 0x5C
                    | 0x5D
                    | 0x5E
                    | 0x7C
            )
        })
        .map(|i| i as char)
        .collect()
}

/// Percent-encode a string, preserving existing %XX sequences.
pub fn quote(string: &str, safe: &str) -> String {
    let unreserved: &str = "ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789-._~";
    let mut result = String::new();
    let chars: Vec<char> = string.chars().collect();
    let mut i = 0;
    while i < chars.len() {
        let ch = chars[i];
        if ch == '%' && i + 2 < chars.len() {
            let hex: String = chars[i + 1..i + 3].iter().collect();
            if hex.chars().all(|c| c.is_ascii_hexdigit()) {
                result.push('%');
                result.push(chars[i + 1].to_ascii_uppercase());
                result.push(chars[i + 2].to_ascii_uppercase());
                i += 3;
                continue;
            }
        }
        if unreserved.contains(ch) || safe.contains(ch) {
            result.push(ch);
        } else {
            for byte in ch.to_string().as_bytes() {
                result.push_str(&format!("%{:02X}", byte));
            }
        }
        i += 1;
    }
    result
}

/// Normalize dot segments in a URL path.
#[allow(dead_code)]
fn normalize_path(path: &str) -> String {
    let mut output: Vec<&str> = Vec::new();
    for segment in path.split('/') {
        match segment {
            "." => {}
            ".." => {
                output.pop();
            }
            s => output.push(s),
        }
    }
    let result = output.join("/");
    if path.starts_with('/') && !result.starts_with('/') {
        format!("/{}", result)
    } else {
        result
    }
}

#[pyclass(from_py_object)]
#[derive(Clone, Debug)]
pub struct ParseResult {
    #[pyo3(get, set)]
    pub scheme: String,
    #[pyo3(get, set)]
    pub userinfo: String,
    #[pyo3(get, set)]
    pub host: String,
    #[pyo3(get, set)]
    pub port: Option<u16>,
    #[pyo3(get, set)]
    pub path: String,
    #[pyo3(get, set)]
    pub query: Option<String>,
    #[pyo3(get, set)]
    pub fragment: Option<String>,
}

/// Public Rust-side helper methods
impl ParseResult {
    pub fn to_url_string(&self) -> String {
        let mut url = String::new();
        if !self.scheme.is_empty() {
            url.push_str(&self.scheme);
            if !self.host.is_empty() {
                url.push_str("://");
            } else {
                url.push(':');
            }
        }
        let authority = self.get_authority();
        url.push_str(&authority);
        url.push_str(&self.path);
        if let Some(ref query) = self.query {
            url.push('?');
            url.push_str(query);
        }
        if let Some(ref fragment) = self.fragment {
            url.push('#');
            url.push_str(fragment);
        }
        url
    }

    pub fn get_authority(&self) -> String {
        let mut authority = String::new();
        if !self.userinfo.is_empty() {
            authority.push_str(&self.userinfo);
            authority.push('@');
        }

        let host = if self.host.contains(':') && !self.host.starts_with('[') {
            format!("[{}]", self.host)
        } else {
            self.host.clone()
        };
        authority.push_str(&host);

        if let Some(port) = self.port {
            authority.push(':');
            authority.push_str(&port.to_string());
        }
        authority
    }

    pub fn get_netloc(&self) -> String {
        let host = if self.host.contains(':') && !self.host.starts_with('[') {
            format!("[{}]", self.host)
        } else {
            self.host.clone()
        };
        let mut netloc = host;
        if let Some(port) = self.port {
            netloc.push(':');
            netloc.push_str(&port.to_string());
        }
        netloc
    }

    pub fn copy_with_dict(
        &self,
        kwargs: Option<&std::collections::HashMap<String, Option<String>>>,
    ) -> PyResult<ParseResult> {
        let mut result = self.clone();
        if let Some(kw) = kwargs {
            for (key, value) in kw {
                match key.as_str() {
                    "scheme" => {
                        if let Some(v) = value {
                            result.scheme = v.clone();
                        }
                    }
                    "userinfo" => {
                        if let Some(v) = value {
                            result.userinfo = v.clone();
                        }
                    }
                    "host" => {
                        if let Some(v) = value {
                            result.host = v.clone();
                        }
                    }
                    "port" => {
                        if let Some(v) = value {
                            result.port = v.parse().ok();
                        } else {
                            result.port = None;
                        }
                    }
                    "path" => {
                        if let Some(v) = value {
                            result.path = v.clone();
                        }
                    }
                    "query" => {
                        result.query = value.clone();
                    }
                    "fragment" => {
                        result.fragment = value.clone();
                    }
                    _ => {}
                }
            }
        }
        let url_str = result.to_url_string();
        urlparse_impl(&url_str, None)
    }
}

#[pymethods]
impl ParseResult {
    #[new]
    #[pyo3(signature = (scheme="".to_string(), userinfo="".to_string(), host="".to_string(), port=None, path="".to_string(), query=None, fragment=None))]
    pub fn new(
        scheme: String,
        userinfo: String,
        host: String,
        port: Option<u16>,
        path: String,
        query: Option<String>,
        fragment: Option<String>,
    ) -> Self {
        ParseResult {
            scheme,
            userinfo,
            host,
            port,
            path,
            query,
            fragment,
        }
    }

    #[getter]
    fn authority(&self) -> String {
        self.get_authority()
    }

    #[getter]
    fn netloc(&self) -> String {
        self.get_netloc()
    }

    fn copy_with(
        &self,
        _py: Python<'_>,
        kwargs: Option<&Bound<'_, PyDict>>,
    ) -> PyResult<ParseResult> {
        let mut result = self.clone();
        if let Some(kw) = kwargs {
            for (key, value) in kw.iter() {
                let key_str: String = key.extract()?;
                match key_str.as_str() {
                    "scheme" => {
                        if !value.is_none() {
                            result.scheme = value.extract()?;
                        }
                    }
                    "userinfo" => {
                        if !value.is_none() {
                            result.userinfo = value.extract()?;
                        }
                    }
                    "host" => {
                        if !value.is_none() {
                            result.host = value.extract()?;
                        }
                    }
                    "port" => {
                        if value.is_none() {
                            result.port = None;
                        } else {
                            result.port = Some(value.extract()?);
                        }
                    }
                    "path" => {
                        if !value.is_none() {
                            result.path = value.extract()?;
                        }
                    }
                    "query" => {
                        if value.is_none() {
                            result.query = None;
                        } else {
                            result.query = Some(value.extract()?);
                        }
                    }
                    "fragment" => {
                        if value.is_none() {
                            result.fragment = None;
                        } else {
                            result.fragment = Some(value.extract()?);
                        }
                    }
                    _ => {}
                }
            }
        }
        let url_str = result.to_url_string();
        urlparse_impl(&url_str, None)
    }

    fn __str__(&self) -> String {
        self.to_url_string()
    }

    fn __repr__(&self) -> String {
        format!(
            "ParseResult(scheme={:?}, userinfo={:?}, host={:?}, port={:?}, path={:?}, query={:?}, fragment={:?})",
            self.scheme, self.userinfo, self.host, self.port, self.path, self.query, self.fragment
        )
    }

    fn __eq__(&self, other: &ParseResult) -> bool {
        self.scheme == other.scheme
            && self.userinfo == other.userinfo
            && self.host == other.host
            && self.port == other.port
            && self.path == other.path
            && self.query == other.query
            && self.fragment == other.fragment
    }
}

fn normalize_ipv6_host(host: &str) -> String {
    let host_clean = if host.starts_with('[') && host.ends_with(']') {
        &host[1..host.len() - 1]
    } else {
        host
    };

    // Check for IPv4-mapped IPv6 address (::ffff:x:x or ::ffff:1.2.3.4)
    if host_clean.starts_with("::ffff:") {
        if let Ok(ip) = host_clean.parse::<std::net::Ipv6Addr>() {
            if let Some(ipv4) = ip.to_ipv4() {
                return format!("::ffff:{}", ipv4);
            }
        }
    }
    host_clean.to_string()
}

fn extract_host_from_invalid_url(url: &str) -> &str {
    let after_scheme = if let Some(pos) = url.find("://") {
        &url[pos + 3..]
    } else {
        url
    };
    // Stop at /, ?, #
    let end = after_scheme
        .find(|c| c == '/' || c == '?' || c == '#')
        .unwrap_or(after_scheme.len());
    &after_scheme[..end]
}

fn extract_port_from_invalid_url(url: &str) -> &str {
    let host_port = extract_host_from_invalid_url(url);
    if let Some(pos) = host_port.rfind(':') {
        // Check if colon is part of IPv6 (inside brackets)
        if let Some(close_bracket) = host_port.rfind(']') {
            if pos < close_bracket {
                return "";
            }
        }
        &host_port[pos + 1..]
    } else {
        ""
    }
}

pub fn default_port(scheme: &str) -> Option<u16> {
    match scheme {
        "http" | "ws" => Some(80),
        "https" | "wss" => Some(443),
        "ftp" => Some(21),
        _ => None,
    }
}

fn normalize_port_value(port_str: &str, scheme: &str) -> PyResult<Option<u16>> {
    if port_str.is_empty() {
        return Ok(None);
    }
    match port_str.parse::<u16>() {
        Ok(port) => {
            if default_port(scheme) == Some(port) {
                Ok(None)
            } else {
                Ok(Some(port))
            }
        }
        Err(_) => Err(crate::exceptions::InvalidURL::new_err(format!(
            "Invalid port: '{}'",
            port_str
        ))),
    }
}

#[allow(dead_code)]
fn normalize_port(port: Option<u16>, _scheme: &str) -> Option<u16> {
    port
}

pub fn validate_host(host: &str) -> PyResult<()> {
    if host.is_empty() {
        return Ok(());
    }
    // Skip encoded hosts (e.g. "exam%20le.com") as they fail IDNA ascii check
    if host.contains('%') {
        return Ok(());
    }
    // If IDNA encoding fails, it's invalid. Use strict STD3 rules.
    let (unicode_host, _) = idna::domain_to_unicode(host);
    if idna::domain_to_ascii_strict(&unicode_host).is_err() {
        return Err(crate::exceptions::InvalidURL::new_err(format!(
            "Invalid IDNA host: '{}'",
            host
        )));
    }
    // Check for specific symbol/emoji ranges that httpx rejects but rust-url accepts (UTS 46 mapping)
    // Blacklist: Misc Symbols (2600-26FF), Emoticons (1F600-1F64F), etc.
    if unicode_host.chars().any(
        |c| {
            (c >= '\u{2600}' && c <= '\u{26FF}') || // Misc Symbols (e.g. Snowman)
        (c >= '\u{1F000}' && c <= '\u{1FFFF}')
        }, // Supplementary Private Use Area-A / Symbols / Main Emoji range
    ) {
        return Err(crate::exceptions::InvalidURL::new_err(format!(
            "Invalid IDNA hostname: '{}'",
            unicode_host
        )));
    }
    Ok(())
}

pub fn urlparse_impl(
    url: &str,
    kwargs: Option<&std::collections::HashMap<String, Option<String>>>,
) -> PyResult<ParseResult> {
    if let Some(kw) = kwargs {
        if !kw.is_empty() {
            // ... handling kwargs ...
            let scheme = kw.get("scheme").and_then(|v| v.clone()).unwrap_or_default();
            let userinfo = kw
                .get("userinfo")
                .and_then(|v| v.clone())
                .unwrap_or_default();
            let host = kw.get("host").and_then(|v| v.clone()).unwrap_or_default();
            let port_str = kw.get("port").and_then(|v| v.clone()).unwrap_or_default();
            let path = kw.get("path").and_then(|v| v.clone()).unwrap_or_default();
            let query = kw.get("query").and_then(|v| v.clone());
            let fragment = kw.get("fragment").and_then(|v| v.clone());
            let port = normalize_port_value(&port_str, &scheme)?;

            // Validate host if present
            if !host.is_empty() {
                // If IDNA encoding fails, it's invalid. Use strict STD3 rules.
                let (unicode_host, _) = idna::domain_to_unicode(&host);
                if idna::domain_to_ascii_strict(&unicode_host).is_err() {
                    return Err(crate::exceptions::InvalidURL::new_err(format!(
                        "Invalid IDNA host: '{}'",
                        host
                    )));
                }
            }
            let scheme_lower = scheme.to_lowercase();
            if (scheme_lower == "http" || scheme_lower == "https") && host.is_empty() {
                return Err(crate::exceptions::InvalidURL::new_err(
                    "Invalid URL: empty host",
                ));
            }
            return Ok(ParseResult {
                scheme: scheme_lower,
                userinfo,
                host: host.to_lowercase(),
                port,
                path,
                query,
                fragment,
            });
        }
    }

    if url.len() > 65536 {
        return Err(crate::exceptions::InvalidURL::new_err("URL too long"));
    }

    for (i, c) in url.chars().enumerate() {
        if c != '\t' && c != '\r' && c != '\n' && (c < ' ' || c == '\x7f') {
            return Err(crate::exceptions::InvalidURL::new_err(format!(
                "Invalid non-printable ASCII character in URL, {:?} at position {}.",
                c, i
            )));
        }
    }

    let url_str = url.trim();
    if url_str.is_empty() {
        return Ok(ParseResult {
            scheme: String::new(),
            userinfo: String::new(),
            host: String::new(),
            port: None,
            path: String::new(),
            query: None,
            fragment: None,
        });
    }

    // Use rust-url strict parsing first to catch invalid URLs
    let parsed_url = match url::Url::parse(url_str) {
        Ok(u) => u,
        Err(url::ParseError::RelativeUrlWithoutBase) if url_str.starts_with("://") => {
            // Handle schemeless authority
            let dummy = format!("http{}", url_str);
            let u = url::Url::parse(&dummy)
                .map_err(|e| crate::exceptions::InvalidURL::new_err(e.to_string()))?;
            // We will clear scheme later
            u
        }
        Err(e) => {
            // Retry with encoded spaces (lenient parsing for host/path)
            let mut repaired_url = None;
            if url_str.contains(' ') {
                let repaired = url_str.replace(' ', "%20");
                if let Ok(u) = url::Url::parse(&repaired) {
                    repaired_url = Some(u);
                }
            }

            if let Some(u) = repaired_url {
                u
            } else {
                // Fallthrough to existing error handling
                // Check if it's a relative URL (which rust-url rejects without base)
                if e == url::ParseError::RelativeUrlWithoutBase {
                    // ... existing relative handling ...
                    let (url_no_frag, fragment) = if let Some(pos) = url_str.find('#') {
                        (
                            &url_str[..pos],
                            Some(quote(&url_str[pos + 1..], &frag_safe())),
                        )
                    } else {
                        (url_str, None)
                    };

                    let (url_no_query, query) = if let Some(pos) = url_no_frag.find('?') {
                        (
                            &url_no_frag[..pos],
                            Some(quote(&url_no_frag[pos + 1..], &query_safe())),
                        )
                    } else {
                        (url_no_frag, None)
                    };

                    let normalized_path = if !url_no_query.is_empty() {
                        quote(url_no_query, &path_safe())
                    } else {
                        String::new()
                    };

                    return Ok(ParseResult {
                        scheme: String::new(),
                        userinfo: String::new(),
                        host: String::new(),
                        port: None,
                        path: normalized_path,
                        query,
                        fragment,
                    });
                }

                // Handle IdnaError or InvalidDomainCharacter for encoded hosts (e.g. spaces)
                if matches!(
                    e,
                    url::ParseError::IdnaError | url::ParseError::InvalidDomainCharacter
                ) && (url_str.contains('%') || url_str.contains(' '))
                {
                    // Manual fallback: split scheme://authority/path
                    if let Some((scheme_part, rest)) = url_str.split_once("://") {
                        let scheme = scheme_part.to_string();
                        // Split authority from path
                        let (authority, path_query_frag) = if let Some(slash_pos) = rest.find('/') {
                            (&rest[..slash_pos], &rest[slash_pos..])
                        } else if let Some(q_pos) = rest.find('?') {
                            (&rest[..q_pos], &rest[q_pos..])
                        } else if let Some(h_pos) = rest.find('#') {
                            (&rest[..h_pos], &rest[h_pos..])
                        } else {
                            (rest, "")
                        };

                        // Parse authority manually? Simplified for now: assume no userinfo/port for this specific failure case
                        // or try best effort.
                        // But we just want to return Success with extracted components.
                        // Extract userinfo, host, port from authority
                        let (userinfo, host_port) = if let Some(at_pos) = authority.find('@') {
                            (authority[..at_pos].to_string(), &authority[at_pos + 1..])
                        } else {
                            (String::new(), authority)
                        };

                        let (host, port_opts) = if let Some(colon_pos) = host_port.rfind(':') {
                            // simplistic port check
                            if let Ok(p) = host_port[colon_pos + 1..].parse::<u16>() {
                                (
                                    host_port[..colon_pos].to_string().replace(' ', "%20"),
                                    Some(p),
                                )
                            } else {
                                (host_port.to_string().replace(' ', "%20"), None)
                            }
                        } else {
                            (host_port.to_string().replace(' ', "%20"), None)
                        };

                        // Path/Query/Frag
                        let (path_str, frag_opts) = if let Some(h_pos) = path_query_frag.find('#') {
                            (
                                &path_query_frag[..h_pos],
                                Some(path_query_frag[h_pos + 1..].to_string()),
                            )
                        } else {
                            (path_query_frag, None)
                        };
                        let (path, query_opts) = if let Some(q_pos) = path_str.find('?') {
                            (&path_str[..q_pos], Some(path_str[q_pos + 1..].to_string()))
                        } else {
                            (path_str, None)
                        };

                        return Ok(ParseResult {
                            scheme,
                            userinfo,
                            host,
                            port: port_opts,
                            path: path.to_string(),
                            query: query_opts,
                            fragment: frag_opts,
                        });
                    }
                }

                let msg = match e {
                    url::ParseError::InvalidIpv4Address => format!(
                        "Invalid IPv4 address: '{}'",
                        extract_host_from_invalid_url(url_str)
                    ),
                    url::ParseError::InvalidIpv6Address => format!(
                        "Invalid IPv6 address: '{}'",
                        extract_host_from_invalid_url(url_str)
                    ),
                    url::ParseError::InvalidPort => {
                        format!("Invalid port: '{}'", extract_port_from_invalid_url(url_str))
                    }
                    url::ParseError::EmptyHost => {
                        // httpx allows empty host
                        let scheme = url_str.split("://").next().unwrap_or("").to_string();
                        if !scheme.is_empty() {
                            return Ok(ParseResult {
                                scheme,
                                userinfo: String::new(),
                                host: String::new(),
                                port: None,
                                path: String::new(),
                                query: None,
                                fragment: None,
                            });
                        }
                        format!("Invalid URL: empty host")
                    }
                    _ => format!("Invalid URL: {}", e),
                };
                return Err(crate::exceptions::InvalidURL::new_err(msg));
            }
        }
    };

    // Extract pieces from parsed_url
    let scheme_str = if url_str.starts_with("://") {
        String::new()
    } else {
        parsed_url.scheme().to_string()
    };
    let scheme = scheme_str;
    let userinfo = format!(
        "{}{}",
        parsed_url.username(),
        parsed_url
            .password()
            .map(|p| format!(":{}", p))
            .unwrap_or_default()
    );

    // IDNA validation check (similar to before)
    if let Some(host_str) = parsed_url.host_str() {
        if host_str.contains("xn--") || !host_str.is_ascii() {
            // If IDNA encoding fails, it's invalid. Use strict STD3 rules.
            let (unicode_host, _) = idna::domain_to_unicode(host_str);
            if idna::domain_to_ascii_strict(&unicode_host).is_err() {
                return Err(crate::exceptions::InvalidURL::new_err(format!(
                    "Invalid IDNA host: '{}'",
                    host_str
                )));
            }
            // Check for specific symbol/emoji ranges that httpx rejects but rust-url accepts (UTS 46 mapping)
            // Blacklist: Misc Symbols (2600-26FF), Emoticons (1F600-1F64F), etc.
            if unicode_host.chars().any(
                |c| {
                    (c >= '\u{2600}' && c <= '\u{26FF}') || // Misc Symbols (e.g. Snowman)
                (c >= '\u{1F000}' && c <= '\u{1FFFF}')
                }, // Supplementary Private Use Area-A / Symbols / Main Emoji range
            ) {
                return Err(crate::exceptions::InvalidURL::new_err(format!(
                    "Invalid IDNA hostname: '{}'",
                    unicode_host
                )));
            }
        }
    }

    let host = if let Some(h) = parsed_url.host_str() {
        normalize_ipv6_host(h)
    } else {
        String::new()
    };
    let port = parsed_url.port();
    let path_str = parsed_url.path();
    let path = if path_str == "/" {
        // rust-url always normalizes authority-bearing URLs to have path "/".
        // We need to check if the original URL explicitly had a "/" path.
        // If not, the path should be empty.
        let has_explicit_slash = if url_str.starts_with("://") {
            // schemeless: "://host/..."
            let rest = &url_str[3..];
            rest.find('/').is_some()
        } else if let Some(scheme_end) = url_str.find("://") {
            // scheme://authority/path
            let rest = &url_str[scheme_end + 3..];
            // Find the first '/' after the authority part
            // Check if there's a '/' in the rest (host[:port] part)
            rest.find('/').is_some()
        } else {
            // No scheme, has path
            true
        };
        if has_explicit_slash {
            "/".to_string()
        } else {
            String::new()
        }
    } else {
        path_str.replace("%27", "'")
    };

    // Manual query extraction to preserve encoding (httpx standard compliance)
    let query = if let Some(q_start) = url.find('?') {
        let remainder = &url[q_start + 1..];
        let h_pos_in_remainder = remainder.find('#');

        // Check if ? is actually part of fragment (i.e. if # appears BEFORE ?)
        // url.find('?') gives absolute pos. url.find('#') gives absolute pos.
        let h_pos_abs = url.find('#');
        if let Some(h) = h_pos_abs {
            if h < q_start {
                // ? is inside fragment. No query.
                None
            } else {
                // ? is before #. Extract query.
                let q_end = h_pos_in_remainder.unwrap_or(remainder.len());
                let raw_query = &remainder[..q_end];
                Some(encode_query_smart(raw_query))
            }
        } else {
            // No fragment. Extract query.
            let raw_query = remainder;
            Some(encode_query_smart(raw_query))
        }
    } else {
        None
    };

    let fragment = parsed_url.fragment().map(|s| s.replace("%27", "'"));

    Ok(ParseResult {
        scheme,
        userinfo,
        host,
        port,
        path,
        query,
        fragment,
    })
}

#[pyfunction]
#[pyo3(signature = (url="".to_string(), **kwargs))]
pub fn urlparse(url: String, kwargs: Option<&Bound<'_, PyDict>>) -> PyResult<ParseResult> {
    if let Some(kw) = kwargs {
        if !kw.is_empty() {
            let mut map = std::collections::HashMap::new();
            for (key, value) in kw.iter() {
                let key_str: String = key.extract()?;
                let val: Option<String> = if value.is_none() {
                    None
                } else {
                    Some(value.extract()?)
                };
                map.insert(key_str, val);
            }
            return urlparse_impl(&url, Some(&map));
        }
    }
    urlparse_impl(&url, None)
}

#[pyfunction]
#[pyo3(signature = (string, safe=""))]
pub fn py_quote(string: &str, safe: &str) -> String {
    quote(string, safe)
}

pub fn register(m: &Bound<'_, PyModule>) -> PyResult<()> {
    m.add_class::<ParseResult>()?;
    m.add_function(wrap_pyfunction!(urlparse, m)?)?;
    m.add_function(wrap_pyfunction!(py_quote, m)?)?;
    Ok(())
}
