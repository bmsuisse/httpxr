use pyo3::prelude::*;
use pyo3::types::{PyAnyMethods, PyBytes, PyDict, PyDictMethods};

use crate::config::Limits;
use crate::models::{Cookies, Headers, Request, Response};
use crate::transports::default::HTTPTransport;
use crate::urls::URL;

pub const DEFAULT_MAX_REDIRECTS: u32 = 20;

/// Sentinel for "use client default" values.
#[derive(FromPyObject)]
pub enum AuthArg {
    Boolean(#[allow(dead_code)] bool),
    #[pyo3(transparent)]
    Custom(Py<PyAny>),
}

#[pyclass(from_py_object)]
#[derive(Clone)]
pub struct UseClientDefault;

#[pymethods]
impl UseClientDefault {
    #[new]
    fn new() -> Self {
        UseClientDefault
    }
    fn __repr__(&self) -> &str {
        "<UseClientDefault>"
    }
    fn __bool__(&self) -> bool {
        false
    }
}

pub fn extract_verify_path(v_bound: &Bound<'_, PyAny>) -> (bool, Option<String>) {
    if let Ok(b) = v_bound.extract::<bool>() {
        (b, None)
    } else if let Ok(s) = v_bound.extract::<String>() {
        (true, Some(s))
    } else {
        let cafile = v_bound
            .getattr("_cafile")
            .ok()
            .and_then(|attr| attr.extract::<String>().ok());
        (true, cafile)
    }
}

pub fn extract_from_kwargs<'py>(
    kwargs: Option<&Bound<'py, PyDict>>,
    key: &str,
) -> Option<Bound<'py, PyAny>> {
    kwargs.and_then(|d| d.get_item(key).ok().flatten())
}

/// Merge a base URL with a request URL, matching httpx's `_merge_url` semantics.
pub fn merge_base_url(base: &URL, url: &str) -> PyResult<URL> {
    if url.contains("://") {
        return URL::create_from_str(url);
    }

    let base_scheme = base.get_scheme();
    let base_host = base.get_host();
    let base_port = base.get_port();
    let base_path = base.parsed.path.clone();

    let merged_path = if url.starts_with('/') {
        let bp = base_path.trim_end_matches('/');
        format!("{}{}", bp, url)
    } else if url.starts_with("..") {
        // Treat base path as a directory by ensuring trailing slash
        let mut dir_base = base.clone();
        if !dir_base.parsed.path.ends_with('/') {
            dir_base.parsed.path = format!("{}/", dir_base.parsed.path);
        }
        return dir_base.join_relative(url);
    } else {
        let bp = if base_path.ends_with('/') {
            base_path.clone()
        } else {
            format!("{}/", base_path)
        };
        format!("{}{}", bp, url)
    };

    let port_str = if let Some(p) = base_port {
        let default_port = match &*base_scheme {
            "http" | "ws" => Some(80),
            "https" | "wss" => Some(443),
            _ => None,
        };
        if Some(p) != default_port {
            format!(":{}", p)
        } else {
            String::new()
        }
    } else {
        String::new()
    };

    let full_url = format!("{}://{}{}{}", base_scheme, base_host, port_str, merged_path);
    URL::create_from_str(&full_url)
}

/// Build the host header value from a URL.
pub fn build_host_header(url: &URL) -> String {
    let host = url.get_host();
    let port = url.get_port();
    let scheme = url.get_scheme();
    let default_port = match &*scheme {
        "http" | "ws" => Some(80),
        "https" | "wss" => Some(443),
        _ => None,
    };
    if let Some(p) = port {
        if Some(p) != default_port {
            format!("{}:{}", host, p)
        } else {
            host
        }
    } else {
        host
    }
}

/// Build body from content/json/files/data arguments.
pub fn build_request_body<'py>(
    py: Python<'py>,
    content: Option<&Bound<'py, PyAny>>,
    data: Option<&Bound<'py, PyAny>>,
    files: Option<&Bound<'py, PyAny>>,
    json: Option<&Bound<'py, PyAny>>,
    merged_headers: &mut Headers,
) -> PyResult<Option<Vec<u8>>> {
    if let Some(c) = content {
        if let Ok(b) = c.cast::<PyBytes>() {
            return Ok(Some(b.as_bytes().to_vec()));
        } else if let Ok(s) = c.extract::<String>() {
            return Ok(Some(s.into_bytes()));
        }
        // Check for async iterators — sync client cannot handle them
        let has_aiter = c.hasattr("__aiter__").unwrap_or(false);
        if has_aiter {
            return Err(pyo3::exceptions::PyRuntimeError::new_err(
                "Attempted to send an async request body with a sync Client instance.",
            ));
        }
        return Ok(None);
    }
    if let Some(j) = json {
        let json_mod = py.import("json")?;
        let s: String = json_mod.call_method1("dumps", (j,))?.extract()?;
        merged_headers.set_header("content-type", "application/json");
        return Ok(Some(s.into_bytes()));
    }
    if let Some(f) = files {
        let boundary = if let Some(ct) = merged_headers.get("content-type", None) {
            if let Some(idx) = ct.find("boundary=") {
                let remainder = &ct[idx + 9..];
                if let Some(end) = remainder.find(';') {
                    Some(remainder[..end].trim().trim_matches('"').to_string())
                } else {
                    Some(remainder.trim().trim_matches('"').to_string())
                }
            } else {
                None
            }
        } else {
            None
        };

        let multipart =
            crate::multipart::MultipartStream::new(py, data, Some(f), boundary.as_deref())?;
        if !merged_headers.contains_header("content-type") {
            merged_headers.set_header("content-type", &multipart.content_type());
        }
        return Ok(Some(multipart.get_content()));
    }
    if let Some(d) = data {
        let urllib = py.import("urllib.parse")?;
        let s: String = urllib.call_method1("urlencode", (d, true))?.extract()?;
        merged_headers.set_header("content-type", "application/x-www-form-urlencoded");
        return Ok(Some(s.into_bytes()));
    }
    Ok(None)
}

/// Arrange headers in httpx canonical order: Host, Accept, Accept-Encoding, Connection, User-Agent, Content-*, then remaining.
pub fn arrange_headers_httpx_order(merged_headers: &Headers, target_url: &URL) -> Headers {
    let mut final_headers = Headers::new_empty();

    // 1. Host (always add, it's URL-dependent)
    if merged_headers.contains_header("host") {
        if let Some(v) = merged_headers.get_first_value("host") {
            final_headers.set_header("Host", &v);
        }
    } else {
        final_headers.set_header("Host", &build_host_header(target_url));
    }

    // 2. Accept (only if present in merged headers)
    if let Some(v) = merged_headers.get_first_value("accept") {
        final_headers.set_header("Accept", &v);
    }

    // 3. Accept-Encoding (only if present)
    if let Some(v) = merged_headers.get_first_value("accept-encoding") {
        final_headers.set_header("Accept-Encoding", &v);
    }

    // 4. Connection (only if present)
    if let Some(v) = merged_headers.get_first_value("connection") {
        final_headers.set_header("Connection", &v);
    }

    // 5. User-Agent (only if present)
    if let Some(v) = merged_headers.get_first_value("user-agent") {
        final_headers.set_header("User-Agent", &v);
    }

    // 6. Content-Type and Content-Length (if present)
    if let Some(v) = merged_headers.get_first_value("content-type") {
        final_headers.set_header("Content-Type", &v);
    }
    if let Some(v) = merged_headers.get_first_value("content-length") {
        final_headers.set_header("Content-Length", &v);
    }

    // 7. Remaining user headers
    for (name, value) in merged_headers.iter_all() {
        let name_lower = name.to_lowercase();
        if ![
            "host",
            "accept",
            "accept-encoding",
            "connection",
            "user-agent",
            "content-type",
            "content-length",
        ]
        .contains(&name_lower.as_str())
        {
            final_headers.append_header(&name, &value);
        }
    }

    final_headers
}

/// Apply URL userinfo auth to headers if present.
pub fn apply_url_auth(url: &URL, headers: &mut Headers) {
    let url_string = url.to_string();
    if let Ok(parsed) = url::Url::parse(&url_string) {
        if !parsed.username().is_empty() && !headers.contains_header("authorization") {
            let user = parsed.username().to_string();
            let pass = parsed.password().unwrap_or("").to_string();
            let creds = format!("{}:{}", user, pass);
            let b64 = {
                use base64::Engine;
                base64::engine::general_purpose::STANDARD.encode(creds.as_bytes())
            };
            headers.set_header("Authorization", &format!("Basic {}", b64));
        }
    }
}

/// Apply auth (tuple or callable) to a request.
/// Result of auth preparation: either auth was applied directly, or we need to run a flow.
pub enum AuthResult {
    /// Auth was applied directly (tuple/callable), no flow needed
    Applied,
    /// Auth is an Auth instance, here's the sync_auth_flow generator
    Flow(Py<PyAny>),
    /// No auth to apply
    None,
}

/// Validate that an auth argument is a valid type (tuple, callable, or Auth instance).
/// Raises TypeError if not valid.
pub fn validate_auth_type(_py: Python<'_>, auth: &Bound<'_, PyAny>) -> PyResult<()> {
    if auth.is_none() {
        return Ok(());
    }
    // tuple is fine
    if auth.cast::<pyo3::types::PyTuple>().is_ok() {
        return Ok(());
    }
    // Auth instance (has auth_flow or sync_auth_flow)
    if auth.hasattr("auth_flow")? {
        return Ok(());
    }
    if auth.hasattr("sync_auth_flow")? {
        return Ok(());
    }
    // callable is fine
    if auth.is_callable() {
        return Ok(());
    }
    let type_name = auth
        .get_type()
        .name()
        .map(|n| n.to_string())
        .unwrap_or_else(|_| "unknown".to_string());
    Err(pyo3::exceptions::PyTypeError::new_err(format!(
        "Invalid \"auth\" argument: {}",
        type_name
    )))
}

/// Coerce an auth value (converting tuples to BasicAuth).
/// Returns the coerced value as a Py<PyAny>.
pub fn coerce_auth(py: Python<'_>, auth: &Bound<'_, PyAny>) -> PyResult<Py<PyAny>> {
    if auth.is_none() {
        return Ok(py.None());
    }
    // Convert tuples to BasicAuth
    if let Ok(tuple) = auth.cast::<pyo3::types::PyTuple>() {
        if tuple.len() == 2 {
            let user: String = tuple.get_item(0)?.extract()?;
            let pass: String = tuple.get_item(1)?.extract()?;
            let ba = crate::auth::BasicAuth {
                username: user,
                password: pass,
            };
            let obj = Py::new(py, (ba, crate::auth::Auth))?;
            return Ok(obj.into_any());
        }
    }
    Ok(auth.clone().unbind())
}

pub fn apply_auth<'py>(
    py: Python<'py>,
    auth: Option<&Bound<'py, PyAny>>,
    client_auth: Option<&Py<PyAny>>,
    request: &mut Request,
    auth_explicitly_none: bool,
) -> PyResult<AuthResult> {
    // If auth=None was explicitly passed, disable auth
    if auth_explicitly_none {
        return Ok(AuthResult::None);
    }

    let effective_auth = if auth.is_some() {
        auth
    } else {
        client_auth.map(|a| a.bind(py))
    };
    if let Some(a) = effective_auth {
        // Check for tuple (Basic auth)
        if let Ok(tuple) = a.cast::<pyo3::types::PyTuple>() {
            if tuple.len() == 2 {
                let user: String = tuple.get_item(0)?.extract()?;
                let pass: String = tuple.get_item(1)?.extract()?;
                let creds = format!("{}:{}", user, pass);
                let b64 = {
                    use base64::Engine;
                    base64::engine::general_purpose::STANDARD.encode(creds.as_bytes())
                };
                request
                    .headers
                    .bind(py)
                    .borrow_mut()
                    .set_header("authorization", &format!("Basic {}", b64));
            }
            return Ok(AuthResult::Applied);
        }

        // Check for Auth instance: prefer auth_flow (Python-level generator) over sync_auth_flow
        if a.hasattr("auth_flow")? {
            let req_py = Py::new(py, request.clone())?;
            let flow = a.call_method1("auth_flow", (req_py,))?;
            return Ok(AuthResult::Flow(flow.unbind()));
        }

        // Check for Auth instance (has sync_auth_flow method)
        if a.hasattr("sync_auth_flow")? {
            // Get the auth flow generator
            let req_py = Py::new(py, request.clone())?;
            let flow = a.call_method1("sync_auth_flow", (req_py,))?;
            return Ok(AuthResult::Flow(flow.unbind()));
        }

        // Simple callable
        if a.is_callable() {
            let req_py = Py::new(py, request.clone())?;
            let flow = a.call1((req_py.bind(py),))?;
            if let Ok(r) = flow.extract::<Request>() {
                *request = r;
            }
            return Ok(AuthResult::Applied);
        }
    }
    Ok(AuthResult::None)
}

/// Apply auth specifically for async context - prefers async_auth_flow over sync_auth_flow.
pub fn apply_auth_async<'py>(
    py: Python<'py>,
    auth: Option<&Bound<'py, PyAny>>,
    client_auth: Option<&Py<PyAny>>,
    request: &mut Request,
    auth_explicitly_none: bool,
) -> PyResult<AuthResult> {
    // If auth=None was explicitly passed, disable auth
    if auth_explicitly_none {
        return Ok(AuthResult::None);
    }

    let effective_auth = if auth.is_some() {
        auth
    } else {
        client_auth.map(|a| a.bind(py))
    };
    if let Some(a) = effective_auth {
        // Check for tuple (Basic auth)
        if let Ok(tuple) = a.cast::<pyo3::types::PyTuple>() {
            if tuple.len() == 2 {
                let user: String = tuple.get_item(0)?.extract()?;
                let pass: String = tuple.get_item(1)?.extract()?;
                let creds = format!("{}:{}", user, pass);
                let b64 = {
                    use base64::Engine;
                    base64::engine::general_purpose::STANDARD.encode(creds.as_bytes())
                };
                request
                    .headers
                    .bind(py)
                    .borrow_mut()
                    .set_header("authorization", &format!("Basic {}", b64));
            }
            return Ok(AuthResult::Applied);
        }

        // For async: prefer async_auth_flow
        if a.hasattr("async_auth_flow")? {
            let req_py = Py::new(py, request.clone())?;
            let flow = a.call_method1("async_auth_flow", (req_py,))?;
            return Ok(AuthResult::Flow(flow.unbind()));
        }

        // Check for Python-level auth_flow (generator)
        if a.hasattr("auth_flow")? {
            let req_py = Py::new(py, request.clone())?;
            let flow = a.call_method1("auth_flow", (req_py,))?;
            return Ok(AuthResult::Flow(flow.unbind()));
        }

        // Fall back to sync_auth_flow
        if a.hasattr("sync_auth_flow")? {
            let req_py = Py::new(py, request.clone())?;
            let flow = a.call_method1("sync_auth_flow", (req_py,))?;
            return Ok(AuthResult::Flow(flow.unbind()));
        }

        // Simple callable
        if a.is_callable() {
            let req_py = Py::new(py, request.clone())?;
            let flow = a.call1((req_py.bind(py),))?;
            if let Ok(r) = flow.extract::<Request>() {
                *request = r;
            }
            return Ok(AuthResult::Applied);
        }
    }
    Ok(AuthResult::None)
}

/// Apply cookies (client-level + request-level) to headers.
pub fn apply_cookies<'py>(
    py: Python<'py>,
    cookies: &Cookies,
    request_cookies: Option<&Bound<'py, PyAny>>,
    headers: &mut Headers,
) -> PyResult<()> {
    let mut cookie_parts: Vec<String> = Vec::new();
    {
        let jar = &cookies.jar;
        if let Ok(bound_jar) = jar.bind(py).cast::<PyAny>() {
            if let Ok(iter) = bound_jar.try_iter() {
                for item in iter {
                    let cookie: Bound<PyAny> = item?;
                    if let (Ok(name), Ok(value)) = (cookie.getattr("name"), cookie.getattr("value"))
                    {
                        let n: String = name.extract()?;
                        let v: String = value.extract()?;
                        cookie_parts.push(format!("{}={}", n, v));
                    }
                }
            }
        }
    }
    if let Some(c) = request_cookies {
        if !c.is_none() {
            // Emit DeprecationWarning for per-request cookies
            let warnings = py.import("warnings")?;
            warnings.call_method1("warn", (
                "Setting per-request cookies=<...> is being deprecated, because the expected behaviour on cookie persistence is ambiguous. Set cookies directly on a client instance instead.",
                py.import("builtins")?.getattr("DeprecationWarning")?,
            ))?;
        }
        if let Ok(d) = c.cast::<PyDict>() {
            for (k, v) in d.iter() {
                let ks: String = k.extract()?;
                let vs: String = v.extract()?;
                cookie_parts.push(format!("{}={}", ks, vs));
            }
        } else if let Ok(cookie_val) = c.extract::<String>() {
            cookie_parts.push(cookie_val);
        }
    }
    if !cookie_parts.is_empty() {
        headers.set_header("Cookie", &cookie_parts.join("; "));
    }
    Ok(())
}

/// Build a stream object from content if it's an iterator.
pub fn build_stream_obj(
    py: Python<'_>,
    content: Option<&Bound<'_, PyAny>>,
    body: &Option<Vec<u8>>,
) -> PyResult<Option<Py<PyAny>>> {
    if body.is_none() {
        if let Some(c) = content {
            let _ = c.get_type().name();
            let has_aiter = c.hasattr("__aiter__").unwrap_or(false);
            let has_iter = c.hasattr("__iter__").unwrap_or(false);

            if has_aiter || has_iter {
                if has_iter {
                    let iterator = c.clone().unbind();
                    let stream_inst = Py::new(py, crate::types::IteratorByteStream::new(iterator))?;
                    return Ok(Some(stream_inst.bind(py).clone().into_any().unbind()));
                } else {
                    return Ok(Some(c.clone().unbind()));
                }
            }
        }
    }
    Ok(None)
}

/// Fire sync event hooks for a given hook key ("request" or "response").
#[allow(dead_code)]
pub fn fire_sync_event_hooks(
    py: Python<'_>,
    event_hooks: &Option<Py<PyAny>>,
    key: &str,
    target: &Py<PyAny>,
) -> PyResult<()> {
    if let Some(hooks) = event_hooks {
        if let Ok(l) = hooks.bind(py).get_item(key) {
            if let Ok(l) = l.cast::<pyo3::types::PyList>() {
                for hook in l.iter() {
                    hook.call1((target.bind(py),))?;
                }
            }
        }
    }
    Ok(())
}

/// Extract set-cookie headers from a response and add them to a cookie jar.
pub fn extract_cookies_to_jar(
    py: Python<'_>,
    response: &Response,
    jar: &Bound<'_, PyAny>,
) -> PyResult<()> {
    let mut cookie_list = Vec::new();
    for (k, v) in response.headers.bind(py).borrow().get_multi_items() {
        if k.to_lowercase() == "set-cookie" {
            cookie_list.push(v);
        }
    }

    if cookie_list.is_empty() {
        return Ok(());
    }

    let code_str = "def process(jar, cookie_list, url):\n    import urllib.request\n    from http.client import HTTPMessage\n    req = urllib.request.Request(url)\n    msg = HTTPMessage()\n    for c in cookie_list:\n        msg.add_header('Set-Cookie', c)\n    res = type('Response', (object,), {'info': lambda self: msg})()\n    jar.extract_cookies(res, req)\n";
    let process_cookies_code = std::ffi::CString::new(code_str)?;

    let locals = pyo3::types::PyDict::new(py);
    py.run(&process_cookies_code, None, Some(&locals))?;
    let process_func = locals
        .get_item("process")?
        .expect("Function 'process' should be defined");

    let py_cookie_list = pyo3::types::PyList::new(py, cookie_list)?;
    process_func.call1((jar, py_cookie_list, response.url()?.to_string()))?;

    Ok(())
}

/// Check if a URL matches a mount pattern (httpx-compatible).
pub fn url_matches_pattern(url_str: &str, pattern: &str) -> bool {
    // "all://" matches everything
    if pattern == "all://" {
        return true;
    }

    // Parse scheme from pattern
    let (pattern_scheme, pattern_rest) = if let Some(idx) = pattern.find("://") {
        (&pattern[..idx], &pattern[idx + 3..])
    } else {
        return false;
    };

    // Parse scheme from URL
    let (url_scheme, url_rest) = if let Some(idx) = url_str.find("://") {
        (&url_str[..idx], &url_str[idx + 3..])
    } else {
        return false;
    };

    // Check if pattern has "all" scheme or matches URL scheme
    if pattern_scheme != "all" && pattern_scheme != url_scheme {
        return false;
    }

    // Scheme-only pattern (e.g., "http://")
    if pattern_rest.is_empty() {
        return true;
    }

    // Extract host:port from URL
    let url_host_port = if let Some(slash) = url_rest.find('/') {
        &url_rest[..slash]
    } else {
        url_rest
    };
    let (url_host, _url_port) = if let Some(colon) = url_host_port.rfind(':') {
        (&url_host_port[..colon], Some(&url_host_port[colon + 1..]))
    } else {
        (url_host_port, None)
    };

    // Extract host:port from pattern
    let pattern_host_port = if let Some(slash) = pattern_rest.find('/') {
        &pattern_rest[..slash]
    } else {
        pattern_rest
    };
    let (pattern_host, _pattern_port) = if let Some(colon) = pattern_host_port.rfind(':') {
        (
            &pattern_host_port[..colon],
            Some(&pattern_host_port[colon + 1..]),
        )
    } else {
        (pattern_host_port, None)
    };

    // Wildcard patterns
    if pattern_host == "*" {
        return true;
    }

    if pattern_host.starts_with("*.") {
        // "*.example.com" matches "www.example.com" but NOT "example.com"
        let suffix = &pattern_host[1..]; // ".example.com"
        return url_host.ends_with(suffix);
    }

    if pattern_host.starts_with('*') {
        // "*example.com" matches "example.com" and "www.example.com" but NOT "wwwexample.com"
        let suffix = &pattern_host[1..]; // "example.com"
        return url_host == suffix || url_host.ends_with(&format!(".{}", suffix));
    }

    // Exact host match
    url_host == pattern_host
}

/// Select the best matching transport from a mounts dict.
pub fn select_transport(
    mounts: Bound<'_, PyDict>,
    default_transport: Py<PyAny>,
    url: &URL,
) -> PyResult<Py<PyAny>> {
    let url_str = url.to_string();

    // Priority order: most specific match wins
    // httpx priority: scheme://domain > scheme:// > all://domain > all://
    let mut best_transport: Option<Py<PyAny>> = None;
    let mut best_priority = 0;

    for (pattern_obj, transport_obj) in mounts.iter() {
        let pattern: String = pattern_obj.extract()?;
        if url_matches_pattern(&url_str, &pattern) {
            let priority = compute_pattern_priority(&pattern);
            if priority > best_priority {
                best_priority = priority;
                best_transport = Some(transport_obj.into());
            }
        }
    }

    Ok(best_transport.unwrap_or(default_transport))
}

/// Compute priority for a mount pattern (higher = more specific).
fn compute_pattern_priority(pattern: &str) -> u32 {
    let (scheme, rest) = if let Some(idx) = pattern.find("://") {
        (&pattern[..idx], &pattern[idx + 3..])
    } else {
        return 0;
    };

    let has_specific_scheme = scheme != "all";
    let has_host = !rest.is_empty();

    match (has_specific_scheme, has_host) {
        (true, true) => 4,   // http://example.com
        (false, true) => 3,  // all://example.com
        (true, false) => 2,  // http://
        (false, false) => 1, // all://
    }
}

/// Resolve URL from a base URL and url string.
pub fn resolve_url(base_url: &Option<URL>, url_str: &str) -> PyResult<URL> {
    if let Some(ref base) = base_url {
        merge_base_url(base, url_str)
    } else {
        URL::create_from_str(url_str)
    }
}

/// Parse proxy from a PyAny argument.
pub fn parse_proxy(proxy: Option<&Bound<'_, PyAny>>) -> PyResult<Option<crate::config::Proxy>> {
    if let Some(p) = proxy {
        if let Ok(proxy_val) = p.extract::<crate::config::Proxy>() {
            Ok(Some(proxy_val))
        } else if let Ok(s) = p.extract::<String>() {
            Ok(Some(crate::config::Proxy::create_from_url(&s)?))
        } else {
            Ok(None)
        }
    } else {
        Ok(None)
    }
}

/// Parse base_url from a PyAny argument.
pub fn parse_base_url(base_url: Option<&Bound<'_, PyAny>>) -> PyResult<Option<URL>> {
    if let Some(b) = base_url {
        let s = b.str()?.extract::<String>()?;
        if s.is_empty() {
            Ok(None)
        } else {
            Ok(Some(URL::create_from_str(&s)?))
        }
    } else {
        Ok(None)
    }
}

/// Create a default sync HTTPTransport.
pub fn create_default_sync_transport(
    py: Python<'_>,
    verify: Option<&Bound<'_, PyAny>>,
    cert: Option<&str>,
    http2: bool,
    limits: Option<&Limits>,
    proxy: Option<&Bound<'_, PyAny>>,
) -> PyResult<Py<PyAny>> {
    let proxy_obj = parse_proxy(proxy)?;
    let (verify_bool, verify_path) = if let Some(v) = verify {
        extract_verify_path(v)
    } else {
        (true, None)
    };
    let t = HTTPTransport::create(
        verify_bool,
        verify_path.as_deref().or(cert),
        http2,
        limits,
        proxy_obj.as_ref(),
        0,
    )?;
    Ok(Py::new(py, t)?.into())
}

/// Create a default async HTTPTransport.
pub fn create_default_async_transport(
    py: Python<'_>,
    verify: Option<&Bound<'_, PyAny>>,
    cert: Option<&str>,
    http2: bool,
    limits: Option<&Limits>,
    proxy: Option<&Bound<'_, PyAny>>,
) -> PyResult<Py<PyAny>> {
    let proxy_obj = parse_proxy(proxy)?;
    let (verify_bool, verify_path) = if let Some(v) = verify {
        extract_verify_path(v)
    } else {
        (true, None)
    };
    let t = crate::transports::default::AsyncHTTPTransport::create(
        verify_bool,
        verify_path.as_deref().or(cert),
        http2,
        limits,
        proxy_obj.as_ref(),
        0,
    )?;
    Ok(Py::new(py, t)?.into())
}

/// Parse mounts dict into Vec<(pattern, transport)> for sync client.
pub fn parse_sync_mounts(mounts: Option<&Bound<'_, PyAny>>) -> PyResult<Vec<(String, Py<PyAny>)>> {
    let mut mounts_vec: Vec<(String, Py<PyAny>)> = Vec::new();
    if let Some(m) = mounts {
        if let Ok(d) = m.cast::<PyDict>() {
            for (k, v) in d.iter() {
                let pattern: String = k.extract()?;
                // Validate deprecated patterns
                if pattern == "http" || pattern == "https" || pattern == "all" {
                    return Err(pyo3::exceptions::PyValueError::new_err(
                        format!("Proxy keys should use proper URL-like patterns (e.g., 'http://', 'https://', 'all://'), not '{}'", pattern)
                    ));
                }
                mounts_vec.push((pattern, v.unbind()));
            }
        }
    }
    Ok(mounts_vec)
}

/// Check if a URL should be excluded by NO_PROXY.
pub fn is_no_proxy(url_str: &str, no_proxy: &str) -> bool {
    let no_proxy = no_proxy.trim();
    if no_proxy.is_empty() {
        return false;
    }
    if no_proxy == "*" {
        return true;
    }

    // Parse the URL to get host
    let (url_scheme, url_rest) = if let Some(idx) = url_str.find("://") {
        (&url_str[..idx], &url_str[idx + 3..])
    } else {
        ("", url_str)
    };
    let url_host = if let Some(slash) = url_rest.find('/') {
        &url_rest[..slash]
    } else {
        url_rest
    };
    let url_host = if let Some(colon) = url_host.rfind(':') {
        &url_host[..colon]
    } else {
        url_host
    };

    for entry in no_proxy.split(',') {
        let entry = entry.trim();
        if entry.is_empty() {
            continue;
        }

        // Check if entry has a scheme
        if let Some(idx) = entry.find("://") {
            let entry_scheme = &entry[..idx];
            let entry_host = &entry[idx + 3..];
            if entry_scheme != url_scheme {
                continue;
            }
            if entry_host == url_host {
                return true;
            }
            continue;
        }

        let entry_clean = entry.trim_start_matches('.');

        // Exact match
        if url_host == entry_clean {
            return true;
        }

        // Domain suffix match (with dot separator)
        if url_host.ends_with(&format!(".{}", entry_clean)) {
            return true;
        }
    }
    false
}

/// Get a proxy transport for a URL based on environment variables.
pub fn get_env_proxy_url(url_str: &str) -> Option<String> {
    let scheme = if let Some(idx) = url_str.find("://") {
        &url_str[..idx]
    } else {
        return None;
    };

    // Look up scheme-specific proxy first, then ALL_PROXY
    let proxy_url = match scheme {
        "http" => std::env::var("HTTP_PROXY")
            .ok()
            .or_else(|| std::env::var("http_proxy").ok()),
        "https" => std::env::var("HTTPS_PROXY")
            .ok()
            .or_else(|| std::env::var("https_proxy").ok()),
        _ => None,
    }
    .or_else(|| {
        std::env::var("ALL_PROXY")
            .ok()
            .or_else(|| std::env::var("all_proxy").ok())
    });

    let proxy_url = proxy_url?;
    if proxy_url.is_empty() {
        return None;
    }

    // Check NO_PROXY
    let no_proxy = std::env::var("NO_PROXY")
        .or_else(|_| std::env::var("no_proxy"))
        .unwrap_or_default();
    if is_no_proxy(url_str, &no_proxy) {
        return None;
    }

    // Auto-prepend http:// if missing
    if proxy_url.contains("://") {
        Some(proxy_url)
    } else {
        Some(format!("http://{}", proxy_url))
    }
}
