//! Send-path helpers for the synchronous `Client`.
//!
//! This module contains the private implementation methods for request dispatch,
//! authentication flows, redirect handling, transport selection, and the
//! `PageIterator` used by `Client.paginate()`.

use pyo3::prelude::*;
use pyo3::types::{PyAnyMethods, PyBytes, PyDict, PyDictMethods};
use std::time::Instant;

use crate::models::{Request, Response};
use crate::urls::URL;

use super::common::*;
use super::sync_client::{Client, PageIterator};

// ---------------------------------------------------------------------------
// Send & auth flow
// ---------------------------------------------------------------------------

impl Client {
    pub(crate) fn _send_single_request(
        &self,
        py: Python<'_>,
        start: Instant,
        request: Request,
        follow_redirects: Option<bool>,
    ) -> PyResult<Response> {
        let selected_transport = self.select_transport_for_url(py, &request.url);

        let mut response = {
            let t_bound = selected_transport.bind(py);
            if let Ok(t) = t_bound.cast::<crate::transports::default::HTTPTransport>() {
                t.borrow().send_request(py, &request)?
            } else {
                let req_py = Py::new(py, request.clone())?;
                let res_py = t_bound.call_method1("handle_request", (req_py,))?;
                res_py.extract::<Response>()?
            }
        };

        response.elapsed = Some(start.elapsed().as_secs_f64());
        response.request = Some(request.clone());

        if let Some(ref de) = self.default_encoding {
            response.default_encoding = de.clone_ref(py);
        }

        let ext = response.ensure_extensions(py).bind(py);
        if let Ok(d) = ext.cast::<PyDict>() {
            if !d.contains("http_version")? {
                d.set_item("http_version", PyBytes::new(py, b"HTTP/1.1"))?;
            }
        }

        {
            let method = &request.method;
            let url = request.url.to_string();
            let http_version = {
                let ext = response.ensure_extensions(py).bind(py);
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

        extract_cookies_to_jar(py, &mut response, self.cookies.jar.bind(py))?;

        let should_follow = follow_redirects.unwrap_or(self.follow_redirects);
        if should_follow {
            return self.handle_redirects(py, start, request, response);
        }

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

    pub(crate) fn _send_handling_auth(
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

        let first_req = match flow_bound.call_method0("__next__") {
            Ok(r) => r.extract::<Request>()?,
            Err(e) if e.is_instance_of::<pyo3::exceptions::PyStopIteration>(py) => {
                return self._send_single_request(py, start, _original_request, follow_redirects);
            }
            Err(e) => return Err(e),
        };

        let mut current_req = first_req;

        loop {
            let req_body_needed = flow_bound
                .getattr("requires_request_body")
                .ok()
                .and_then(|v| v.extract::<bool>().ok())
                .unwrap_or(false);
            if req_body_needed {
                if let Some(ref body) = current_req.content_body {
                    let _ = body; // Body is already available
                }
            }

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

            if let Some(ref de) = self.default_encoding {
                response.default_encoding = de.clone_ref(py);
            }

            let ext = response.ensure_extensions(py).bind(py);
            if let Ok(d) = ext.cast::<PyDict>() {
                if !d.contains("http_version")? {
                    d.set_item("http_version", PyBytes::new(py, b"HTTP/1.1"))?;
                }
            }

            extract_cookies_to_jar(py, &mut response, self.cookies.jar.bind(py))?;

            let resp_body_needed = flow_bound
                .getattr("requires_response_body")
                .ok()
                .and_then(|v| v.extract::<bool>().ok())
                .unwrap_or(false);
            if resp_body_needed {
                if response.content_bytes.is_none() {
                    let resp_py = Py::new(py, response.clone())?;
                    let _ = resp_py.call_method0(py, "read");
                    response = resp_py.extract(py)?;
                }
            }

            let resp_py = Py::new(py, response.clone())?;
            let next_req = match flow_bound.call_method1("send", (resp_py.bind(py),)) {
                Ok(r) => {
                    match r.extract::<Request>() {
                        Ok(req) => Some(req),
                        Err(_) => {
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
                response.history = history.clone();
                history.push(response);
                current_req = next;
            } else {
                response.history = history;

                let should_follow = follow_redirects.unwrap_or(self.follow_redirects);
                if should_follow {
                    return self.handle_redirects(py, start, current_req, response);
                }

                return Ok(response);
            }
        }
    }
}

// ---------------------------------------------------------------------------
// Transport selection & redirect handling
// ---------------------------------------------------------------------------

/// Private implementation helpers (not exposed to Python).
impl Client {
    /// Select the correct transport for a URL, checking mounts first
    pub(crate) fn select_transport_for_url(&self, py: Python<'_>, url: &URL) -> Py<PyAny> {
        self._transport_for_url_inner(py, url)
            .unwrap_or_else(|_| self.transport.clone_ref(py))
    }

    /// Core transport selection logic, shared between `_transport_for_url` and `select_transport_for_url`.
    pub(crate) fn _transport_for_url_inner(&self, py: Python<'_>, url: &URL) -> PyResult<Py<PyAny>> {
        let url_str = url.to_string();
        let mut best_transport: Option<Py<PyAny>> = None;
        let mut best_priority = 0u32;

        for (pattern, transport) in &self.mounts {
            if url_matches_pattern(&url_str, pattern) {
                let priority = compute_pattern_priority(pattern);
                if priority > best_priority {
                    best_priority = priority;
                    best_transport = Some(transport.clone_ref(py));
                }
            }
        }

        if let Some(t) = best_transport {
            return Ok(t);
        }

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

    pub(crate) fn handle_redirects(
        &self,
        py: Python<'_>,
        start: Instant,
        request: Request,
        response: Response,
    ) -> PyResult<Response> {
        let mut redirect_count = 0u32;
        let mut current_response = response;
        let mut current_request = request;

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
                .headers(py).unwrap()
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

            if redirect_url.parsed.fragment.is_none() {
                redirect_url.parsed.fragment = current_request.url.parsed.fragment.clone();
            }

            let redirect_method = if current_response.status_code == 303 {
                "GET".to_string()
            } else {
                current_request.method.clone()
            };

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

            extract_cookies_to_jar(py, &mut current_response, self.cookies.jar.bind(py))?;

            let mut redirect_headers = current_request.headers.bind(py).borrow().clone();
            redirect_headers.remove_header("cookie");

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

            let current_host = current_request.url.get_raw_host().to_lowercase();
            let redirect_host = redirect_url.get_raw_host().to_lowercase();
            if current_host != redirect_host {
                redirect_headers.remove_header("authorization");
            }

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

            {
                let method = &redirect_request.method;
                let url = redirect_request.url.to_string();
                let http_version = {
                    let ext = new_response.ensure_extensions(py).bind(py);
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
    pub(crate) fn _send_raw(
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

// ---------------------------------------------------------------------------
// PageIterator — lazy pagination over HTTP responses
// ---------------------------------------------------------------------------

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

        let client_bound = self.client.bind(py);

        let params_arg = if self.page_count == 0 {
            self.params.as_ref().map(|p| p.bind(py).clone())
        } else {
            None
        };

        let mut response: Response = client_bound
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
            let hdrs = response.headers(py).unwrap().bind(py).borrow();
            if let Some(header_val) = hdrs.get_first_value(header_name.as_str()) {
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
pub(crate) fn parse_link_next(header: &str) -> Option<String> {
    for part in header.split(',') {
        let part = part.trim();
        if part.contains("rel=\"next\"") || part.contains("rel='next'") {
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
