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
        if self.is_closed {
            return Err(pyo3::exceptions::PyRuntimeError::new_err(
                "Cannot send a request, as the client has been closed.",
            ));
        }

        let start = Instant::now();

        let method_bytes = method.as_bytes();
        let is_bodyless = method_bytes.eq_ignore_ascii_case(b"GET")
            || method_bytes.eq_ignore_ascii_case(b"HEAD")
            || method_bytes.eq_ignore_ascii_case(b"DELETE")
            || method_bytes.eq_ignore_ascii_case(b"OPTIONS");
        let url_str_for_check = url.str()?.extract::<String>()?;
        let has_valid_scheme = (url_str_for_check.starts_with("http://") && url_str_for_check.len() > 7)
            || (url_str_for_check.starts_with("https://") && url_str_for_check.len() > 8)
            || url_str_for_check.starts_with("/");
        let _can_fast_path = is_bodyless
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



        let method_upper = method.to_uppercase();

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

        {
            let mut merged_qp_items: Vec<(String, String)> = Vec::new();
            if let Some(ref client_params) = self.params {
                let bound = client_params.bind(py);
                let qp = crate::urls::QueryParams::create(Some(bound))?;
                merged_qp_items.extend(qp.items_raw());
            }
            if let Some(req_params) = params {
                let qp = crate::urls::QueryParams::create(Some(req_params))?;
                merged_qp_items.extend(qp.items_raw());
            }
            if !merged_qp_items.is_empty() {
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

        let mut merged_headers = self.default_headers.bind(py).borrow().clone();
        if let Some(h) = headers {
            merged_headers.update_from(Some(h))?;
        }

        let body = build_request_body(py, content, data, files, json, &mut merged_headers)?;

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

        let mut merged_headers = arrange_headers_httpx_order(&merged_headers, &target_url);

        apply_url_auth(&target_url, &mut merged_headers);

        apply_cookies(py, &self.cookies, cookies, &mut merged_headers)?;

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

        let auth_kw = if let Some(kw) = kwargs {
            kw.get_item("auth").unwrap_or(None)
        } else {
            None
        };

        let (auth_val, auth_explicitly_none) = match auth_kw {
            None => (None, false), // Not passed -> Default (Use Client Auth) -> (None, false)? Wait.
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

        if let Some(c) = cookies {
            if let Ok(cookie_val) = c.extract::<String>() {
                request
                    .headers
                    .bind(py)
                    .borrow_mut()
                    .set_header("cookie", &cookie_val);
            }
        }

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
                self._send_handling_auth(py, start, request, flow, follow_redirects)
            }
            _ => {
                self._send_single_request(py, start, request, follow_redirects)
            }
        }
    }
}

// -- Macros for sync HTTP convenience methods --
// Each macro generates a separate #[pymethods] impl block because PyO3's proc macro
// does not support macro invocations inside #[pymethods] blocks.

macro_rules! sync_no_body_method {
    ($name:ident, $method:expr) => {
        #[pymethods]
        impl Client {
            #[pyo3(signature = (url, *, params=None, headers=None, cookies=None, follow_redirects=None, timeout=None, extensions=None, **kwargs))]
            fn $name(
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
                let t = timeout.or_else(|| timeout_kw.as_ref());
                let f = follow_redirects.or_else(|| {
                    extract_from_kwargs(kwargs, "follow_redirects")
                        .and_then(|v| v.extract::<bool>().ok())
                });
                self.request(
                    py, $method, url, None, None, None, None, params, headers,
                    cookies, f, t, extensions, kwargs,
                )
            }
        }
    };
}

macro_rules! sync_body_method {
    ($name:ident, $method:expr) => {
        #[pymethods]
        impl Client {
            #[pyo3(signature = (url, *, content=None, data=None, files=None, json=None, params=None, headers=None, cookies=None, follow_redirects=None, timeout=None, extensions=None, **kwargs))]
            fn $name(
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
                let t = timeout.or_else(|| timeout_kw.as_ref());
                let f = follow_redirects.or_else(|| {
                    extract_from_kwargs(kwargs, "follow_redirects")
                        .and_then(|v| v.extract::<bool>().ok())
                });
                self.request(
                    py, $method, url, content, data, files, json, params, headers,
                    cookies, f, t, extensions, kwargs,
                )
            }
        }
    };
}

macro_rules! sync_raw_no_body {
    ($name:ident, $method:expr) => {
        #[pymethods]
        impl Client {
            #[pyo3(signature = (url, *, headers=None, timeout=None))]
            fn $name(&self, py: Python<'_>, url: &str, headers: Option<&Bound<'_, PyDict>>, timeout: Option<f64>) -> PyResult<Py<PyAny>> {
                self._send_raw(py, $method, url, headers, None, timeout)
            }
        }
    };
}

macro_rules! sync_raw_with_body {
    ($name:ident, $method:expr) => {
        #[pymethods]
        impl Client {
            #[pyo3(signature = (url, *, headers=None, body=None, timeout=None))]
            fn $name(&self, py: Python<'_>, url: &str, headers: Option<&Bound<'_, PyDict>>, body: Option<Vec<u8>>, timeout: Option<f64>) -> PyResult<Py<PyAny>> {
                self._send_raw(py, $method, url, headers, body, timeout)
            }
        }
    };
}

sync_no_body_method!(get, "GET");
sync_no_body_method!(head, "HEAD");
sync_no_body_method!(options, "OPTIONS");
sync_no_body_method!(delete, "DELETE");
sync_body_method!(post, "POST");
sync_body_method!(put, "PUT");
sync_body_method!(patch, "PATCH");
sync_raw_no_body!(get_raw, reqwest::Method::GET);
sync_raw_with_body!(post_raw, reqwest::Method::POST);
sync_raw_with_body!(put_raw, reqwest::Method::PUT);
sync_raw_with_body!(patch_raw, reqwest::Method::PATCH);
sync_raw_no_body!(delete_raw, reqwest::Method::DELETE);
sync_raw_no_body!(head_raw, reqwest::Method::HEAD);

#[pymethods]
impl Client {
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

        let request_refs: Vec<Request> = requests
            .iter()
            .map(|r| {
                let bound = r.bind(py);
                let borrowed = bound.borrow();
                borrowed.clone()
            })
            .collect();

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
        let u = crate::urls::URL::create_from_str(&url_str)?;
        self._transport_for_url_inner(py, &u)
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

            let is_same_host = req_host == red_host;
            let is_same_port = req_port == red_port;
            let is_https_upgrade = is_same_host
                && req_scheme == "http"
                && red_scheme == "https"
                && (req_port.is_none() || req_port == Some(80))
                && (red_port.is_none() || red_port == Some(443));
            let is_same_origin = is_same_host && is_same_port && req_scheme == red_scheme;

            if !is_same_origin && !is_https_upgrade {
                hdrs.remove_header("authorization");
            }

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

/// Lazy iterator that fetches one page per `__next__()` call.
/// Created by `Client.paginate()`.
#[pyclass]
pub struct PageIterator {
    pub(crate) client: Py<PyAny>,
    pub(crate) method: String,
    pub(crate) current_url: Option<Py<PyAny>>,
    pub(crate) next_url_key: Option<String>,
    pub(crate) next_header_name: Option<String>,
    pub(crate) next_func: Option<Py<PyAny>>,
    pub(crate) max_pages: usize,
    pub(crate) page_count: usize,
    pub(crate) done: bool,
    pub(crate) params: Option<Py<PyAny>>,
    pub(crate) headers: Option<Py<PyAny>>,
    pub(crate) cookies: Option<Py<PyAny>>,
    pub(crate) timeout: Option<Py<PyAny>>,
    pub(crate) extensions: Option<Py<PyAny>>,
    pub(crate) _kwargs: Option<Py<PyDict>>,
}
