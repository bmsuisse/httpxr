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
                &format!("python-httpr/{}", env!("CARGO_PKG_VERSION")),
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
        let target_url = request.url.clone();

        // Select transport (mounts support)
        let selected_transport = self.select_transport_for_url(py, &target_url);

        // Send request
        let mut response = {
            let t_bound = selected_transport.bind(py);
            let req_py = Py::new(py, request.clone())?;
            let res_py = t_bound.call_method1("handle_request", (req_py,))?;
            res_py.extract::<Response>()?
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
            log::info!(target: "httpr", "HTTP Request: {} {} \"{} {} {}\"", method, url, http_version, status, reason);
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

            // Send the request
            let mut response = {
                let selected_transport = self.select_transport_for_url(py, &current_req.url);
                let t_bound = selected_transport.bind(py);
                let req_py = Py::new(py, current_req.clone())?;
                let res_py = t_bound.call_method1("handle_request", (req_py,))?;
                res_py.extract::<Response>()?
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
        let req_py = Py::new(py, request.clone())?;
        let res_py = t_bound.call_method1("handle_request", (req_py,))?;
        let mut response: Response = res_py.extract()?;
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
                let mut new_response = {
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
                let req_py = Py::new(py, redirect_request.clone())?;
                let res_py = t_bound.call_method1("handle_request", (req_py,))?;
                res_py.extract::<Response>()?
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
                log::info!(target: "httpr", "HTTP Request: {} {} \"{} {} {}\"", method, url, http_version, status, reason);
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
}
