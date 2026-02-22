use pyo3::prelude::*;
use pyo3::types::{PyAnyMethods, PyBytes, PyDict, PyDictMethods};
use std::time::Instant;

use crate::config::{Limits, RetryConfig, Timeout};
use crate::models::{Cookies, Headers, Request, Response};
use crate::urls::URL;

use super::common::*;

#[pyclass(subclass, module = "httpxr._httpxr")]
pub struct Client {
    pub(crate) base_url: Option<URL>,
    pub(crate) reqwest_base_url: Option<reqwest::Url>,
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
    pub(crate) retry: Option<RetryConfig>,
    pub(crate) rate_limit: Option<crate::config::RateLimit>,
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
        event_hooks=None,
        retry=None,
        rate_limit=None
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
        retry: Option<RetryConfig>,
        rate_limit: Option<crate::config::RateLimit>,
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
        let reqwest_base_url = base.as_ref().and_then(|b| reqwest::Url::parse(&b.to_string()).ok());

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
            reqwest_base_url,
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
            retry,
            rate_limit,
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
        let _is_bodyless = method_bytes.eq_ignore_ascii_case(b"GET")
            || method_bytes.eq_ignore_ascii_case(b"HEAD")
            || method_bytes.eq_ignore_ascii_case(b"DELETE")
            || method_bytes.eq_ignore_ascii_case(b"OPTIONS");
        let url_str_for_check = url.str()?.extract::<String>()?;
        let has_valid_scheme = (url_str_for_check.starts_with("http://") && url_str_for_check.len() > 7)
            || (url_str_for_check.starts_with("https://") && url_str_for_check.len() > 8)
            || url_str_for_check.starts_with("/");

        let should_follow = follow_redirects.unwrap_or(self.follow_redirects);

        let mut simple_body_bytes: Option<Vec<u8>> = None;
        let mut can_fast_path_body = true;

        if data.is_some() || files.is_some() || json.is_some() {
            can_fast_path_body = false;
        } else if let Some(c) = content {
            if let Ok(b) = c.cast::<pyo3::types::PyBytes>() {
                simple_body_bytes = Some(b.as_bytes().to_vec());
            } else if let Ok(s) = c.extract::<String>() {
                simple_body_bytes = Some(s.into_bytes());
            } else {
                can_fast_path_body = false;
            }
        }

        let is_fast_method = method_bytes.eq_ignore_ascii_case(b"GET")
            || method_bytes.eq_ignore_ascii_case(b"HEAD")
            || method_bytes.eq_ignore_ascii_case(b"DELETE")
            || method_bytes.eq_ignore_ascii_case(b"OPTIONS")
            || method_bytes.eq_ignore_ascii_case(b"POST")
            || method_bytes.eq_ignore_ascii_case(b"PUT")
            || method_bytes.eq_ignore_ascii_case(b"PATCH");
            
        let _can_fast_path = is_fast_method
            && has_valid_scheme
            && can_fast_path_body
            && params.is_none()
            && cookies.is_none()
            && extensions.is_none()
            && kwargs.map(|k| k.is_empty()).unwrap_or(true)
            && self.auth.is_none()
            && self.params.is_none()
            && self.event_hooks.is_none()
            && self.mounts.is_empty()
            && self.timeout.is_none()
            && timeout.is_none()
            && !should_follow;

        if _can_fast_path {
            let t_bound = self.transport.bind(py);
            if let Ok(transport) = t_bound.cast::<crate::transports::default::HTTPTransport>() {
                if let Ok(method_reqwest) = reqwest::Method::from_bytes(method_bytes) {
                    let parsed_url_res = if let Some(ref r_base) = self.reqwest_base_url {
                        r_base.join(&url_str_for_check).map_err(|_| ())
                    } else {
                        reqwest::Url::parse(&url_str_for_check).map_err(|_| ())
                    };

                    if let Ok(target_url) = parsed_url_res {
                        let target_url_str = target_url.to_string();
                        let def_hdrs = self.default_headers.bind(py).borrow();
                        let raw_headers = &def_hdrs.raw;

                        match transport.borrow().send_fast_raw(py, method_reqwest, target_url, raw_headers, simple_body_bytes.as_deref(), None) {
                            Ok(mut response) => {
                                response.elapsed = Some(start.elapsed().as_secs_f64());
                                response.lazy_request_method = Some(method.to_uppercase());
                                response.lazy_request_url = Some(target_url_str);

                                if let Some(ref de) = self.default_encoding {
                                    response.default_encoding = de.clone_ref(py);
                                }

                                if log::log_enabled!(log::Level::Info) {
                                    log::info!(target: "httpxr", "HTTP Request: {} {} \"HTTP/1.1 {} {}\"", 
                                        response.lazy_request_method.as_ref().unwrap(), response.lazy_request_url.as_ref().unwrap(), response.status_code, response.reason_phrase());
                                }

                                return Ok(response);
                            },
                            Err(_) => {}
                        }
                    }
                }
            }
        }
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
                let qp = crate::query_params::QueryParams::create(Some(bound))?;
                merged_qp_items.extend(qp.items_raw());
            }
            if let Some(req_params) = params {
                let qp = crate::query_params::QueryParams::create(Some(req_params))?;
                merged_qp_items.extend(qp.items_raw());
            }
            if !merged_qp_items.is_empty() {
                if let Some(ref existing_q) = target_url.parsed.query {
                    let existing_qp = crate::query_params::QueryParams::from_query_string(existing_q);
                    let mut all_items = existing_qp.items_raw();
                    all_items.extend(merged_qp_items);
                    merged_qp_items = all_items;
                }
                let final_qp = crate::query_params::QueryParams::from_items(merged_qp_items);
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
            None => (None, false),
            Some(b) => {
                if b.is_none() {
                    (None, true)
                } else {
                    validate_auth_type(py, &b)?;
                    (Some(b), false)
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
        self.reqwest_base_url = self.base_url.as_ref().and_then(|b| reqwest::Url::parse(&b.to_string()).ok());
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

    #[pyo3(signature = (method, url, *, content=None, data=None, files=None, json=None, params=None, headers=None, cookies=None, extensions=None, timeout=None))]
    #[allow(unused_variables)]
    pub(crate) fn build_request(
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
        timeout: Option<&Bound<'_, PyAny>>,
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

        let ext_dict = PyDict::new(py);
        if let Some(t) = timeout {
            ext_dict.set_item("timeout", t)?;
        }
        if let Some(e) = extensions {
            if let Ok(d) = e.cast::<PyDict>() {
                for (k, v) in d.iter() {
                    ext_dict.set_item(k, v)?;
                }
            }
        }
        Ok(Request {
            method: method_upper,
            url: target_url,
            headers: Py::new(py, final_headers)?,
            extensions: ext_dict.into(),
            content_body: body,
            stream: None,
            stream_response: false,
        })
    }

    fn __repr__(&self) -> String {
        if self.is_closed {
            "<Client [closed]>".to_string()
        } else {
            "<Client>".to_string()
        }
    }
}

