use pyo3::prelude::*;
use pyo3::types::{PyBytes, PyDict, PyString};
use std::time::Instant;

use crate::models::{Headers, Request, Response};
use crate::stream_ctx::StreamContextManager;

use super::common::*;
use super::async_client::AsyncClient;

impl AsyncClient {
    pub(crate) async fn _send_with_auth_flow(
        start: Instant,
        flow: Py<PyAny>,
        original_req: Request,
        transport: Py<PyAny>,
        mounts: Py<PyDict>,
        _max_redirects: u32,
        _follow_redirects: bool,
        _event_hooks: Option<Py<PyAny>>,
        default_encoding: Option<Py<PyAny>>,
    ) -> PyResult<Py<Response>> {
        let mut history: Vec<Response> = Vec::new();

        let is_async_flow = Python::attach(|py| flow.bind(py).hasattr("__anext__"))?;

        let first_req = if is_async_flow {
            let coro =
                Python::attach(|py| flow.bind(py).call_method0("__anext__").map(|b| b.unbind()))?;
            let fut = Python::attach(|py| {
                pyo3_async_runtimes::tokio::into_future(coro.bind(py).clone())
            })?;
            match fut.await {
                Ok(v) => Python::attach(|py| Ok(Some(v.bind(py).extract::<Request>()?))),
                Err(e) => {
                    let is_stop = Python::attach(|py| {
                        e.is_instance_of::<pyo3::exceptions::PyStopAsyncIteration>(py)
                    });
                    if is_stop {
                        Ok(None)
                    } else {
                        Err(e)
                    }
                }
            }?
        } else {
            Python::attach(|py| {
                let flow_bound = flow.bind(py);
                match flow_bound.call_method0("__next__") {
                    Ok(r) => Ok(Some(r.extract::<Request>()?)),
                    Err(e) if e.is_instance_of::<pyo3::exceptions::PyStopIteration>(py) => Ok(None),
                    Err(e) => Err(e),
                }
            })?
        };

        let mut current_req = match first_req {
            Some(r) => r,
            None => {
                return Self::_send_single_async(
                    start,
                    original_req,
                    transport,
                    mounts,
                    default_encoding,
                )
                .await;
            }
        };

        loop {
            let response_coro = Python::attach(|py| {
                let transport_to_use = select_transport(
                    mounts.bind(py).clone(),
                    transport.clone_ref(py),
                    &current_req.url,
                )?;
                let t_bound = transport_to_use.bind(py);
                let req_py = Py::new(py, current_req.clone())?;
                t_bound
                    .call_method1("handle_async_request", (req_py,))
                    .map(|b| b.unbind())
            })?;

            let future = Python::attach(|py| {
                let bound = response_coro.bind(py);
                pyo3_async_runtimes::tokio::into_future(bound.clone())
            })?;
            let response_obj: Py<PyAny> = future.await?;

            let mut response = Python::attach(|py| {
                let mut resp: Response = response_obj.bind(py).extract()?;
                resp.request = Some(current_req.clone());
                resp.elapsed = Some(start.elapsed().as_secs_f64());

                if let Some(ref de) = default_encoding {
                    let current_encoding: String = resp.default_encoding.bind(py).extract()?;
                    if current_encoding == "utf-8" {
                        let new_encoding: String = de.bind(py).extract()?;
                        resp.default_encoding = pyo3::types::PyString::new(py, &new_encoding)
                            .into_any()
                            .unbind();
                    }
                }

                let ext = resp.extensions.bind(py);
                if let Ok(d) = ext.cast::<PyDict>() {
                    if !d.contains("http_version")? {
                        d.set_item("http_version", PyBytes::new(py, b"HTTP/1.1"))?;
                    }
                }

                Ok::<Response, PyErr>(resp)
            })?;

            let resp_body_needed = Python::attach(|py| {
                let flow_bound = flow.bind(py);
                flow_bound
                    .getattr("requires_response_body")
                    .ok()
                    .and_then(|v| v.extract::<bool>().ok())
                    .unwrap_or(false)
            });
            if resp_body_needed {
                if response.content_bytes.is_none() {
                    Python::attach(|py| {
                        let resp_py = Py::new(py, response.clone())?;
                        let _ = resp_py.call_method0(py, "read");
                        response = resp_py.extract(py)?;
                        Ok::<_, PyErr>(())
                    })?;
                }
            }

            let next_req = if is_async_flow {
                let coro = Python::attach(|py| {
                    flow.bind(py)
                        .call_method1("asend", (response_obj.bind(py),))
                        .map(|b| b.unbind())
                })?;
                let fut = Python::attach(|py| {
                    pyo3_async_runtimes::tokio::into_future(coro.bind(py).clone())
                })?;
                match fut.await {
                    Ok(v) => Python::attach(|py| Ok(Some(v.bind(py).extract::<Request>()?))),
                    Err(e) => {
                        let is_stop = Python::attach(|py| {
                            e.is_instance_of::<pyo3::exceptions::PyStopAsyncIteration>(py)
                        });
                        if is_stop {
                            Ok(None)
                        } else {
                            Err(e)
                        }
                    }
                }?
            } else {
                Python::attach(|py| {
                    match flow.bind(py).call_method1("send", (response_obj.bind(py),)) {
                        Ok(r) => Ok(Some(r.extract::<Request>()?)),
                        Err(e) if e.is_instance_of::<pyo3::exceptions::PyStopIteration>(py) => {
                            Ok(None)
                        }
                        Err(e) => Err(e),
                    }
                })?
            };

            if let Some(next) = next_req {
                response.history = history.clone();
                history.push(response);
                current_req = next;
            } else {
                response.history = history;
                return Python::attach(|py| Py::new(py, response));
            }
        }
    }

    pub(crate) async fn _send_single_async(
        start: Instant,
        request: Request,
        transport: Py<PyAny>,
        mounts: Py<PyDict>,
        default_encoding: Option<Py<PyAny>>,
    ) -> PyResult<Py<Response>> {
        let response_coro = Python::attach(|py| {
            let transport_to_use = select_transport(
                mounts.bind(py).clone(),
                transport.clone_ref(py),
                &request.url,
            )?;
            let t_bound = transport_to_use.bind(py);
            let req_py = Py::new(py, request.clone())?;
            t_bound
                .call_method1("handle_async_request", (req_py,))
                .map(|b| b.unbind())
        })?;

        let future = Python::attach(|py| {
            let bound = response_coro.bind(py);
            pyo3_async_runtimes::tokio::into_future(bound.clone())
        })?;
        let response_obj: Py<PyAny> = future.await?;

        Python::attach(|py| {
            let mut resp: Response = response_obj.bind(py).extract()?;
            resp.request = Some(request);
            resp.elapsed = Some(start.elapsed().as_secs_f64());

            if let Some(ref de) = default_encoding {
                resp.default_encoding = de.clone_ref(py);
            }

            let ext = resp.extensions.bind(py);
            if let Ok(d) = ext.cast::<PyDict>() {
                if !d.contains("http_version")? {
                    d.set_item("http_version", PyBytes::new(py, b"HTTP/1.1"))?;
                }
            }

            {
                let method = &resp
                    .request
                    .as_ref()
                    .map(|r| r.method.clone())
                    .unwrap_or_default();
                let url = resp
                    .request
                    .as_ref()
                    .map(|r| r.url.to_string())
                    .unwrap_or_default();
                let http_version = {
                    let ext = resp.extensions.bind(py);
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
                let status = resp.status_code;
                let reason = resp.reason_phrase();
                log::info!(target: "httpxr", "HTTP Request: {} {} \"{} {} {}\"", method, url, http_version, status, reason);
            }

            Py::new(py, resp)
        })
    }
}

#[pymethods]
impl AsyncClient {
    #[pyo3(signature = (method, url, **kwargs))]
    fn stream<'py>(
        client: Bound<'py, Self>,
        py: Python<'py>,
        method: String,
        url: Bound<'py, PyAny>,
        kwargs: Option<Bound<'py, PyDict>>,
    ) -> PyResult<StreamContextManager> {
        let mut request = client
            .call_method1("build_request", (method, url))
            .map_err(|e| e)?
            .extract::<Request>()?;
        request.stream_response = true;

        Ok(StreamContextManager {
            client: client.unbind(),
            method: request.method,
            url: request.url.into_pyobject(py)?.into_any().unbind(),
            kwargs: kwargs.map(|k| k.unbind()),
            response: None,
        })
    }

    #[pyo3(signature = (method, url, *, content=None, data=None, files=None, json=None, params=None, headers=None, cookies=None, follow_redirects=None, timeout=None, extensions=None, stream=false, **kwargs))]
    pub fn request<'py>(
        &self,
        py: Python<'py>,
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
        stream: bool,
        kwargs: Option<&Bound<'_, PyDict>>,
    ) -> PyResult<Bound<'py, PyAny>> {
        if self.is_closed {
            return Err(pyo3::exceptions::PyRuntimeError::new_err(
                "The client is closed",
            ));
        }
        let _files = files;
        let method_str = method.to_uppercase();
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

        let scheme = target_url.get_scheme();
        let host = target_url.get_host();
        if scheme.is_empty() || scheme == "://" {
            return Err(crate::exceptions::UnsupportedProtocol::new_err(format!(
                "Request URL is missing a scheme for URL '{}'",
                url_str
            )));
        }
        {
            let url_full = target_url.to_string();
            let has_mount = self.mounts.bind(py).iter().any(|(p, _)| {
                if let Ok(pattern) = p.extract::<String>() {
                    url_matches_pattern(&url_full, &pattern)
                } else {
                    false
                }
            });
            if !["http", "https", "ws", "wss"].contains(&&*scheme) && !has_mount {
                return Err(crate::exceptions::UnsupportedProtocol::new_err(format!(
                    "Request URL has an unsupported protocol '{}': for URL '{}'",
                    scheme, url_str
                )));
            }
        }
        if host.is_empty() && ["http", "https"].contains(&&*scheme) {
            return Err(crate::exceptions::LocalProtocolError::new_err(format!(
                "Invalid URL '{}'",
                url_str
            )));
        }

        let mut merged_headers = self.default_headers.bind(py).borrow().clone();
        if let Some(h) = headers {
            merged_headers.update_from(Some(h))?;
        }

        if !merged_headers.contains_header("host") {
            merged_headers.set_header("host", &build_host_header(&target_url));
        }

        let (body, stream_obj): (Option<Vec<u8>>, Option<Py<PyAny>>) = if let Some(c) = content {
            if let Ok(b) = c.cast::<PyBytes>() {
                (Some(b.as_bytes().to_vec()), None)
            } else if let Ok(s) = c.extract::<String>() {
                (Some(s.into_bytes()), None)
            } else {
                let is_async_iter = c.call_method0("__aiter__").is_ok();
                let is_iter = c.call_method0("__iter__").is_ok();
                if !is_async_iter && is_iter {
                    return Err(pyo3::exceptions::PyRuntimeError::new_err(
                        "The content argument must be an async iterator.",
                    ));
                }
                let wrapper = crate::types::ResponseAsyncIteratorStream::new(c.clone().unbind());
                let wrapper_py = Py::new(py, wrapper)?;
                (None, Some(wrapper_py.into_any()))
            }
        } else if let Some(j) = json {
            let json_mod = py.import("json")?;
            let s: String = json_mod.call_method1("dumps", (j,))?.extract()?;
            merged_headers.set_header("content-type", "application/json");
            (Some(s.into_bytes()), None)
        } else if let Some(f) = files {
            let boundary = if let Some(ct) = merged_headers.get("content-type", None) {
                if let Some(idx) = ct.find("boundary=") {
                    let remainder = &ct[idx + 9..];
                    if let Some(end) = remainder.find(';') {
                        Some(remainder[..end].trim_matches('"').to_string())
                    } else {
                        Some(remainder.trim_matches('"').to_string())
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
            (Some(multipart.get_content()), None)
        } else if let Some(d) = data {
            let urllib = py.import("urllib.parse")?;
            let s: String = urllib.call_method1("urlencode", (d, true))?.extract()?;
            merged_headers.set_header("content-type", "application/x-www-form-urlencoded");
            (Some(s.into_bytes()), None)
        } else {
            (None, None)
        };

        if !merged_headers.contains_header("host") {
            merged_headers.set_header("host", target_url.get_raw_host());
        }

        apply_url_auth(&target_url, &mut merged_headers);

        apply_cookies(py, &self.cookies, cookies, &mut merged_headers)?;

        let mut request = Request {
            method: method_str,
            url: target_url.clone(),
            headers: Py::new(py, merged_headers)?,
            extensions: PyDict::new(py).into(),
            content_body: body,
            stream: stream_obj,
            stream_response: stream,
        };

        let t_val = if let Some(t_arg) = timeout {
            if t_arg.is_none() {
                None
            } else {
                Some(crate::config::Timeout::new(
                    py,
                    Some(t_arg),
                    None,
                    None,
                    None,
                    None,
                )?)
            }
        } else {
            self.timeout.clone()
        };

        if let Some(t) = t_val {
            let _ = request
                .extensions
                .bind(py)
                .set_item("timeout", Py::new(py, t)?);
        }
        if let Some(e) = extensions {
            if let Ok(d) = e.cast::<PyDict>() {
                for (k, v) in d.iter() {
                    request.extensions.bind(py).set_item(k, v)?;
                }
            }
        }

        let auth_kw = if let Some(kw) = kwargs {
            kw.get_item("auth").unwrap_or(None)
        } else {
            None
        };

        let (auth_val, auth_explicitly_none) = match auth_kw {
            None => (None, false), // Default -> Use Client
            Some(b) => {
                if b.is_none() {
                    (None, true) // Explicit None -> Disable
                } else {
                    validate_auth_type(py, &b)?;
                    (Some(b), false) // Object
                }
            }
        };
        let auth_result = apply_auth_async(
            py,
            auth_val.as_ref(),
            self.auth.as_ref(),
            &mut request,
            auth_explicitly_none,
        )?;

        let auth_flow = match auth_result {
            AuthResult::Flow(flow) => Some(flow),
            _ => None,
        };

        let req_py = Py::new(py, request)?;
        self.send(
            py,
            req_py.into_bound(py).into_any(),
            stream,
            auth_flow,
            follow_redirects,
        )
    }

    #[pyo3(signature = (url, *, params=None, headers=None, cookies=None, follow_redirects=None, timeout=None, extensions=None, **kwargs))]
    fn get<'py>(
        &self,
        py: Python<'py>,
        url: &Bound<'_, PyAny>,
        params: Option<&Bound<'_, PyAny>>,
        headers: Option<&Bound<'_, PyAny>>,
        cookies: Option<&Bound<'_, PyAny>>,
        follow_redirects: Option<bool>,
        timeout: Option<&Bound<'_, PyAny>>,
        extensions: Option<&Bound<'_, PyAny>>,
        kwargs: Option<&Bound<'_, PyDict>>,
    ) -> PyResult<Bound<'py, PyAny>> {
        let has_no_extras = params.is_none()
            && cookies.is_none()
            && extensions.is_none()
            && (kwargs.is_none() || kwargs.map_or(true, |k| k.is_empty()));

        let url_str_for_check = url.str()?.extract::<String>()?;
        let has_valid_scheme = (url_str_for_check.starts_with("http://") && url_str_for_check.len() > 7)
            || (url_str_for_check.starts_with("https://") && url_str_for_check.len() > 8)
            || url_str_for_check.starts_with("/");

        if has_no_extras
            && has_valid_scheme
            && !self.is_closed
            && self.auth.is_none()
            && self.event_hooks.is_none()
            && self.params.is_none()
        {
            let has_custom_mounts = !self.mounts.bind(py).is_empty();
            if !has_custom_mounts {
                if let Ok(transport_ref) = self.transport.bind(py)
                    .cast::<crate::transports::default::AsyncHTTPTransport>()
                {
                    let transport_borrow = transport_ref.borrow();
                    let client = transport_borrow.get_client();
                    let pool_sem = transport_borrow.get_pool_semaphore();

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

                    let raw_headers = self.cached_raw_headers.clone();

                    let extra_raw: Vec<(Vec<u8>, Vec<u8>)> = if let Some(h) = headers {
                        let extra = Headers::create(Some(h), "utf-8")?;
                        extra.get_raw_items_owned()
                    } else {
                        Vec::new()
                    };

                    let (timeout_val, connect_timeout_val, read_timeout_val, write_timeout_val, pool_timeout_val) = if let Some(t_arg) = timeout {
                        if t_arg.is_none() {
                            (None, None, None, None, None)
                        } else {
                            let t = crate::config::Timeout::new(py, Some(t_arg), None, None, None, None)?;
                            let dur = [t.connect, t.read, t.write]
                                .iter()
                                .filter_map(|t| *t)
                                .reduce(f64::min)
                                .map(std::time::Duration::from_secs_f64);
                            (dur, t.connect, t.read, t.write, t.pool)
                        }
                    } else {
                        (self.cached_timeout, self.cached_connect_timeout, self.cached_read_timeout, self.cached_write_timeout, self.cached_pool_timeout)
                    };

                    let max_redirects = self.max_redirects;
                    let do_follow_redirects = follow_redirects.unwrap_or(self.follow_redirects);

                    return pyo3_async_runtimes::tokio::future_into_py(py, async move {
                        let start = Instant::now();

                        let _pool_permit = if let Some(ref sem) = pool_sem {
                            let pool_dur = pool_timeout_val
                                .map(std::time::Duration::from_secs_f64)
                                .unwrap_or(std::time::Duration::from_secs(30));
                            match tokio::time::timeout(pool_dur, sem.clone().acquire_owned()).await {
                                Ok(Ok(permit)) => Some(permit),
                                Ok(Err(_)) => {
                                    return Err(crate::exceptions::PoolTimeout::new_err(
                                        "Connection pool is closed".to_string(),
                                    ));
                                }
                                Err(_) => {
                                    return Err(crate::exceptions::PoolTimeout::new_err(
                                        "Timed out waiting for a connection from the pool".to_string(),
                                    ));
                                }
                            }
                        } else {
                            None
                        };

                        let mut current_url = full_url;
                        let mut redirects_remaining = max_redirects;
                        let mut history: Vec<(u16, Vec<(Vec<u8>, Vec<u8>)>, Vec<u8>, String)> = Vec::new();

                        let (final_status, final_headers_raw, final_body, final_url) = loop {
                            let mut req_builder = client.get(&current_url);
                            for (key, value) in &raw_headers {
                                req_builder = req_builder.header(key.as_slice(), value.as_slice());
                            }
                            for (key, value) in &extra_raw {
                                req_builder = req_builder.header(key.as_slice(), value.as_slice());
                            }
                            if let Some(t) = timeout_val {
                                req_builder = req_builder.timeout(t);
                            }

                            let response = req_builder.send().await.map_err(|e| {
                                if e.is_timeout() {
                                    if e.is_connect()
                                        || (connect_timeout_val.is_some()
                                            && read_timeout_val.is_none()
                                            && write_timeout_val.is_none())
                                    {
                                        crate::exceptions::ConnectTimeout::new_err(format!("{}", e))
                                    } else {
                                        crate::exceptions::ReadTimeout::new_err(format!("{}", e))
                                    }
                                } else if e.is_connect() {
                                    crate::exceptions::ConnectError::new_err(format!("{}", e))
                                } else {
                                    crate::exceptions::NetworkError::new_err(format!("{}", e))
                                }
                            })?;

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

                            let body = response.bytes().await.map_err(|e| {
                                if e.is_timeout() {
                                    crate::exceptions::ReadTimeout::new_err(format!("{}", e))
                                } else {
                                    crate::exceptions::ReadError::new_err(format!("{}", e))
                                }
                            })?;

                            if let Some(loc) = redirect_location {
                                if redirects_remaining == 0 {
                                    return Err(crate::exceptions::TooManyRedirects::new_err(
                                        "Too many redirects".to_string(),
                                    ));
                                }
                                history.push((status, resp_headers, body.to_vec(), current_url.clone()));
                                redirects_remaining -= 1;
                                let new_url = if loc.starts_with("http://") || loc.starts_with("https://") {
                                    loc
                                } else if let Ok(base) = reqwest::Url::parse(&current_url) {
                                    base.join(&loc)
                                        .map(|u| u.to_string())
                                        .unwrap_or(loc)
                                } else {
                                    loc
                                };
                                current_url = new_url;
                                continue;
                            }

                            break (status, resp_headers, body.to_vec(), current_url);
                        };

                        let elapsed = start.elapsed().as_secs_f64();

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

                        Python::attach(|py| {
                            let mut history_responses: Vec<crate::models::Response> = Vec::new();
                            for (hist_status, hist_headers, hist_body, hist_url) in &history {
                                let h_hdrs = crate::models::Headers::from_raw_byte_pairs(hist_headers.clone());
                                let h_ext = PyDict::new(py);
                                h_ext.set_item("http_version", PyBytes::new(py, b"HTTP/1.1"))?;
                                let h_req_url = crate::urls::URL::create_from_str_fast(hist_url);
                                let h_req = crate::models::Request {
                                    method: "GET".to_string(),
                                    url: h_req_url,
                                    headers: Py::new(py, crate::models::Headers::empty())?,
                                    extensions: PyDict::new(py).into(),
                                    content_body: None,
                                    stream: None,
                                    stream_response: false,
                                };
                                if log::log_enabled!(target: "httpxr", log::Level::Info) {
                                    log::info!(target: "httpxr", "HTTP Request: GET {} \"HTTP/1.1 {} {}\"", hist_url, hist_status, reason_for(*hist_status));
                                }
                                history_responses.push(crate::models::Response {
                                    status_code: *hist_status,
                                    headers: Py::new(py, h_hdrs)?,
                                    extensions: h_ext.into(),
                                    request: Some(h_req),
                                    lazy_request_method: None,
                                    lazy_request_url: None,
                                    history: Vec::new(),
                                    content_bytes: Some(hist_body.clone()),
                                    stream: None,
                                    default_encoding: PyString::intern(py, "utf-8").into_any().unbind(),
                                    default_encoding_override: None,
                                    elapsed: None,
                                    is_closed_flag: false,
                                    is_stream_consumed: false,
                                    was_streaming: false,
                                    text_accessed: std::sync::atomic::AtomicBool::new(false),
                                    num_bytes_downloaded_counter: std::sync::Arc::new(std::sync::atomic::AtomicUsize::new(0)),
                                });
                            }

                            let headers_obj = crate::models::Headers::from_raw_byte_pairs(final_headers_raw);

                            let req_url = crate::urls::URL::create_from_str_fast(&final_url);
                            let request_obj = crate::models::Request {
                                method: "GET".to_string(),
                                url: req_url,
                                headers: Py::new(py, crate::models::Headers::empty())?,
                                extensions: PyDict::new(py).into(),
                                content_body: None,
                                stream: None,
                                stream_response: false,
                            };

                            if log::log_enabled!(target: "httpxr", log::Level::Info) {
                                log::info!(target: "httpxr", "HTTP Request: GET {} \"HTTP/1.1 {} {}\"", final_url, final_status, reason_for(final_status));
                            }

                            let ext = PyDict::new(py);
                            ext.set_item("http_version", PyBytes::new(py, b"HTTP/1.1"))?;

                            let response = crate::models::Response {
                                status_code: final_status,
                                headers: Py::new(py, headers_obj)?,
                                stream: None,
                                content_bytes: Some(final_body),
                                is_stream_consumed: false,
                                is_closed_flag: false,
                                was_streaming: false,
                                default_encoding: PyString::intern(py, "utf-8")
                                    .into_any()
                                    .unbind(),
                                default_encoding_override: None,
                                text_accessed: std::sync::atomic::AtomicBool::new(false),
                                request: Some(request_obj),
                                lazy_request_method: None,
                                lazy_request_url: None,
                                elapsed: Some(elapsed),
                                extensions: ext.into(),
                                history: history_responses,
                                num_bytes_downloaded_counter: std::sync::Arc::new(
                                    std::sync::atomic::AtomicUsize::new(0),
                                ),
                            };

                            Ok(Py::new(py, response)?)
                        })
                    });
                }
            }
        }

        let timeout_kw = extract_from_kwargs(kwargs, "timeout");
        let t = if let Some(t) = timeout {
            Some(t)
        } else {
            timeout_kw.as_ref()
        };
        self.request(
            py,
            "GET",
            url,
            None,
            None,
            None,
            None,
            params,
            headers,
            cookies,
            follow_redirects,
            t,
            extensions,
            false,
            kwargs,
        )
    }
}
