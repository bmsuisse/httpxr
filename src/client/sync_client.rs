use pyo3::prelude::*;
use pyo3::types::{PyAnyMethods, PyBytes, PyDict, PyDictMethods};
use std::time::Instant;

use crate::config::{Limits, Timeout};
use crate::models::{Cookies, Headers, Request, Response};
use crate::urls::URL;

use super::common::*;

/// Synchronous HTTP Client backed by reqwest::blocking.
#[pyclass]
pub struct Client {
    pub(crate) base_url: Option<URL>,
    pub(crate) auth: Option<Py<PyAny>>,
    pub(crate) params: Option<Py<PyAny>>,
    pub(crate) default_headers: Py<Headers>,
    pub(crate) cookies: Cookies,
    pub(crate) max_redirects: u32,
    pub(crate) follow_redirects: bool,
    pub(crate) transport: Py<PyAny>,
    pub(crate) mounts: Vec<(String, Py<PyAny>)>,
    pub(crate) is_closed: bool,
    pub(crate) trust_env: bool,
    pub(crate) event_hooks: Option<Py<PyAny>>,
    pub(crate) timeout: Option<Py<PyAny>>,
    pub(crate) default_encoding: Option<Py<PyAny>>,
    // ── Cached fields for ultra-fast path ──
    /// Pre-built raw header bytes — avoids cloning Headers on every request
    pub(crate) cached_raw_headers: Vec<(Vec<u8>, Vec<u8>)>,
    /// Pre-parsed timeout duration for the fast path
    pub(crate) cached_timeout: Option<std::time::Duration>,
    pub(crate) cached_connect_timeout: Option<f64>,
    pub(crate) cached_read_timeout: Option<f64>,
    pub(crate) cached_write_timeout: Option<f64>,
    /// Pre-computed base URL string (trimmed of trailing /) for fast path concatenation
    pub(crate) cached_base_url_str: Option<String>,
}

#[pymethods]
impl Client {
    #[new]
    #[pyo3(signature = (
        *,
        auth=None,
        params=None,
        headers=None,
        cookies=None,
        timeout=None,
        follow_redirects=false,
        max_redirects=None,
        verify=None,
        cert=None,
        http2=false,
        proxy=None,
        limits=None,
        mounts=None,
        transport=None,
        base_url=None,
        trust_env=true,
        default_encoding=None,
        event_hooks=None
    ))]
    pub fn new(
        py: Python<'_>,
        auth: Option<AuthArg>,
        params: Option<Py<PyAny>>,
        headers: Option<&Bound<'_, PyAny>>,
        cookies: Option<&Bound<'_, PyAny>>,
        timeout: Option<&Bound<'_, PyAny>>,
        follow_redirects: bool,
        max_redirects: Option<u32>,
        verify: Option<&Bound<'_, PyAny>>,
        cert: Option<&str>,
        http2: bool,
        proxy: Option<&Bound<'_, PyAny>>,
        limits: Option<&Limits>,
        mounts: Option<&Bound<'_, PyAny>>,
        transport: Option<&Bound<'_, PyAny>>,
        base_url: Option<&Bound<'_, PyAny>>,
        trust_env: bool,
        default_encoding: Option<&Bound<'_, PyAny>>,
        event_hooks: Option<&Bound<'_, PyAny>>,
    ) -> PyResult<Self> {
        let mut hdrs = Headers::create(headers, "utf-8")?;
        // Add default headers if not already present
        if !hdrs.contains_header("user-agent") {
            hdrs.set_header(
                "user-agent",
                &format!("python-httpxr/{}", env!("CARGO_PKG_VERSION")),
            );
        }
        if !hdrs.contains_header("accept") {
            hdrs.set_header("accept", "*/*");
        }
        if !hdrs.contains_header("accept-encoding") {
            hdrs.set_header("accept-encoding", "gzip, deflate, br, zstd");
        }
        if !hdrs.contains_header("connection") {
            hdrs.set_header("connection", "keep-alive");
        }
        let hdrs_py = Py::new(py, hdrs)?;
        let ckies = Cookies::create(py, cookies)?;
        let base = parse_base_url(base_url)?;

        let transport_obj = if let Some(t) = transport {
            t.clone().unbind()
        } else {
            create_default_sync_transport(py, verify, cert, http2, limits, proxy)?
        };

        let mounts_vec = parse_sync_mounts(mounts)?;
        let eh = event_hooks.map(|e| e.clone().unbind());
        let to = timeout.map(|t| t.clone().unbind());
        let de = default_encoding.map(|e| e.clone().unbind());

        // Pre-cache raw headers for the ultra-fast path
        let cached_raw_headers = hdrs_py.bind(py).borrow().get_raw_items_owned();

        // Pre-parse timeout for the ultra-fast path
        let (cached_timeout, cached_connect_timeout, cached_read_timeout, cached_write_timeout) =
            if let Some(ref t) = to {
                if let Ok(t_val) = t.extract::<Timeout>(py) {
                    let dur = [t_val.connect, t_val.read, t_val.write]
                        .iter()
                        .filter_map(|t| *t)
                        .reduce(f64::min)
                        .map(std::time::Duration::from_secs_f64);
                    (dur, t_val.connect, t_val.read, t_val.write)
                } else {
                    (None, None, None, None)
                }
            } else {
                (None, None, None, None)
            };

        // Pre-compute base URL string before base is moved into struct
        let cached_base_url_str = base.as_ref().map(|b| b.to_string().trim_end_matches('/').to_string());

        Ok(Client {
            base_url: base,
            auth: auth
                .map(|a| {
                    if let AuthArg::Custom(p) = a {
                        Some(p)
                    } else {
                        None
                    }
                })
                .flatten(),
            params,
            default_headers: hdrs_py,
            cookies: ckies,
            max_redirects: max_redirects.unwrap_or(DEFAULT_MAX_REDIRECTS),
            follow_redirects,
            transport: transport_obj,
            mounts: mounts_vec,
            is_closed: false,
            trust_env,
            event_hooks: eh,
            timeout: to,
            default_encoding: de,
            cached_raw_headers,
            cached_timeout,
            cached_connect_timeout,
            cached_read_timeout,
            cached_write_timeout,
            cached_base_url_str,
        })
    }

    #[pyo3(signature = (method, url, *, content=None, data=None, files=None, json=None, params=None, headers=None, cookies=None, follow_redirects=None, timeout=None, extensions=None, **kwargs))]
    pub fn request(
        &self,
        py: Python<'_>,
        method: &str,
        url: &Bound<'_, PyAny>,
        content: Option<&Bound<'_, PyAny>>,
        data: Option<&Bound<'_, PyAny>>,
        files: Option<&Bound<'_, PyAny>>,
        json: Option<&Bound<'_, PyAny>>,
        params: Option<&Bound<'_, PyAny>>,
        headers: Option<&Bound<'_, PyAny>>,
        cookies: Option<&Bound<'_, PyAny>>,
        follow_redirects: Option<bool>,
        timeout: Option<&Bound<'_, PyAny>>,
        extensions: Option<&Bound<'_, PyAny>>,
        kwargs: Option<&Bound<'_, PyDict>>,
    ) -> PyResult<Response> {
        // Enforce closed state
        if self.is_closed {
            return Err(pyo3::exceptions::PyRuntimeError::new_err(
                "Cannot send a request, as the client has been closed.",
            ));
        }

        let start = Instant::now();

        // ── ULTRA-FAST PATH ────────────────────────────────────────────
        // For simple GET/HEAD/DELETE/OPTIONS with no body, auth, hooks,
        // or per-request overrides: bypass ALL Python overhead and go
        // straight to reqwest.
        let method_upper = method.to_uppercase();
        let is_bodyless = matches!(method_upper.as_str(), "GET" | "HEAD" | "DELETE" | "OPTIONS");
        // Extract URL string early for fast-path validation
        let url_str_for_check = url.str()?.extract::<String>()?;
        let has_valid_scheme = (url_str_for_check.starts_with("http://") && url_str_for_check.len() > 7)
            || (url_str_for_check.starts_with("https://") && url_str_for_check.len() > 8)
            || url_str_for_check.starts_with("/");
        let can_fast_path = is_bodyless
            && has_valid_scheme
            && content.is_none()
            && data.is_none()
            && files.is_none()
            && json.is_none()
            && params.is_none()
            && cookies.is_none()
            && extensions.is_none()
            && kwargs.is_none()
            && self.auth.is_none()
            && self.params.is_none()
            && self.event_hooks.is_none()
            && self.mounts.is_empty();

        if can_fast_path {
            // Try to extract our HTTPTransport for direct Rust dispatch
            let t_bound = self.transport.bind(py);
            if let Ok(t) = t_bound.cast::<crate::transports::default::HTTPTransport>() {
                let transport = t.borrow();

                // Resolve URL — fast string concat using cached base URL string
                let url_str = url_str_for_check;
                let full_url = if let Some(ref base_str) = self.cached_base_url_str {
                    if url_str.starts_with("http://") || url_str.starts_with("https://") {
                        url_str
                    } else {
                        format!("{}{}", base_str, url_str)
                    }
                } else {
                    url_str
                };

                // No reqwest::Url::parse needed — reqwest parses internally in .get()/.request()

                // Extract timeout — use cached values unless per-request override
                let (timeout_val, connect_timeout_val, read_timeout_val, write_timeout_val) =
                    if let Some(t_arg) = timeout {
                        if t_arg.is_none() {
                            (None, None, None, None)
                        } else {
                            let t =
                                Timeout::new(py, Some(t_arg), None, None, None, None)?;
                            let dur = [t.connect, t.read, t.write]
                                .iter()
                                .filter_map(|t| *t)
                                .reduce(f64::min)
                                .map(std::time::Duration::from_secs_f64);
                            (dur, t.connect, t.read, t.write)
                        }
                    } else {
                        // Use pre-cached timeout — no Python extraction needed
                        (self.cached_timeout, self.cached_connect_timeout, self.cached_read_timeout, self.cached_write_timeout)
                    };

                // Use pre-cached raw headers — no cloning needed
                let raw_headers = &self.cached_raw_headers;

                // Merge per-request headers if any
                let extra_raw: Vec<(Vec<u8>, Vec<u8>)> = if let Some(h) = headers {
                    let extra = Headers::create(Some(h), "utf-8")?;
                    extra.get_raw_items_owned()
                } else {
                    Vec::new()
                };

                let method_reqwest = match method_upper.as_str() {
                    "GET" => reqwest::Method::GET,
                    "HEAD" => reqwest::Method::HEAD,
                    "DELETE" => reqwest::Method::DELETE,
                    "OPTIONS" => reqwest::Method::OPTIONS,
                    _ => unreachable!(),
                };

                let do_follow_redirects = follow_redirects.unwrap_or(self.follow_redirects);
                let max_redirects = self.max_redirects;

                // Build reqwest request — no host header needed (reqwest adds it)
                let mut req_builder = transport.client.request(method_reqwest.clone(), &full_url);
                for (key, value) in raw_headers {
                    req_builder = req_builder.header(key.as_slice(), value.as_slice());
                }
                for (key, value) in &extra_raw {
                    req_builder = req_builder.header(key.as_slice(), value.as_slice());
                }
                if let Some(t) = timeout_val {
                    req_builder = req_builder.timeout(t);
                }

                let built = req_builder.build().map_err(|e| {
                    pyo3::exceptions::PyRuntimeError::new_err(format!("Failed to build request: {}", e))
                })?;

                let client = transport.client.clone();
                let handle = transport.handle.clone();
                // Clone raw_headers for the redirect-loop closure (only used on redirects)
                let raw_headers_owned = raw_headers.clone();

                // Redirect loop with GIL released
                let result = py.detach(move || {
                    handle.block_on(async {
                        let mut current_url = full_url;
                        let mut redirects_remaining = max_redirects;
                        let mut current_request = Some(built);
                        let mut history: Vec<(u16, Vec<(Vec<u8>, Vec<u8>)>, Vec<u8>, String)> = Vec::new();

                        loop {
                            let req = if let Some(r) = current_request.take() {
                                r
                            } else {
                                let mut rb = client.request(method_reqwest.clone(),
                                    reqwest::Url::parse(&current_url).unwrap());
                                for (key, value) in &raw_headers_owned {
                                    rb = rb.header(key.as_slice(), value.as_slice());
                                }
                                for (key, value) in &extra_raw {
                                    rb = rb.header(key.as_slice(), value.as_slice());
                                }
                                if let Some(t) = timeout_val {
                                    rb = rb.timeout(t);
                                }

                                rb.build()?
                            };

                            let response = client.execute(req).await?;
                            let status = response.status().as_u16();
                            let is_redirect = (300..400).contains(&status);

                            let redirect_location = if is_redirect && do_follow_redirects {
                                response.headers()
                                    .get("location")
                                    .and_then(|v| v.to_str().ok())
                                    .map(|s| s.to_string())
                            } else {
                                None
                            };

                            let resp_headers: Vec<(Vec<u8>, Vec<u8>)> = response
                                .headers()
                                .iter()
                                .map(|(k, v)| (k.as_str().as_bytes().to_vec(), v.as_bytes().to_vec()))
                                .collect();

                            let body = response.bytes().await?;

                            if let Some(loc) = redirect_location {
                                if redirects_remaining == 0 {
                                    // Signal too-many-redirects via a special Result type
                                    break Ok((0u16, Vec::new(), b"too many redirects".to_vec(), current_url, history));
                                }
                                // Save this redirect response in history
                                history.push((status, resp_headers, body.to_vec(), current_url.clone()));
                                redirects_remaining -= 1;
                                current_url = if loc.starts_with("http://") || loc.starts_with("https://") {
                                    loc
                                } else if let Ok(base) = reqwest::Url::parse(&current_url) {
                                    base.join(&loc).map(|u| u.to_string()).unwrap_or(loc)
                                } else {
                                    loc
                                };
                                continue;
                            }

                            break Ok((status, resp_headers, body.to_vec(), current_url, history));
                        }
                    })
                }).map_err(|e: reqwest::Error| {
                    let msg = format!("{}", e);
                    if e.is_timeout() {
                        if e.is_connect()
                            || (connect_timeout_val.is_some()
                                && read_timeout_val.is_none()
                                && write_timeout_val.is_none())
                        {
                            crate::exceptions::ConnectTimeout::new_err(msg)
                        } else {
                            crate::exceptions::ReadTimeout::new_err(msg)
                        }
                    } else if e.is_connect() {
                        crate::exceptions::ConnectError::new_err(msg)
                    } else if msg.contains("too many redirects") {
                        crate::exceptions::TooManyRedirects::new_err(msg)
                    } else {
                        crate::exceptions::NetworkError::new_err(msg)
                    }
                })?;

                let (status_code, resp_headers, body_bytes, final_url, redirect_history) = result;

                // Check for too-many-redirects sentinel
                if status_code == 0 {
                    return Err(crate::exceptions::TooManyRedirects::new_err(
                        "Too many redirects".to_string(),
                    ));
                }

                let elapsed = start.elapsed().as_secs_f64();

                // Helper: status code to reason phrase
                let reason_for = |code: u16| -> &'static str {
                    match code {
                        200 => "OK", 201 => "Created", 204 => "No Content",
                        301 => "Moved Permanently", 302 => "Found", 303 => "See Other",
                        304 => "Not Modified", 307 => "Temporary Redirect", 308 => "Permanent Redirect",
                        400 => "Bad Request", 401 => "Unauthorized", 403 => "Forbidden",
                        404 => "Not Found", 500 => "Internal Server Error",
                        _ => "",
                    }
                };

                // Build history Response objects from redirect data
                let mut history_responses: Vec<Response> = Vec::new();
                for (hist_status, hist_headers, hist_body, hist_url) in &redirect_history {
                    let h_hdrs = Headers::from_raw_byte_pairs(hist_headers.clone());
                    let h_ext = pyo3::types::PyDict::new(py);
                    h_ext.set_item("http_version", PyBytes::new(py, b"HTTP/1.1"))?;
                    let h_req_url = crate::urls::URL::create_from_str_fast(hist_url);
                    let h_req = Request {
                        method: method_upper.clone(),
                        url: h_req_url,
                        headers: Py::new(py, Headers::empty())?,
                        extensions: pyo3::types::PyDict::new(py).into(),
                        content_body: None,
                        stream: None,
                        stream_response: false,
                    };
                    if log::log_enabled!(target: "httpxr", log::Level::Info) {
                        log::info!(target: "httpxr", "HTTP Request: {} {} \"HTTP/1.1 {} {}\"", method_upper, hist_url, hist_status, reason_for(*hist_status));
                    }
                    history_responses.push(Response {
                        status_code: *hist_status,
                        headers: Py::new(py, h_hdrs)?,
                        extensions: h_ext.into(),
                        request: Some(h_req),
                        history: Vec::new(),
                        content_bytes: Some(hist_body.clone()),
                        stream: None,
                        default_encoding: pyo3::types::PyString::intern(py, "utf-8").into_any().unbind(),
                        default_encoding_override: None,
                        elapsed: None,
                        is_closed_flag: false,
                        is_stream_consumed: false,
                        was_streaming: false,
                        text_accessed: std::sync::atomic::AtomicBool::new(false),
                        num_bytes_downloaded_counter: std::sync::Arc::new(std::sync::atomic::AtomicUsize::new(0)),
                    });
                }

                let hdrs = Headers::from_raw_byte_pairs(resp_headers);
                let ext = pyo3::types::PyDict::new(py);
                ext.set_item("http_version", PyBytes::new(py, b"HTTP/1.1"))?;

                // Build minimal Request for response.url / response.request
                let req_url = crate::urls::URL::create_from_str_fast(&final_url);
                let req_hdrs = Py::new(py, Headers::empty())?;
                let req_ext = pyo3::types::PyDict::new(py);
                let minimal_request = Request {
                    method: method_upper.clone(),
                    url: req_url,
                    headers: req_hdrs,
                    extensions: req_ext.into(),
                    content_body: None,
                    stream: None,
                    stream_response: false,
                };

                if log::log_enabled!(target: "httpxr", log::Level::Info) {
                    log::info!(target: "httpxr", "HTTP Request: {} {} \"HTTP/1.1 {} {}\"", method_upper, final_url, status_code, reason_for(status_code));
                }

                return Ok(Response {
                    status_code,
                    headers: Py::new(py, hdrs)?,
                    extensions: ext.into(),
                    request: Some(minimal_request),
                    history: history_responses,
                    content_bytes: Some(body_bytes),
                    stream: None,
                    default_encoding: pyo3::types::PyString::intern(py, "utf-8")
                        .into_any()
                        .unbind(),
                    default_encoding_override: None,
                    elapsed: Some(elapsed),
                    is_closed_flag: false,
                    is_stream_consumed: false,
                    was_streaming: false,
                    text_accessed: std::sync::atomic::AtomicBool::new(false),
                    num_bytes_downloaded_counter: std::sync::Arc::new(
                        std::sync::atomic::AtomicUsize::new(0),
                    ),
                });
            }
        }
        // ── END ULTRA-FAST PATH ────────────────────────────────────────

        // Build URL
        let url_str = url.str()?.extract::<String>()?;
        let mut target_url = match resolve_url(&self.base_url, &url_str) {
            Ok(u) => u,
            Err(e) => {
                let msg = e.to_string();
                if msg.contains("RelativeUrlWithoutBase")
                    || msg.contains("EmptyHost")
                    || msg.contains("empty host")
                {
                    return Err(crate::exceptions::LocalProtocolError::new_err(format!(
                        "Invalid URL '{}'",
                        url_str
                    )));
                }
                return Err(crate::exceptions::UnsupportedProtocol::new_err(format!(
                    "Request URL has an unsupported protocol '{}': {}",
                    url_str, msg
                )));
            }
        };

        // Validate resolved URL has valid scheme and host
        let scheme = target_url.get_scheme();
        let host = target_url.get_host();
        if scheme.is_empty() || scheme == "://" {
            return Err(crate::exceptions::UnsupportedProtocol::new_err(format!(
                "Request URL is missing a scheme for URL '{}'",
                url_str
            )));
        }
        if !["http", "https", "ws", "wss"].contains(&&*scheme)
            && !self
                .mounts
                .iter()
                .any(|(p, _)| url_matches_pattern(&target_url.to_string(), p))
        {
            return Err(crate::exceptions::UnsupportedProtocol::new_err(format!(
                "Request URL has an unsupported protocol '{}': for URL '{}'",
                scheme, url_str
            )));
        }
        if host.is_empty() && ["http", "https"].contains(&&*scheme) {
            return Err(crate::exceptions::LocalProtocolError::new_err(format!(
                "Invalid URL '{}'",
                url_str
            )));
        }

        // Merge client-level params and per-request params into URL
        {
            let mut merged_qp_items: Vec<(String, String)> = Vec::new();
            // First add client-level params
            if let Some(ref client_params) = self.params {
                let bound = client_params.bind(py);
                let qp = crate::urls::QueryParams::create(Some(bound))?;
                merged_qp_items.extend(qp.items_raw());
            }
            // Then add per-request params
            if let Some(req_params) = params {
                let qp = crate::urls::QueryParams::create(Some(req_params))?;
                merged_qp_items.extend(qp.items_raw());
            }
            if !merged_qp_items.is_empty() {
                // Merge with existing URL query params
                if let Some(ref existing_q) = target_url.parsed.query {
                    let existing_qp = crate::urls::QueryParams::from_query_string(existing_q);
                    let mut all_items = existing_qp.items_raw();
                    all_items.extend(merged_qp_items);
                    merged_qp_items = all_items;
                }
                let final_qp = crate::urls::QueryParams::from_items(merged_qp_items);
                target_url.parsed.query = Some(final_qp.encode());
            }
        }

        // Merge headers
        let mut merged_headers = self.default_headers.bind(py).borrow().clone();
        if let Some(h) = headers {
            merged_headers.update_from(Some(h))?;
        }

        // Build body
        let body = build_request_body(py, content, data, files, json, &mut merged_headers)?;

        // For POST/PUT/PATCH with no body, set content-length: 0
        if body.is_none()
            && content.is_none()
            && data.is_none()
            && files.is_none()
            && json.is_none()
            && ["POST", "PUT", "PATCH"].contains(&method_upper.as_str())
        {
            if !merged_headers.contains_header("content-length")
                && !merged_headers.contains_header("transfer-encoding")
            {
                merged_headers.set_header("content-length", "0");
            }
        }

        // Arrange headers in httpx order
        let mut merged_headers = arrange_headers_httpx_order(&merged_headers, &target_url);

        // URL auth
        apply_url_auth(&target_url, &mut merged_headers);

        // Apply cookies
        apply_cookies(py, &self.cookies, cookies, &mut merged_headers)?;

        // Build stream object
        let stream_obj = build_stream_obj(py, content, &body)?;

        let mut request = Request {
            method: method_upper,
            url: target_url.clone(),
            headers: Py::new(py, merged_headers)?,
            extensions: PyDict::new(py).into(),
            content_body: body,
            stream: stream_obj,
            stream_response: false,
        };

        if let Some(t) = timeout {
            request.extensions.bind(py).set_item("timeout", t)?;
        }
        if let Some(e) = extensions {
            if let Ok(d) = e.cast::<PyDict>() {
                for (k, v) in d.iter() {
                    request.extensions.bind(py).set_item(k, v)?;
                }
            }
        }
        let _ = files;

        // Apply auth
        // auth=None (default) means "not set" (use client auth), auth=None means "disable auth"
        // Apply auth
        // AuthArg enum handles: None (Rust default), ExplicitNone (Python None), ExplicitBoolean, Custom
        // Apply auth
        // AuthArg handles distinguishing missing vs explicit None vs Values
        // Apply auth
        // AuthArg (default=True).
        // Boolean(true) -> Default (Use Client Auth)
        // Boolean(false) -> Disable Auth
        // Custom(None) -> Disable Auth
        // Custom(obj) -> Use Object
        // Apply auth
        // AuthArg handles usage.
        // None (missing arg) -> Use Client Auth.
        // Some(AuthArg::Boolean(true)) -> Use Client Auth.
        // Some(AuthArg::Boolean(false)) -> Disable Auth.
        // Some(AuthArg::Custom(obj)) -> If None: Disable, Else: Use Object.
        // Extract auth from kwargs or use Client Default
        // If auth is in kwargs:
        //   None -> Explicit Disable
        //   Object -> Use Object
        // If auth NOT in kwargs -> Default        // Extract auth from kwargs
        let auth_kw = if let Some(kw) = kwargs {
            kw.get_item("auth").unwrap_or(None)
        } else {
            None
        };

        let (auth_val, auth_explicitly_none) = match auth_kw {
            None => (None, false), // Not passed -> Default (Use Client Auth) -> (None, false)? Wait.
            // Logic: apply_auth(..., auth_val, client_auth, ..., disable_client_auth)
            // disable_client_auth=false means USE client auth.
            // So (None, false) is correct for Default.
            Some(b) => {
                if b.is_none() {
                    (None, true) // Explicit None -> Disable Client Auth
                } else {
                    validate_auth_type(py, &b)?;
                    (Some(b), false) // Object
                }
            }
        };
        let auth_result = apply_auth(
            py,
            auth_val.as_ref(),
            self.auth.as_ref(),
            &mut request,
            auth_explicitly_none,
        )?;

        // Apply cookies (string override)
        if let Some(c) = cookies {
            if let Ok(cookie_val) = c.extract::<String>() {
                request
                    .headers
                    .bind(py)
                    .borrow_mut()
                    .set_header("cookie", &cookie_val);
            }
        }

        // Event hooks: request
        if let Some(hooks) = &self.event_hooks {
            if let Ok(l) = hooks.bind(py).get_item("request") {
                if let Ok(l) = l.cast::<pyo3::types::PyList>() {
                    let req_py = Py::new(py, request.clone())?;
                    for hook in l.iter() {
                        hook.call1((req_py.bind(py),))?;
                    }
                }
            }
        }

        match auth_result {
            AuthResult::Flow(flow) => {
                // Drive the auth flow generator
                self._send_handling_auth(py, start, request, flow, follow_redirects)
            }
            _ => {
                // No auth flow, send directly
                self._send_single_request(py, start, request, follow_redirects)
            }
        }
    }
}

impl Client {
    fn _send_single_request(
        &self,
        py: Python<'_>,
        start: Instant,
        request: Request,
        follow_redirects: Option<bool>,
    ) -> PyResult<Response> {
        // Select transport (mounts support)
        let selected_transport = self.select_transport_for_url(py, &request.url);

        // Send request — fast path: direct Rust call if transport is our HTTPTransport
        let mut response = {
            let t_bound = selected_transport.bind(py);
            if let Ok(t) = t_bound.cast::<crate::transports::default::HTTPTransport>() {
                // Direct Rust dispatch — no Python FFI overhead
                t.borrow().send_request(py, &request)?
            } else {
                // Slow path: Python dispatch for custom/mock transports
                let req_py = Py::new(py, request.clone())?;
                let res_py = t_bound.call_method1("handle_request", (req_py,))?;
                res_py.extract::<Response>()?
            }
        };

        response.elapsed = Some(start.elapsed().as_secs_f64());
        response.request = Some(request.clone());

        // Apply default_encoding from client to response
        if let Some(ref de) = self.default_encoding {
            response.default_encoding = de.clone_ref(py);
        }

        // Set http_version in response extensions if not present
        let ext = response.extensions.bind(py);
        if let Ok(d) = ext.cast::<PyDict>() {
            if !d.contains("http_version")? {
                d.set_item("http_version", PyBytes::new(py, b"HTTP/1.1"))?;
            }
        }

        // Log the request/response
        {
            let method = &request.method;
            let url = request.url.to_string();
            let http_version = {
                let ext = response.extensions.bind(py);
                if let Ok(d) = ext.cast::<PyDict>() {
                    d.get_item("http_version")
                        .ok()
                        .flatten()
                        .and_then(|v| v.extract::<Vec<u8>>().ok())
                        .map(|b| String::from_utf8_lossy(&b).to_string())
                        .unwrap_or_else(|| "HTTP/1.1".to_string())
                } else {
                    "HTTP/1.1".to_string()
                }
            };
            let status = response.status_code;
            let reason = response.reason_phrase();
            log::info!(target: "httpxr", "HTTP Request: {} {} \"{} {} {}\"", method, url, http_version, status, reason);
        }

        // Persist response cookies to client cookie jar
        extract_cookies_to_jar(py, &response, self.cookies.jar.bind(py))?;

        // Handle redirects
        let should_follow = follow_redirects.unwrap_or(self.follow_redirects);
        if should_follow {
            return self.handle_redirects(py, start, request, response);
        }

        // Event hooks: response
        if let Some(hooks) = &self.event_hooks {
            if let Ok(l) = hooks.bind(py).get_item("response") {
                if let Ok(l) = l.cast::<pyo3::types::PyList>() {
                    let resp_py = Py::new(py, response.clone())?;
                    for hook in l.iter() {
                        hook.call1((resp_py.bind(py),))?;
                    }
                }
            }
        }

        Ok(response)
    }

    fn _send_handling_auth(
        &self,
        py: Python<'_>,
        start: Instant,
        _original_request: Request,
        flow: Py<PyAny>,
        follow_redirects: Option<bool>,
    ) -> PyResult<Response> {
        let flow_bound = flow.bind(py);
        let mut history: Vec<Response> = Vec::new();
        let _last_response: Option<Response> = None;

        // Get the first request from the flow
        let first_req = match flow_bound.call_method0("__next__") {
            Ok(r) => r.extract::<Request>()?,
            Err(e) if e.is_instance_of::<pyo3::exceptions::PyStopIteration>(py) => {
                return self._send_single_request(py, start, _original_request, follow_redirects);
            }
            Err(e) => return Err(e),
        };

        // Send first request and collect response
        let mut current_req = first_req;

        loop {
            // Check if auth requires_request_body
            let req_body_needed = flow_bound
                .getattr("requires_request_body")
                .ok()
                .and_then(|v| v.extract::<bool>().ok())
                .unwrap_or(false);
            if req_body_needed {
                // Read the request body if needed
                if let Some(ref body) = current_req.content_body {
                    let _ = body; // Body is already available
                }
            }

            // Send the request — fast path: direct Rust dispatch
            let mut response = {
                let selected_transport = self.select_transport_for_url(py, &current_req.url);
                let t_bound = selected_transport.bind(py);
                if let Ok(t) = t_bound.cast::<crate::transports::default::HTTPTransport>() {
                    t.borrow().send_request(py, &current_req)?
                } else {
                    let req_py = Py::new(py, current_req.clone())?;
                    let res_py = t_bound.call_method1("handle_request", (req_py,))?;
                    res_py.extract::<Response>()?
                }
            };
            response.elapsed = Some(start.elapsed().as_secs_f64());
            response.request = Some(current_req.clone());

            // Apply default_encoding
            if let Some(ref de) = self.default_encoding {
                response.default_encoding = de.clone_ref(py);
            }

            // Set http_version
            let ext = response.extensions.bind(py);
            if let Ok(d) = ext.cast::<PyDict>() {
                if !d.contains("http_version")? {
                    d.set_item("http_version", PyBytes::new(py, b"HTTP/1.1"))?;
                }
            }

            // Persist response cookies
            extract_cookies_to_jar(py, &response, self.cookies.jar.bind(py))?;

            // Check if auth requires_response_body
            let resp_body_needed = flow_bound
                .getattr("requires_response_body")
                .ok()
                .and_then(|v| v.extract::<bool>().ok())
                .unwrap_or(false);
            if resp_body_needed {
                // Ensure response body is read
                if response.content_bytes.is_none() {
                    let resp_py = Py::new(py, response.clone())?;
                    let _ = resp_py.call_method0(py, "read");
                    response = resp_py.extract(py)?;
                }
            }

            // Feed the response to the auth flow using send()
            let resp_py = Py::new(py, response.clone())?;
            let next_req = match flow_bound.call_method1("send", (resp_py.bind(py),)) {
                Ok(r) => {
                    // Got another request to send
                    match r.extract::<Request>() {
                        Ok(req) => Some(req),
                        Err(_) => {
                            // Result is not a Request, try getting next via __next__
                            match flow_bound.call_method0("__next__") {
                                Ok(r2) => Some(r2.extract::<Request>()?),
                                Err(e)
                                    if e.is_instance_of::<pyo3::exceptions::PyStopIteration>(
                                        py,
                                    ) =>
                                {
                                    None
                                }
                                Err(e) => return Err(e),
                            }
                        }
                    }
                }
                Err(e) if e.is_instance_of::<pyo3::exceptions::PyStopIteration>(py) => {
                    // Flow is done, check if there's another request via __next__
                    match flow_bound.call_method0("__next__") {
                        Ok(r) => Some(r.extract::<Request>()?),
                        Err(e2) if e2.is_instance_of::<pyo3::exceptions::PyStopIteration>(py) => {
                            None
                        }
                        Err(e2) => return Err(e2),
                    }
                }
                Err(e) => return Err(e),
            };

            if let Some(next) = next_req {
                // There's another request to send
                response.history = history.clone();
                history.push(response);
                current_req = next;
            } else {
                // Auth flow is complete, this is the final response
                response.history = history;

                // Handle redirects
                let should_follow = follow_redirects.unwrap_or(self.follow_redirects);
                if should_follow {
                    return self.handle_redirects(py, start, current_req, response);
                }

                return Ok(response);
            }
        }
    }
}

#[pymethods]
impl Client {
    #[pyo3(signature = (url, *, params=None, headers=None, cookies=None, follow_redirects=None, timeout=None, extensions=None, **kwargs))]
    fn get(
        &self,
        py: Python<'_>,
        url: &Bound<'_, PyAny>,
        params: Option<&Bound<'_, PyAny>>,
        headers: Option<&Bound<'_, PyAny>>,
        cookies: Option<&Bound<'_, PyAny>>,
        follow_redirects: Option<bool>,
        timeout: Option<&Bound<'_, PyAny>>,
        extensions: Option<&Bound<'_, PyAny>>,
        kwargs: Option<&Bound<'_, PyDict>>,
    ) -> PyResult<Response> {
        let timeout_kw = extract_from_kwargs(kwargs, "timeout");
        let t = if let Some(t) = timeout {
            Some(t)
        } else {
            timeout_kw.as_ref()
        };
        let f = if let Some(f) = follow_redirects {
            Some(f)
        } else {
            extract_from_kwargs(kwargs, "follow_redirects").and_then(|v| v.extract::<bool>().ok())
        };
        self.request(
            py, "GET", url, None, None, None, None, params, headers, cookies, f, t, extensions,
            kwargs,
        )
    }

    #[pyo3(signature = (url, *, params=None, headers=None, cookies=None, follow_redirects=None, timeout=None, extensions=None, **kwargs))]
    fn head(
        &self,
        py: Python<'_>,
        url: &Bound<'_, PyAny>,
        params: Option<&Bound<'_, PyAny>>,
        headers: Option<&Bound<'_, PyAny>>,
        cookies: Option<&Bound<'_, PyAny>>,
        follow_redirects: Option<bool>,
        timeout: Option<&Bound<'_, PyAny>>,
        extensions: Option<&Bound<'_, PyAny>>,
        kwargs: Option<&Bound<'_, PyDict>>,
    ) -> PyResult<Response> {
        let timeout_kw = extract_from_kwargs(kwargs, "timeout");
        let t = if let Some(t) = timeout {
            Some(t)
        } else {
            timeout_kw.as_ref()
        };
        let f = if let Some(f) = follow_redirects {
            Some(f)
        } else {
            extract_from_kwargs(kwargs, "follow_redirects").and_then(|v| v.extract::<bool>().ok())
        };
        self.request(
            py, "HEAD", url, None, None, None, None, params, headers, cookies, f, t, extensions,
            kwargs,
        )
    }

    #[pyo3(signature = (url, *, params=None, headers=None, cookies=None, follow_redirects=None, timeout=None, extensions=None, **kwargs))]
    fn options(
        &self,
        py: Python<'_>,
        url: &Bound<'_, PyAny>,
        params: Option<&Bound<'_, PyAny>>,
        headers: Option<&Bound<'_, PyAny>>,
        cookies: Option<&Bound<'_, PyAny>>,
        follow_redirects: Option<bool>,
        timeout: Option<&Bound<'_, PyAny>>,
        extensions: Option<&Bound<'_, PyAny>>,
        kwargs: Option<&Bound<'_, PyDict>>,
    ) -> PyResult<Response> {
        let timeout_kw = extract_from_kwargs(kwargs, "timeout");
        let t = if let Some(t) = timeout {
            Some(t)
        } else {
            timeout_kw.as_ref()
        };
        let f = if let Some(f) = follow_redirects {
            Some(f)
        } else {
            extract_from_kwargs(kwargs, "follow_redirects").and_then(|v| v.extract::<bool>().ok())
        };
        self.request(
            py, "OPTIONS", url, None, None, None, None, params, headers, cookies, f, t, extensions,
            kwargs,
        )
    }

    #[pyo3(signature = (url, *, content=None, data=None, files=None, json=None, params=None, headers=None, cookies=None, follow_redirects=None, timeout=None, extensions=None, **kwargs))]
    fn post(
        &self,
        py: Python<'_>,
        url: &Bound<'_, PyAny>,
        content: Option<&Bound<'_, PyAny>>,
        data: Option<&Bound<'_, PyAny>>,
        files: Option<&Bound<'_, PyAny>>,
        json: Option<&Bound<'_, PyAny>>,
        params: Option<&Bound<'_, PyAny>>,
        headers: Option<&Bound<'_, PyAny>>,
        cookies: Option<&Bound<'_, PyAny>>,
        follow_redirects: Option<bool>,
        timeout: Option<&Bound<'_, PyAny>>,
        extensions: Option<&Bound<'_, PyAny>>,
        kwargs: Option<&Bound<'_, PyDict>>,
    ) -> PyResult<Response> {
        let timeout_kw = extract_from_kwargs(kwargs, "timeout");
        let t = if let Some(t) = timeout {
            Some(t)
        } else {
            timeout_kw.as_ref()
        };
        let f = if let Some(f) = follow_redirects {
            Some(f)
        } else {
            extract_from_kwargs(kwargs, "follow_redirects").and_then(|v| v.extract::<bool>().ok())
        };
        self.request(
            py, "POST", url, content, data, files, json, params, headers, cookies, f, t,
            extensions, kwargs,
        )
    }

    #[pyo3(signature = (url, *, content=None, data=None, files=None, json=None, params=None, headers=None, cookies=None, follow_redirects=None, timeout=None, extensions=None, **kwargs))]
    fn put(
        &self,
        py: Python<'_>,
        url: &Bound<'_, PyAny>,
        content: Option<&Bound<'_, PyAny>>,
        data: Option<&Bound<'_, PyAny>>,
        files: Option<&Bound<'_, PyAny>>,
        json: Option<&Bound<'_, PyAny>>,
        params: Option<&Bound<'_, PyAny>>,
        headers: Option<&Bound<'_, PyAny>>,
        cookies: Option<&Bound<'_, PyAny>>,
        follow_redirects: Option<bool>,
        timeout: Option<&Bound<'_, PyAny>>,
        extensions: Option<&Bound<'_, PyAny>>,
        kwargs: Option<&Bound<'_, PyDict>>,
    ) -> PyResult<Response> {
        let timeout_kw = extract_from_kwargs(kwargs, "timeout");
        let t = if let Some(t) = timeout {
            Some(t)
        } else {
            timeout_kw.as_ref()
        };
        let f = if let Some(f) = follow_redirects {
            Some(f)
        } else {
            extract_from_kwargs(kwargs, "follow_redirects").and_then(|v| v.extract::<bool>().ok())
        };
        self.request(
            py, "PUT", url, content, data, files, json, params, headers, cookies, f, t, extensions,
            kwargs,
        )
    }

    #[pyo3(signature = (url, *, content=None, data=None, files=None, json=None, params=None, headers=None, cookies=None, follow_redirects=None, timeout=None, extensions=None, **kwargs))]
    fn patch(
        &self,
        py: Python<'_>,
        url: &Bound<'_, PyAny>,
        content: Option<&Bound<'_, PyAny>>,
        data: Option<&Bound<'_, PyAny>>,
        files: Option<&Bound<'_, PyAny>>,
        json: Option<&Bound<'_, PyAny>>,
        params: Option<&Bound<'_, PyAny>>,
        headers: Option<&Bound<'_, PyAny>>,
        cookies: Option<&Bound<'_, PyAny>>,
        follow_redirects: Option<bool>,
        timeout: Option<&Bound<'_, PyAny>>,
        extensions: Option<&Bound<'_, PyAny>>,
        kwargs: Option<&Bound<'_, PyDict>>,
    ) -> PyResult<Response> {
        let timeout_kw = extract_from_kwargs(kwargs, "timeout");
        let t = if let Some(t) = timeout {
            Some(t)
        } else {
            timeout_kw.as_ref()
        };
        let f = if let Some(f) = follow_redirects {
            Some(f)
        } else {
            extract_from_kwargs(kwargs, "follow_redirects").and_then(|v| v.extract::<bool>().ok())
        };
        self.request(
            py, "PATCH", url, content, data, files, json, params, headers, cookies, f, t,
            extensions, kwargs,
        )
    }

    #[pyo3(signature = (url, *, params=None, headers=None, cookies=None, follow_redirects=None, timeout=None, extensions=None, **kwargs))]
    fn delete(
        &self,
        py: Python<'_>,
        url: &Bound<'_, PyAny>,
        params: Option<&Bound<'_, PyAny>>,
        headers: Option<&Bound<'_, PyAny>>,
        cookies: Option<&Bound<'_, PyAny>>,
        follow_redirects: Option<bool>,
        timeout: Option<&Bound<'_, PyAny>>,
        extensions: Option<&Bound<'_, PyAny>>,
        kwargs: Option<&Bound<'_, PyDict>>,
    ) -> PyResult<Response> {
        let timeout_kw = extract_from_kwargs(kwargs, "timeout");
        let t = if let Some(t) = timeout {
            Some(t)
        } else {
            timeout_kw.as_ref()
        };
        let f = if let Some(f) = follow_redirects {
            Some(f)
        } else {
            extract_from_kwargs(kwargs, "follow_redirects").and_then(|v| v.extract::<bool>().ok())
        };
        self.request(
            py, "DELETE", url, None, None, None, None, params, headers, cookies, f, t, extensions,
            kwargs,
        )
    }

    // ── Fast-path raw API ──────────────────────────────────────────────
    // These bypass httpx-compatible Request/Response construction entirely.
    // Returns (status_code: int, headers: dict[str, str], body: bytes).

    #[pyo3(signature = (url, *, headers=None, timeout=None))]
    fn get_raw(
        &self,
        py: Python<'_>,
        url: &str,
        headers: Option<&Bound<'_, PyDict>>,
        timeout: Option<f64>,
    ) -> PyResult<Py<PyAny>> {
        self._send_raw(py, reqwest::Method::GET, url, headers, None, timeout)
    }

    #[pyo3(signature = (url, *, headers=None, body=None, timeout=None))]
    fn post_raw(
        &self,
        py: Python<'_>,
        url: &str,
        headers: Option<&Bound<'_, PyDict>>,
        body: Option<Vec<u8>>,
        timeout: Option<f64>,
    ) -> PyResult<Py<PyAny>> {
        self._send_raw(py, reqwest::Method::POST, url, headers, body, timeout)
    }

    #[pyo3(signature = (url, *, headers=None, body=None, timeout=None))]
    fn put_raw(
        &self,
        py: Python<'_>,
        url: &str,
        headers: Option<&Bound<'_, PyDict>>,
        body: Option<Vec<u8>>,
        timeout: Option<f64>,
    ) -> PyResult<Py<PyAny>> {
        self._send_raw(py, reqwest::Method::PUT, url, headers, body, timeout)
    }

    #[pyo3(signature = (url, *, headers=None, body=None, timeout=None))]
    fn patch_raw(
        &self,
        py: Python<'_>,
        url: &str,
        headers: Option<&Bound<'_, PyDict>>,
        body: Option<Vec<u8>>,
        timeout: Option<f64>,
    ) -> PyResult<Py<PyAny>> {
        self._send_raw(py, reqwest::Method::PATCH, url, headers, body, timeout)
    }

    #[pyo3(signature = (url, *, headers=None, timeout=None))]
    fn delete_raw(
        &self,
        py: Python<'_>,
        url: &str,
        headers: Option<&Bound<'_, PyDict>>,
        timeout: Option<f64>,
    ) -> PyResult<Py<PyAny>> {
        self._send_raw(py, reqwest::Method::DELETE, url, headers, None, timeout)
    }

    #[pyo3(signature = (url, *, headers=None, timeout=None))]
    fn head_raw(
        &self,
        py: Python<'_>,
        url: &str,
        headers: Option<&Bound<'_, PyDict>>,
        timeout: Option<f64>,
    ) -> PyResult<Py<PyAny>> {
        self._send_raw(py, reqwest::Method::HEAD, url, headers, None, timeout)
    }

    fn close(&mut self) -> PyResult<()> {
        self.is_closed = true;
        Ok(())
    }

    #[getter]
    fn is_closed(&self) -> bool {
        self.is_closed
    }

    #[getter]
    fn headers(&self, py: Python<'_>) -> Py<Headers> {
        self.default_headers.clone_ref(py)
    }
    #[setter]
    fn set_headers(&mut self, py: Python<'_>, value: &Bound<'_, PyAny>) -> PyResult<()> {
        let hdrs = Headers::create(Some(value), "utf-8")?;
        self.default_headers = Py::new(py, hdrs)?;
        Ok(())
    }

    #[getter]
    fn get_base_url(&self, py: Python<'_>) -> PyResult<Py<PyAny>> {
        if let Some(ref u) = self.base_url {
            Ok(Py::new(py, u.clone())?.into())
        } else {
            Ok(Py::new(py, URL::create_from_str("")?)?.into())
        }
    }
    #[setter]
    fn set_base_url(&mut self, value: &Bound<'_, PyAny>) -> PyResult<()> {
        let s = value.str()?.extract::<String>()?;
        self.base_url = if s.is_empty() {
            None
        } else {
            Some(URL::create_from_str(&s)?)
        };
        Ok(())
    }

    #[getter]
    fn get_auth(&self, py: Python<'_>) -> Option<Py<PyAny>> {
        self.auth.as_ref().map(|a| a.clone_ref(py))
    }
    #[setter]
    fn set_auth(&mut self, py: Python<'_>, value: Option<&Bound<'_, PyAny>>) -> PyResult<()> {
        if let Some(v) = value {
            if v.is_none() {
                self.auth = None;
            } else {
                validate_auth_type(py, v)?;
                self.auth = Some(coerce_auth(py, v)?);
            }
        } else {
            self.auth = None;
        }
        Ok(())
    }

    #[getter]
    fn get_cookies(&self, _py: Python<'_>) -> Cookies {
        self.cookies.clone()
    }
    #[setter]
    fn set_cookies(&mut self, py: Python<'_>, value: &Bound<'_, PyAny>) -> PyResult<()> {
        self.cookies = Cookies::create(py, Some(value))?;
        Ok(())
    }

    #[getter]
    fn get_timeout(&self, py: Python<'_>) -> Py<PyAny> {
        self.timeout
            .as_ref()
            .map(|t| t.clone_ref(py))
            .unwrap_or_else(|| py.None())
    }
    #[setter]
    fn set_timeout(&mut self, py: Python<'_>, value: &Bound<'_, PyAny>) -> PyResult<()> {
        let timeout = Timeout::new(py, Some(value), None, None, None, None)?;
        self.timeout = Some(Py::new(py, timeout)?.into());
        Ok(())
    }

    #[getter]
    fn get_event_hooks(&self, py: Python<'_>) -> Py<PyAny> {
        self.event_hooks
            .as_ref()
            .map(|e| e.clone_ref(py))
            .unwrap_or_else(|| {
                let d = PyDict::new(py);
                let _ = d.set_item("request", pyo3::types::PyList::empty(py));
                let _ = d.set_item("response", pyo3::types::PyList::empty(py));
                d.into()
            })
    }
    #[setter]
    fn set_event_hooks(&mut self, py: Python<'_>, value: &Bound<'_, PyAny>) {
        if let Ok(d) = value.cast::<PyDict>() {
            if d.get_item("request").ok().flatten().is_none() {
                let _ = d.set_item("request", pyo3::types::PyList::empty(py));
            }
            if d.get_item("response").ok().flatten().is_none() {
                let _ = d.set_item("response", pyo3::types::PyList::empty(py));
            }
        }
        self.event_hooks = Some(value.clone().unbind());
    }

    #[getter]
    fn get_trust_env(&self) -> bool {
        self.trust_env
    }

    #[pyo3(signature = (method, url, *, content=None, data=None, files=None, json=None, params=None, headers=None, cookies=None, extensions=None))]
    #[allow(unused_variables)]
    fn build_request(
        &self,
        py: Python<'_>,
        method: &str,
        url: &Bound<'_, PyAny>,
        content: Option<&Bound<'_, PyAny>>,
        data: Option<&Bound<'_, PyAny>>,
        files: Option<&Bound<'_, PyAny>>,
        json: Option<&Bound<'_, PyAny>>,
        params: Option<&Bound<'_, PyAny>>,
        headers: Option<&Bound<'_, PyAny>>,
        cookies: Option<&Bound<'_, PyAny>>,
        extensions: Option<&Bound<'_, PyAny>>,
    ) -> PyResult<Request> {
        let url_str = url.str()?.extract::<String>()?;
        let target_url = if let Some(ref base) = self.base_url {
            merge_base_url(base, &url_str)?
        } else {
            URL::create_from_str(&url_str)?
        };

        let mut merged_headers = self.default_headers.bind(py).borrow().clone();
        if let Some(h) = headers {
            merged_headers.update_from(Some(h))?;
        }

        let body = if let Some(c) = content {
            if let Ok(b) = c.cast::<PyBytes>() {
                Some(b.as_bytes().to_vec())
            } else if let Ok(s) = c.extract::<String>() {
                Some(s.into_bytes())
            } else {
                None
            }
        } else if let Some(j) = json {
            let json_mod = py.import("json")?;
            let s: String = json_mod.call_method1("dumps", (j,))?.extract()?;
            merged_headers.set_header("content-type", "application/json");
            Some(s.into_bytes())
        } else if let Some(d) = data {
            let urllib = py.import("urllib.parse")?;
            let s: String = urllib.call_method1("urlencode", (d, true))?.extract()?;
            merged_headers.set_header("content-type", "application/x-www-form-urlencoded");
            Some(s.into_bytes())
        } else {
            None
        };

        // For POST/PUT/PATCH with no body, set content-length: 0
        let method_upper = method.to_uppercase();
        if body.is_none()
            && content.is_none()
            && data.is_none()
            && files.is_none()
            && json.is_none()
            && ["POST", "PUT", "PATCH"].contains(&method_upper.as_str())
        {
            if !merged_headers.contains_header("content-length")
                && !merged_headers.contains_header("transfer-encoding")
            {
                merged_headers.set_header("content-length", "0");
            }
        }

        let final_headers = arrange_headers_httpx_order(&merged_headers, &target_url);

        Ok(Request {
            method: method_upper,
            url: target_url,
            headers: Py::new(py, final_headers)?,
            extensions: if let Some(e) = extensions {
                e.clone().unbind()
            } else {
                PyDict::new(py).into()
            },
            content_body: body,
            stream: None,
            stream_response: false,
        })
    }

    /// Dispatch multiple requests concurrently using Rust's tokio runtime.
    /// Returns a list of Response objects (or exceptions if return_exceptions=True).
    /// This is an httpxr extension — not available in httpx.
    #[pyo3(signature = (requests, *, max_concurrency=10, return_exceptions=false))]
    fn gather(
        &self,
        py: Python<'_>,
        requests: Vec<Py<Request>>,
        max_concurrency: usize,
        return_exceptions: bool,
    ) -> PyResult<Py<pyo3::types::PyList>> {
        if self.is_closed {
            return Err(pyo3::exceptions::PyRuntimeError::new_err(
                "Cannot send requests, as the client has been closed.",
            ));
        }
        if requests.is_empty() {
            return Ok(pyo3::types::PyList::empty(py).unbind());
        }

        // Extract Request objects
        let request_refs: Vec<Request> = requests
            .iter()
            .map(|r| {
                let bound = r.bind(py);
                let borrowed = bound.borrow();
                borrowed.clone()
            })
            .collect();

        // Try to downcast transport to HTTPTransport for batch dispatch
        let transport_bound = self.transport.bind(py);
        if let Ok(transport) = transport_bound.cast::<crate::transports::default::HTTPTransport>() {
            let transport_ref = transport.borrow();
            let results = transport_ref.send_batch_requests(py, &request_refs, max_concurrency)?;

            let py_list = pyo3::types::PyList::empty(py);
            for (i, result) in results.into_iter().enumerate() {
                match result {
                    Ok(mut response) => {
                        response.request = Some(request_refs[i].clone());
                        if let Some(ref de) = self.default_encoding {
                            response.default_encoding = de.clone_ref(py);
                        }
                        py_list.append(Py::new(py, response)?)?;
                    }
                    Err(err) => {
                        if return_exceptions {
                            let err_str = format!("{}", err);
                            py_list.append(pyo3::exceptions::PyRuntimeError::new_err(err_str).value(py))?;
                        } else {
                            return Err(err);
                        }
                    }
                }
            }
            Ok(py_list.unbind())
        } else {
            // Fallback: non-native transport, send sequentially
            let py_list = pyo3::types::PyList::empty(py);
            for req in &request_refs {
                let req_py = Py::new(py, req.clone())?;
                match transport_bound.call_method1("handle_request", (req_py,)) {
                    Ok(res_py) => {
                        py_list.append(res_py)?;
                    }
                    Err(err) => {
                        if return_exceptions {
                            let err_str = format!("{}", err);
                            py_list.append(pyo3::exceptions::PyRuntimeError::new_err(err_str).value(py))?;
                        } else {
                            return Err(err);
                        }
                    }
                }
            }
            Ok(py_list.unbind())
        }
    }

    /// Auto-follow pagination links, returning a lazy iterator.
    /// Each iteration fetches the next page — memory-efficient for large result sets.
    /// Supports JSON key extraction, Link header parsing, and custom callables.
    /// This is an httpxr extension — not available in httpx.
    ///
    /// Usage:
    ///   for page in client.paginate("GET", url, next_header="link"):
    ///       process(page.json())
    #[pyo3(signature = (method, url, *, next_url=None, next_header=None, next_func=None, max_pages=100, params=None, headers=None, cookies=None, timeout=None, extensions=None, **kwargs))]
    #[allow(clippy::too_many_arguments)]
    fn paginate(
        slf: &Bound<'_, Self>,
        _py: Python<'_>,
        method: &str,
        url: &Bound<'_, PyAny>,
        next_url: Option<&str>,
        next_header: Option<&str>,
        next_func: Option<&Bound<'_, PyAny>>,
        max_pages: usize,
        params: Option<&Bound<'_, PyAny>>,
        headers: Option<&Bound<'_, PyAny>>,
        cookies: Option<&Bound<'_, PyAny>>,
        timeout: Option<&Bound<'_, PyAny>>,
        extensions: Option<&Bound<'_, PyAny>>,
        kwargs: Option<&Bound<'_, PyDict>>,
    ) -> PyResult<PageIterator> {
        let this = slf.borrow();
        if this.is_closed {
            return Err(pyo3::exceptions::PyRuntimeError::new_err(
                "Cannot send requests, as the client has been closed.",
            ));
        }
        if next_url.is_none() && next_header.is_none() && next_func.is_none() {
            return Err(pyo3::exceptions::PyValueError::new_err(
                "Must specify one of: next_url, next_header, or next_func",
            ));
        }

        Ok(PageIterator {
            client: slf.clone().unbind().into_any(),
            method: method.to_string(),
            current_url: Some(url.clone().unbind()),
            next_url_key: next_url.map(|s| s.to_string()),
            next_header_name: next_header.map(|s| s.to_string()),
            next_func: next_func.map(|f| f.clone().unbind()),
            max_pages,
            page_count: 0,
            done: false,
            params: params.map(|p| p.clone().unbind()),
            headers: headers.map(|h| h.clone().unbind()),
            cookies: cookies.map(|c| c.clone().unbind()),
            timeout: timeout.map(|t| t.clone().unbind()),
            extensions: extensions.map(|e| e.clone().unbind()),
            _kwargs: kwargs.map(|k| k.clone().unbind()),
        })
    }

    #[pyo3(signature = (request, *, auth=None, follow_redirects=None))]
    fn send(
        &self,
        py: Python<'_>,
        request: &Request,
        auth: Option<&Bound<'_, PyAny>>,
        follow_redirects: Option<bool>,
    ) -> PyResult<Response> {
        let _ = auth;
        let start = Instant::now();
        let t_bound = self.transport.bind(py);
        let mut response: Response = if let Ok(t) = t_bound.cast::<crate::transports::default::HTTPTransport>() {
            t.borrow().send_request(py, request)?
        } else {
            let req_py = Py::new(py, request.clone())?;
            let res_py = t_bound.call_method1("handle_request", (req_py,))?;
            res_py.extract()?
        };
        response.elapsed = Some(start.elapsed().as_secs_f64());
        response.request = Some(request.clone());
        let should_follow = follow_redirects.unwrap_or(self.follow_redirects);
        if should_follow && response.has_redirect_check(py) {
            let mut redirect_count = 0u32;
            let mut current_response = response;
            let mut current_request = request.clone();
            while current_response.has_redirect_check(py) && redirect_count < self.max_redirects {
                redirect_count += 1;
                let location = current_response
                    .headers
                    .bind(py)
                    .borrow()
                    .get_first_value("location")
                    .ok_or_else(|| {
                        pyo3::exceptions::PyValueError::new_err("Redirect without Location header")
                    })?;
                let redirect_url = current_request.url.join_relative(&location)?;
                let redirect_method = if current_response.status_code == 303 {
                    "GET".to_string()
                } else {
                    current_request.method.clone()
                };
                let redirect_body = if redirect_method == "GET" {
                    None
                } else {
                    current_request.content_body.clone()
                };
                let redirect_request = Request {
                    method: redirect_method,
                    url: redirect_url,
                    headers: current_request.headers.clone_ref(py),
                    extensions: PyDict::new(py).into(),
                    content_body: redirect_body,
                    stream: None,
                    stream_response: false,
                };
                let mut history = current_response.history.clone();
                history.push(current_response);
                let mut new_response = if let Ok(t) = t_bound.cast::<crate::transports::default::HTTPTransport>() {
                    t.borrow().send_request(py, &redirect_request)?
                } else {
                    let req_py2 = Py::new(py, redirect_request.clone())?;
                    let res_py2 = t_bound.call_method1("handle_request", (req_py2,))?;
                    res_py2.extract::<Response>()?
                };
                new_response.elapsed = Some(start.elapsed().as_secs_f64());
                new_response.request = Some(redirect_request.clone());
                new_response.history = history;
                current_response = new_response;
                current_request = redirect_request;
            }
            if current_response.has_redirect_check(py) && redirect_count >= self.max_redirects {
                return Err(crate::exceptions::TooManyRedirects::new_err(format!(
                    "Exceeded maximum number of redirects ({})",
                    self.max_redirects
                )));
            }
            return Ok(current_response);
        }
        Ok(response)
    }

    fn _transport_for_url(&self, py: Python<'_>, url: &Bound<'_, PyAny>) -> PyResult<Py<PyAny>> {
        let url_str = url.str()?.extract::<String>().unwrap_or_default();

        // Check mounts with proper pattern matching
        let mut best_transport: Option<Py<PyAny>> = None;
        let mut best_priority = 0u32;

        for (pattern, transport) in &self.mounts {
            if url_matches_pattern(&url_str, pattern) {
                let priority = match (
                    !pattern.starts_with("all://"),
                    pattern.split("://").nth(1).map_or(false, |r| !r.is_empty()),
                ) {
                    (true, true) => 4,
                    (false, true) => 3,
                    (true, false) => 2,
                    (false, false) => 1,
                };
                if priority > best_priority {
                    best_priority = priority;
                    best_transport = Some(transport.clone_ref(py));
                }
            }
        }

        if let Some(t) = best_transport {
            return Ok(t);
        }

        // Check environment proxy if trust_env is true
        if self.trust_env {
            if let Some(proxy_url) = get_env_proxy_url(&url_str) {
                let transport = crate::transports::default::HTTPTransport::create(
                    true,
                    None,
                    false,
                    None,
                    Some(&crate::config::Proxy {
                        url: proxy_url,
                        auth: None,
                        headers: None,
                    }),
                    0,
                )?;
                return Ok(Py::new(py, transport)?.into());
            }
        }

        Ok(self.transport.clone_ref(py))
    }

    #[getter]
    fn _transport(&self, py: Python<'_>) -> Py<PyAny> {
        self.transport.clone_ref(py)
    }

    #[getter]
    fn _mounts(&self, py: Python<'_>) -> PyResult<Py<PyAny>> {
        let dict = PyDict::new(py);
        for (pattern, transport) in &self.mounts {
            dict.set_item(pattern, transport.clone_ref(py))?;
        }
        Ok(dict.into())
    }

    #[getter]
    fn get_params(&self, py: Python<'_>) -> PyResult<Py<PyAny>> {
        if let Some(ref p) = self.params {
            // Always return a QueryParams instance, converting if needed
            let bound = p.bind(py);
            if bound.is_instance_of::<crate::urls::QueryParams>() {
                Ok(p.clone_ref(py))
            } else {
                let qp = crate::urls::QueryParams::create(Some(bound))?;
                Ok(Py::new(py, qp)?.into())
            }
        } else {
            let qp = crate::urls::QueryParams::create(None)?;
            Ok(Py::new(py, qp)?.into())
        }
    }

    #[setter]
    fn set_params(&mut self, py: Python<'_>, value: &Bound<'_, PyAny>) -> PyResult<()> {
        if value.is_none() {
            self.params = None;
        } else {
            let qp = crate::urls::QueryParams::create(Some(value))?;
            self.params = Some(Py::new(py, qp)?.into());
        }
        Ok(())
    }

    fn _redirect_headers(
        &self,
        request: &Request,
        url: &Bound<'_, PyAny>,
        _method: &str,
    ) -> PyResult<Headers> {
        Python::attach(|py| {
            let mut hdrs = request.headers.bind(py).borrow().clone();

            // Check if this is a same-origin or HTTPS upgrade redirect
            let request_url = &request.url;
            let redirect_url = if let Ok(u) = url.extract::<URL>() {
                u
            } else {
                let s = url.str()?.extract::<String>()?;
                URL::create_from_str(&s)?
            };

            let req_host = request_url.get_host().to_lowercase();
            let red_host = redirect_url.get_host().to_lowercase();
            let req_scheme = request_url.get_scheme().to_lowercase();
            let red_scheme = redirect_url.get_scheme().to_lowercase();
            let req_port = request_url.get_port();
            let red_port = redirect_url.get_port();

            // Determine if this is a same-origin redirect
            let is_same_host = req_host == red_host;
            let is_same_port = req_port == red_port;
            // HTTPS upgrade: same host, http -> https, both on default ports
            let is_https_upgrade = is_same_host
                && req_scheme == "http"
                && red_scheme == "https"
                && (req_port.is_none() || req_port == Some(80))
                && (red_port.is_none() || red_port == Some(443));
            let is_same_origin = is_same_host && is_same_port && req_scheme == red_scheme;

            if !is_same_origin && !is_https_upgrade {
                hdrs.remove_header("authorization");
            }

            // Update host header
            hdrs.set_header("host", &build_host_header(&redirect_url));

            Ok(hdrs)
        })
    }

    #[pyo3(signature = (method, url, *, content=None, data=None, files=None, json=None, params=None, headers=None, cookies=None, follow_redirects=None, timeout=None, extensions=None, **kwargs))]
    fn stream(
        &self,
        py: Python<'_>,
        method: &str,
        url: &Bound<'_, PyAny>,
        content: Option<&Bound<'_, PyAny>>,
        data: Option<&Bound<'_, PyAny>>,
        files: Option<&Bound<'_, PyAny>>,
        json: Option<&Bound<'_, PyAny>>,
        params: Option<&Bound<'_, PyAny>>,
        headers: Option<&Bound<'_, PyAny>>,
        cookies: Option<&Bound<'_, PyAny>>,
        follow_redirects: Option<bool>,
        timeout: Option<&Bound<'_, PyAny>>,
        extensions: Option<&Bound<'_, PyAny>>,
        kwargs: Option<&Bound<'_, PyDict>>,
    ) -> PyResult<Response> {
        let timeout_kw = extract_from_kwargs(kwargs, "timeout");
        let t = if let Some(t) = timeout {
            Some(t)
        } else {
            timeout_kw.as_ref()
        };
        let f = if let Some(f) = follow_redirects {
            Some(f)
        } else {
            extract_from_kwargs(kwargs, "follow_redirects").and_then(|v| v.extract::<bool>().ok())
        };
        self.request(
            py, method, url, content, data, files, json, params, headers, cookies, f, t,
            extensions, kwargs,
        )
    }

    fn __enter__<'py>(slf: PyRef<'py, Self>, py: Python<'py>) -> PyResult<PyRef<'py, Self>> {
        if slf.is_closed {
            return Err(pyo3::exceptions::PyRuntimeError::new_err(
                "Cannot open a client instance that has been closed.",
            ));
        }
        let transport = slf.transport.bind(py);
        if transport.hasattr("__enter__")? {
            transport.call_method0("__enter__")?;
        }
        for (_, mt) in &slf.mounts {
            let t = mt.bind(py);
            if t.hasattr("__enter__")? {
                t.call_method0("__enter__")?;
            }
        }
        Ok(slf)
    }

    fn __exit__(
        &mut self,
        py: Python<'_>,
        _e1: Option<&Bound<'_, PyAny>>,
        _e2: Option<&Bound<'_, PyAny>>,
        _e3: Option<&Bound<'_, PyAny>>,
    ) -> PyResult<()> {
        self.close()?;
        // Call close() on transport explicitly, then __exit__ (proper lifecycle ordering)
        let transport = self.transport.bind(py);
        if transport.hasattr("close")? {
            transport.call_method0("close")?;
        }
        if transport.hasattr("__exit__")? {
            let none = py.None();
            transport.call_method1("__exit__", (none.bind(py), none.bind(py), none.bind(py)))?;
        }
        for (_, mt) in &self.mounts {
            let t = mt.bind(py);
            if t.hasattr("close")? {
                t.call_method0("close")?;
            }
            if t.hasattr("__exit__")? {
                let none = py.None();
                t.call_method1("__exit__", (none.bind(py), none.bind(py), none.bind(py)))?;
            }
        }
        Ok(())
    }

    fn __repr__(&self) -> String {
        if self.is_closed {
            "<Client [closed]>".to_string()
        } else {
            "<Client>".to_string()
        }
    }
}

/// Private implementation helpers (not exposed to Python).
impl Client {
    /// Select the correct transport for a URL, checking mounts first
    fn select_transport_for_url(&self, py: Python<'_>, url: &URL) -> Py<PyAny> {
        let url_str = url.to_string();
        let mut best_transport: Option<Py<PyAny>> = None;
        let mut best_priority = 0u32;

        for (pattern, transport) in &self.mounts {
            if url_matches_pattern(&url_str, pattern) {
                let priority = match (
                    !pattern.starts_with("all://"),
                    pattern.split("://").nth(1).map_or(false, |r| !r.is_empty()),
                ) {
                    (true, true) => 4,
                    (false, true) => 3,
                    (true, false) => 2,
                    (false, false) => 1,
                };
                if priority > best_priority {
                    best_priority = priority;
                    best_transport = Some(transport.clone_ref(py));
                }
            }
        }

        if let Some(t) = best_transport {
            return t;
        }

        self.transport.clone_ref(py)
    }

    fn handle_redirects(
        &self,
        py: Python<'_>,
        start: Instant,
        request: Request,
        response: Response,
    ) -> PyResult<Response> {
        let mut redirect_count = 0u32;
        let mut current_response = response;
        let mut current_request = request;

        // Event hooks: response for the initial response
        if let Some(hooks) = &self.event_hooks {
            if let Ok(l) = hooks.bind(py).get_item("response") {
                if let Ok(l) = l.cast::<pyo3::types::PyList>() {
                    let resp_py = Py::new(py, current_response.clone())?;
                    for hook in l.iter() {
                        hook.call1((resp_py.bind(py),))?;
                    }
                }
            }
        }

        while current_response.has_redirect_check(py) && redirect_count < self.max_redirects {
            redirect_count += 1;
            let location = current_response
                .headers
                .bind(py)
                .borrow()
                .get_first_value("location")
                .ok_or_else(|| {
                    pyo3::exceptions::PyValueError::new_err("Redirect without Location header")
                })?;

            let mut redirect_url = current_request
                .url
                .join_relative(&location)
                .map_err(|e| crate::exceptions::RemoteProtocolError::new_err(e.to_string()))?;

            // Fragment preservation
            if redirect_url.parsed.fragment.is_none() {
                redirect_url.parsed.fragment = current_request.url.parsed.fragment.clone();
            }

            let redirect_method = if current_response.status_code == 303 {
                "GET".to_string()
            } else {
                current_request.method.clone()
            };

            // Check for streaming body
            if redirect_method != "GET" && redirect_method != "HEAD" {
                if current_request.stream.is_some() {
                    return Err(crate::exceptions::StreamConsumed::new_err(
                        "Cannot redirect request with streaming body",
                    ));
                }
            }

            let redirect_body = if redirect_method == "GET" {
                None
            } else {
                current_request.content_body.clone()
            };

            // Update cookies from response
            extract_cookies_to_jar(py, &current_response, self.cookies.jar.bind(py))?;

            let mut redirect_headers = current_request.headers.bind(py).borrow().clone();
            redirect_headers.remove_header("cookie");

            // Get fresh cookie header for redirect_url
            let cookie_header_val = {
                let locals = pyo3::types::PyDict::new(py);
                locals.set_item("jar", self.cookies.jar.bind(py))?;
                locals.set_item("url", redirect_url.to_string())?;
                let code = std::ffi::CString::new("import urllib.request; req = urllib.request.Request(url); jar.add_cookie_header(req); c = req.get_header('Cookie')").expect("CString new failed");
                py.run(&code, None, Some(&locals))?;
                if let Some(item) = locals.get_item("c")? {
                    item.extract::<Option<String>>()?
                } else {
                    None
                }
            };

            if let Some(c) = cookie_header_val {
                redirect_headers.set_header("cookie", &c);
            }

            // Preserve body/content headers for 307/308
            if (current_response.status_code == 307 || current_response.status_code == 308)
                && redirect_method != "GET"
                && redirect_method != "HEAD"
            {
                if let Some(ref body) = redirect_body {
                    redirect_headers.set_header("Content-Length", &body.len().to_string());
                    if !redirect_headers.contains_header("Content-Type") {
                        if let Some(ct) = current_request
                            .headers
                            .bind(py)
                            .borrow()
                            .get_first_value("content-type")
                        {
                            redirect_headers.set_header("Content-Type", &ct);
                        }
                    }
                }
            } else if redirect_method == "GET" {
                redirect_headers.remove_header("Content-Length");
                redirect_headers.remove_header("Content-Type");
                redirect_headers.remove_header("Transfer-Encoding");
            }

            // Strip authorization on cross-origin redirect
            let current_host = current_request.url.get_raw_host().to_lowercase();
            let redirect_host = redirect_url.get_raw_host().to_lowercase();
            if current_host != redirect_host {
                redirect_headers.remove_header("authorization");
            }

            // Update host header
            redirect_headers.set_header("host", &build_host_header(&redirect_url));

            let redirect_request = Request {
                method: redirect_method,
                url: redirect_url,
                headers: Py::new(py, redirect_headers)?,
                extensions: PyDict::new(py).into(),
                content_body: redirect_body,
                stream: None,
                stream_response: false,
            };

            let mut history = current_response.history.clone();
            history.push(current_response);

            // Event hooks: request (redirect)
            if let Some(hooks) = &self.event_hooks {
                if let Ok(l) = hooks.bind(py).get_item("request") {
                    if let Ok(l) = l.cast::<pyo3::types::PyList>() {
                        let req_py = Py::new(py, redirect_request.clone())?;
                        for hook in l.iter() {
                            hook.call1((req_py.bind(py),))?;
                        }
                    }
                }
            }

            let mut new_response = {
                let t_bound = self.transport.bind(py);
                if let Ok(t) = t_bound.cast::<crate::transports::default::HTTPTransport>() {
                    t.borrow().send_request(py, &redirect_request)?
                } else {
                    let req_py = Py::new(py, redirect_request.clone())?;
                    let res_py = t_bound.call_method1("handle_request", (req_py,))?;
                    res_py.extract::<Response>()?
                }
            };

            // Event hooks: response (redirect)
            if let Some(hooks) = &self.event_hooks {
                if let Ok(l) = hooks.bind(py).get_item("response") {
                    if let Ok(l) = l.cast::<pyo3::types::PyList>() {
                        let resp_py = Py::new(py, new_response.clone())?;
                        for hook in l.iter() {
                            hook.call1((resp_py.bind(py),))?;
                        }
                    }
                }
            }
            new_response.elapsed = Some(start.elapsed().as_secs_f64());
            new_response.request = Some(redirect_request.clone());
            new_response.history = history;

            // Log the redirect request/response
            {
                let method = &redirect_request.method;
                let url = redirect_request.url.to_string();
                let http_version = {
                    let ext = new_response.extensions.bind(py);
                    if let Ok(d) = ext.cast::<PyDict>() {
                        d.get_item("http_version")
                            .ok()
                            .flatten()
                            .and_then(|v| v.extract::<Vec<u8>>().ok())
                            .map(|b| String::from_utf8_lossy(&b).to_string())
                            .unwrap_or_else(|| "HTTP/1.1".to_string())
                    } else {
                        "HTTP/1.1".to_string()
                    }
                };
                let status = new_response.status_code;
                let reason = new_response.reason_phrase();
                log::info!(target: "httpxr", "HTTP Request: {} {} \"{} {} {}\"", method, url, http_version, status, reason);
            }

            current_response = new_response;
            current_request = redirect_request;
        }

        if current_response.has_redirect_check(py) && redirect_count >= self.max_redirects {
            return Err(crate::exceptions::TooManyRedirects::new_err(format!(
                "Exceeded maximum number of redirects ({})",
                self.max_redirects
            )));
        }

        Ok(current_response)
    }

    /// Internal fast-path dispatcher: pass Python args directly to transport.
    fn _send_raw(
        &self,
        py: Python<'_>,
        method: reqwest::Method,
        url: &str,
        headers: Option<&Bound<'_, PyDict>>,
        body: Option<Vec<u8>>,
        timeout: Option<f64>,
    ) -> PyResult<Py<PyAny>> {
        let t_bound = self.transport.bind(py);
        let transport = t_bound
            .cast::<crate::transports::default::HTTPTransport>()
            .map_err(|_| {
                pyo3::exceptions::PyRuntimeError::new_err(
                    "Raw API requires the default HTTPTransport",
                )
            })?;

        transport.borrow().send_raw(py, method, url, headers, body, timeout)
    }
}

/// Lazy iterator that fetches one page per `__next__()` call.
/// Created by `Client.paginate()`.
#[pyclass]
pub struct PageIterator {
    client: Py<PyAny>,
    method: String,
    current_url: Option<Py<PyAny>>,
    next_url_key: Option<String>,
    next_header_name: Option<String>,
    next_func: Option<Py<PyAny>>,
    max_pages: usize,
    page_count: usize,
    done: bool,
    params: Option<Py<PyAny>>,
    headers: Option<Py<PyAny>>,
    cookies: Option<Py<PyAny>>,
    timeout: Option<Py<PyAny>>,
    extensions: Option<Py<PyAny>>,
    _kwargs: Option<Py<PyDict>>,
}

#[pymethods]
impl PageIterator {
    fn __iter__(slf: PyRef<'_, Self>) -> PyRef<'_, Self> {
        slf
    }

    fn __next__(&mut self, py: Python<'_>) -> PyResult<Option<Response>> {
        if self.done || self.page_count >= self.max_pages {
            return Ok(None);
        }

        let current_url = match &self.current_url {
            Some(url) => url.clone_ref(py),
            None => {
                self.done = true;
                return Ok(None);
            }
        };

        // Call client.request() to fetch the page
        let client_bound = self.client.bind(py);

        // Only pass params on the first page
        let params_arg = if self.page_count == 0 {
            self.params.as_ref().map(|p| p.bind(py).clone())
        } else {
            None
        };

        let response: Response = client_bound
            .call_method(
                "request",
                (self.method.as_str(), current_url.bind(py)),
                Some(
                    &{
                        let kw = PyDict::new(py);
                        if let Some(ref p) = params_arg {
                            kw.set_item("params", p)?;
                        }
                        if let Some(ref h) = self.headers {
                            kw.set_item("headers", h)?;
                        }
                        if let Some(ref c) = self.cookies {
                            kw.set_item("cookies", c)?;
                        }
                        if let Some(ref t) = self.timeout {
                            kw.set_item("timeout", t)?;
                        }
                        if let Some(ref e) = self.extensions {
                            kw.set_item("extensions", e)?;
                        }
                        kw
                    },
                ),
            )?
            .extract()?;

        self.page_count += 1;

        // Extract next URL from response
        let next: Option<String> = if let Some(ref json_key) = self.next_url_key {
            let resp_py = Py::new(py, response.clone())?;
            let json_val = resp_py.call_method0(py, "json")?;
            let json_bound = json_val.bind(py);
            if let Ok(val) = json_bound.get_item(json_key.as_str()) {
                if !val.is_none() {
                    val.extract::<String>().ok()
                } else {
                    None
                }
            } else {
                None
            }
        } else if let Some(ref header_name) = self.next_header_name {
            let hdrs = response.headers.bind(py).borrow();
            if let Some(header_val) = hdrs.get_first_value(header_name) {
                parse_link_next(&header_val)
            } else {
                None
            }
        } else if let Some(ref func) = self.next_func {
            let resp_py = Py::new(py, response.clone())?;
            let result = func.call1(py, (resp_py,))?;
            let result_bound = result.bind(py);
            if result_bound.is_none() {
                None
            } else {
                result_bound.extract::<String>().ok()
            }
        } else {
            None
        };

        // Update URL for next iteration
        match next {
            Some(next_url_str) => {
                self.current_url =
                    Some(pyo3::types::PyString::new(py, &next_url_str).into_any().unbind());
            }
            None => {
                self.done = true;
            }
        }

        Ok(Some(response))
    }

    /// Collect all remaining pages into a list (convenience method).
    fn collect(&mut self, py: Python<'_>) -> PyResult<Vec<Response>> {
        let mut pages = Vec::new();
        while let Some(page) = self.__next__(py)? {
            pages.push(page);
        }
        Ok(pages)
    }

    /// The number of pages fetched so far.
    #[getter]
    fn pages_fetched(&self) -> usize {
        self.page_count
    }
}

/// Parse a Link header value to extract the URL with rel="next".
/// Example: `<https://api.example.com/items?page=2>; rel="next", <...>; rel="prev"`
fn parse_link_next(header: &str) -> Option<String> {
    for part in header.split(',') {
        let part = part.trim();
        // Check if this part contains rel="next"
        if part.contains("rel=\"next\"") || part.contains("rel='next'") {
            // Extract URL from angle brackets
            if let Some(start) = part.find('<') {
                if let Some(end) = part.find('>') {
                    if start < end {
                        return Some(part[start + 1..end].to_string());
                    }
                }
            }
        }
    }
    None
}
