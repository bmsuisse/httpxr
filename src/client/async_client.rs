use pyo3::prelude::*;
use pyo3::types::{PyAnyMethods, PyBytes, PyDict, PyDictMethods, PyString};
use std::time::Instant;

use crate::config::{Limits, RetryConfig, Timeout};
use crate::models::{Cookies, Headers, Request, Response};
use crate::urls::URL;

use super::common::*;

/// Async HTTP Client backed by reqwest + tokio.
#[pyclass]
pub struct AsyncClient {
    pub(crate) base_url: Option<URL>,
    pub(crate) auth: Option<Py<PyAny>>,
    pub(crate) params: Option<Py<PyAny>>,
    pub(crate) default_headers: Py<Headers>,
    pub(crate) cookies: Cookies,
    pub(crate) max_redirects: u32,
    pub(crate) follow_redirects: bool,
    pub(crate) transport: Py<PyAny>,
    pub(crate) mounts: Py<PyDict>,
    pub(crate) timeout: Option<Timeout>,
    pub(crate) is_closed: bool,
    pub(crate) trust_env: bool,
    pub(crate) event_hooks: Option<Py<PyAny>>,
    #[allow(dead_code)]
    pub(crate) default_encoding: Option<Py<PyAny>>,
    pub(crate) cached_raw_headers: Vec<(Vec<u8>, Vec<u8>)>,
    pub(crate) cached_timeout: Option<std::time::Duration>,
    pub(crate) cached_connect_timeout: Option<f64>,
    pub(crate) cached_read_timeout: Option<f64>,
    pub(crate) cached_write_timeout: Option<f64>,
    pub(crate) cached_pool_timeout: Option<f64>,
    pub(crate) cached_base_url_str: Option<String>,
    pub(crate) retry: Option<RetryConfig>,
    pub(crate) rate_limit: Option<crate::config::RateLimit>,
}

#[pymethods]
impl AsyncClient {
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
    fn new(
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
        let cached_raw_headers = hdrs.get_raw_items_owned();
        let hdrs_py = Py::new(py, hdrs)?;
        let ckies = Cookies::create(py, cookies)?;
        let base = parse_base_url(base_url)?;

        let cached_base_url_str = base.as_ref().map(|b| b.to_string().trim_end_matches('/').to_string());

        let transport_obj = if let Some(t) = transport {
            t.clone().unbind()
        } else {
            create_default_async_transport(py, verify, cert, http2, limits, proxy)?
        };

        let mounts_dict: Py<PyDict> = if let Some(m) = mounts {
            if let Ok(d) = m.cast::<PyDict>() {
                d.clone().unbind()
            } else {
                PyDict::new(py).into()
            }
        } else {
            PyDict::new(py).into()
        };

        let to = if let Some(t) = timeout {
            t.extract::<Timeout>().ok()
        } else {
            None
        };
        let cached_timeout = to.as_ref().and_then(|t| {
            [t.connect, t.read, t.write]
                .iter()
                .filter_map(|v| *v)
                .reduce(f64::min)
                .map(std::time::Duration::from_secs_f64)
        });
        let cached_connect_timeout = to.as_ref().and_then(|t| t.connect);
        let cached_read_timeout = to.as_ref().and_then(|t| t.read);
        let cached_write_timeout = to.as_ref().and_then(|t| t.write);
        let cached_pool_timeout = to.as_ref().and_then(|t| t.pool);

        Ok(AsyncClient {
            base_url: base,
            auth: auth
                .map(|a| {
                    if let AuthArg::Custom(p) = a {
                        if let Err(e) = validate_auth_type(py, p.bind(py)) {
                            return Err(e);
                        }
                        Ok(Some(p))
                    } else {
                        Ok(None)
                    }
                })
                .transpose()?
                .flatten(),
            params,
            default_headers: hdrs_py,
            cookies: ckies,
            max_redirects: max_redirects.unwrap_or(DEFAULT_MAX_REDIRECTS),
            follow_redirects,
            transport: transport_obj,
            mounts: mounts_dict,
            timeout: to,
            is_closed: false,
            trust_env,
            event_hooks: event_hooks.map(|e| e.clone().unbind()),
            default_encoding: default_encoding.map(|de| de.clone().unbind()),
            cached_raw_headers,
            cached_timeout,
            cached_connect_timeout,
            cached_read_timeout,
            cached_write_timeout,
            cached_pool_timeout,
            cached_base_url_str,
            retry,
            rate_limit,
        })
    }

    #[pyo3(signature = (request, *, stream=false, auth=None, follow_redirects=None))]
    pub fn send<'py>(
        &self,
        py: Python<'py>,
        request: Bound<'py, PyAny>,
        stream: bool,
        auth: Option<Py<PyAny>>,
        follow_redirects: Option<bool>,
    ) -> PyResult<Bound<'py, PyAny>> {
        let mut req = request.extract::<Request>()?;
        if stream {
            req.stream_response = true;
        }

        if let Some(ref t) = self.timeout {
            let ext = req.extensions.bind(py);
            if let Ok(d) = ext.cast::<PyDict>() {
                if !d.contains("timeout")? {
                    d.set_item("timeout", Py::new(py, t.clone())?)?;
                }
            }
        }

        let follow_redirects = follow_redirects.unwrap_or(self.follow_redirects);
        let is_streaming = stream;

        let has_auth_flow = auth.is_some();
        let has_hooks = self.event_hooks.is_some() && {
            let hooks = self.event_hooks.as_ref().unwrap().bind(py);
            if let Ok(d) = hooks.cast::<PyDict>() {
                let has_request_hooks = d.get_item("request").ok().flatten()
                    .map(|l| l.len().unwrap_or(0) > 0).unwrap_or(false);
                let has_response_hooks = d.get_item("response").ok().flatten()
                    .map(|l| l.len().unwrap_or(0) > 0).unwrap_or(false);
                has_request_hooks || has_response_hooks
            } else {
                false
            }
        };
        let has_custom_mounts = !self.mounts.bind(py).is_empty();

        let can_fast_path = !has_auth_flow && !has_hooks && !has_custom_mounts && !is_streaming;

        if can_fast_path {
            if let Ok(transport_ref) = self.transport.bind(py)
                .cast::<crate::transports::default::AsyncHTTPTransport>()
            {
                let transport_borrow = transport_ref.borrow();
                let client = transport_borrow.get_client();
                let pool_sem = transport_borrow.get_pool_semaphore();

                let url_str = req.url.to_string();
                let method_str = req.method.clone();
                let raw_headers = req.headers.bind(py).borrow().get_raw_items_owned();
                let body = req.content_body.clone();
                let req_url = req.url.clone();

                let mut connect_timeout_val = None;
                let mut read_timeout_val = None;
                let mut write_timeout_val = None;
                let mut pool_timeout_val = None;
                if let Ok(ext) = req.extensions.bind(py).cast::<PyDict>() {
                    if let Some(timeout_obj) = ext.get_item("timeout").ok().flatten() {
                        if let Ok(t) = timeout_obj.extract::<crate::config::Timeout>() {
                            connect_timeout_val = t.connect;
                            read_timeout_val = t.read;
                            write_timeout_val = t.write;
                            pool_timeout_val = t.pool;
                        }
                    }
                }

                let timeout_val: Option<std::time::Duration> =
                    [connect_timeout_val, read_timeout_val, write_timeout_val]
                        .iter()
                        .filter_map(|t| *t)
                        .reduce(f64::min)
                        .map(std::time::Duration::from_secs_f64);

                let max_redirects = self.max_redirects;

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

                    let mut current_url = url_str;
                    let mut current_method = method_str;
                    let mut current_headers = raw_headers;
                    let mut current_body = body;
                    let mut redirect_count: u32 = 0;
                    let mut history_entries: Vec<(u16, Vec<(Vec<u8>, Vec<u8>)>, Vec<u8>, String)> = Vec::new();

                    loop {
                        let method = match current_method.as_str() {
                            "GET" => reqwest::Method::GET,
                            "POST" => reqwest::Method::POST,
                            "PUT" => reqwest::Method::PUT,
                            "DELETE" => reqwest::Method::DELETE,
                            "PATCH" => reqwest::Method::PATCH,
                            "HEAD" => reqwest::Method::HEAD,
                            "OPTIONS" => reqwest::Method::OPTIONS,
                            _ => reqwest::Method::from_bytes(current_method.as_bytes())
                                .map_err(|e| pyo3::exceptions::PyValueError::new_err(format!("Invalid method: {}", e)))?,
                        };

                        let mut req_builder = client.request(method, &current_url);
                        for (key, value) in &current_headers {
                            req_builder = req_builder.header(key.as_slice(), value.as_slice());
                        }
                        if let Some(ref b) = current_body {
                            req_builder = req_builder.body(b.clone());
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
                                } else if write_timeout_val.is_some()
                                    && current_body.is_some()
                                    && read_timeout_val.is_none()
                                {
                                    crate::exceptions::WriteTimeout::new_err(format!("{}", e))
                                } else {
                                    crate::exceptions::ReadTimeout::new_err(format!("{}", e))
                                }
                            } else if e.is_connect() {
                                crate::exceptions::ConnectError::new_err(format!("{}", e))
                            } else {
                                crate::exceptions::NetworkError::new_err(format!("{}", e))
                            }
                        })?;

                        let status_code = response.status().as_u16();
                        let version = response.version();
                        let resp_headers: Vec<(Vec<u8>, Vec<u8>)> = response.headers().iter()
                            .map(|(k, v)| (k.as_str().as_bytes().to_vec(), v.as_bytes().to_vec()))
                            .collect();

                        if follow_redirects && (300..400).contains(&status_code) {
                            let location = resp_headers.iter()
                                .find(|(k, _)| k == b"location")
                                .map(|(_, v)| String::from_utf8_lossy(v).to_string());

                            if let Some(loc) = location {
                                if redirect_count >= max_redirects {
                                    return Err(crate::exceptions::TooManyRedirects::new_err(format!(
                                        "Exceeded maximum number of redirects ({})",
                                        max_redirects
                                    )));
                                }

                                let body_bytes = response.bytes().await
                                    .map_err(|e| crate::exceptions::ReadError::new_err(format!("{}", e)))?;
                                history_entries.push((status_code, resp_headers, Vec::from(body_bytes), current_url.clone()));

                                let redirect_url = req_url.join_relative(&loc)
                                    .map_err(|e| crate::exceptions::RemoteProtocolError::new_err(e.to_string()))?;
                                current_url = redirect_url.to_string();

                                if status_code == 303 {
                                    current_method = "GET".to_string();
                                    current_body = None;
                                }

                                let original_host = req_url.get_raw_host().to_lowercase();
                                let redirect_host = redirect_url.get_raw_host().to_lowercase();
                                if original_host != redirect_host {
                                    current_headers.retain(|(k, _)| {
                                        let key_lower = String::from_utf8_lossy(k).to_lowercase();
                                        key_lower != "authorization"
                                    });
                                }
                                let new_host = crate::client::common::build_host_header(&redirect_url);
                                current_headers.retain(|(k, _)| {
                                    let key_lower = String::from_utf8_lossy(k).to_lowercase();
                                    key_lower != "host"
                                });
                                current_headers.push((b"host".to_vec(), new_host.into_bytes()));

                                redirect_count += 1;
                                continue;
                            }
                        }

                        let body_bytes = response.bytes().await.map_err(|e| {
                            if e.is_timeout() {
                                crate::exceptions::ReadTimeout::new_err(format!("{}", e))
                            } else {
                                crate::exceptions::ReadError::new_err(format!("{}", e))
                            }
                        })?;

                        let elapsed = start.elapsed().as_secs_f64();

                        return Python::attach(|py| {
                            let hdrs = Headers::from_raw_byte_pairs(resp_headers);
                            let ext = PyDict::new(py);
                            let version_str = match version {
                                reqwest::Version::HTTP_09 => "HTTP/0.9",
                                reqwest::Version::HTTP_10 => "HTTP/1.0",
                                reqwest::Version::HTTP_11 => "HTTP/1.1",
                                reqwest::Version::HTTP_2 => "HTTP/2",
                                reqwest::Version::HTTP_3 => "HTTP/3",
                                _ => "HTTP/1.1",
                            };
                            ext.set_item("http_version", version_str.as_bytes())?;

                            let history = if history_entries.is_empty() {
                                Vec::new()
                            } else {
                                let mut hist = Vec::new();
                                for (h_status, h_headers, h_body, _h_url) in &history_entries {
                                    let h_hdrs = Headers::from_raw_byte_pairs(h_headers.clone());
                                    let h_ext = PyDict::new(py);
                                    h_ext.set_item("http_version", b"HTTP/1.1")?;
                                    let h_resp = Response {
                                        status_code: *h_status,
                                        headers: Some(Py::new(py, h_hdrs)?),
                                lazy_headers: None,
                                        extensions: Some(h_ext.into()),
                                        request: Some(req.clone()),
                                        lazy_request_method: None,
                                        lazy_request_url: None,
                                        history: Vec::new(),
                                        content_bytes: Some(h_body.clone()),
                                        stream: None,
                                        default_encoding: PyString::intern(py, "utf-8")
                                            .into_any().unbind(),
                                        default_encoding_override: None,
                                        elapsed: None,
                                        is_closed_flag: false,
                                        is_stream_consumed: false,
                                        was_streaming: false,
                                        text_accessed: std::sync::atomic::AtomicBool::new(false),
                                        num_bytes_downloaded_counter: std::sync::Arc::new(
                                            std::sync::atomic::AtomicUsize::new(0),
                                        ),
                                    };
                                    hist.push(h_resp);
                                }
                                hist
                            };

                            let response = Response {
                                status_code,
                                headers: Some(Py::new(py, hdrs)?),
                            lazy_headers: None,
                                extensions: Some(ext.into()),
                                request: Some(req),
                                lazy_request_method: None,
                                lazy_request_url: None,
                                history,
                                content_bytes: Some(Vec::from(body_bytes)),
                                stream: None,
                                default_encoding: PyString::intern(py, "utf-8")
                                    .into_any().unbind(),
                                default_encoding_override: None,
                                elapsed: Some(elapsed),
                                is_closed_flag: false,
                                is_stream_consumed: false,
                                was_streaming: false,
                                text_accessed: std::sync::atomic::AtomicBool::new(false),
                                num_bytes_downloaded_counter: std::sync::Arc::new(
                                    std::sync::atomic::AtomicUsize::new(0),
                                ),
                            };

                            Ok(Py::new(py, response)?)
                        });
                    }
                });
            }
        }

        let transport = self.transport.clone_ref(py);
        let mounts = self.mounts.clone_ref(py);
        let max_redirects = self.max_redirects;
        let event_hooks = self.event_hooks.as_ref().map(|h| h.clone_ref(py));
        let event_loop: Option<Py<PyAny>> = if !is_streaming {
            Some(
                py.import("asyncio")?
                    .call_method0("get_running_loop")?
                    .unbind(),
            )
        } else {
            None
        };

        pyo3_async_runtimes::tokio::future_into_py(py, async move {
            let start = Instant::now();

            if let Some(flow) = auth {
                return Self::_send_with_auth_flow(
                    start,
                    flow,
                    req,
                    transport,
                    mounts,
                    max_redirects,
                    follow_redirects,
                    event_hooks,
                    None,
                )
                .await;
            }

            let mut current_req: Request = req;
            let mut redirect_count: u32 = 0;

            loop {
                if let Some(hooks) = &event_hooks {
                    let hook_list: Vec<Py<PyAny>> = Python::attach(|py| {
                        let h: &Bound<'_, PyAny> = hooks.bind(py);
                        if let Ok(d) = h.cast::<PyDict>() {
                            if let Ok(Some(req_hooks)) = d.get_item("request") {
                                if let Ok(l) = req_hooks.cast::<pyo3::types::PyList>() {
                                    let mut vec = Vec::new();
                                    for item in l.try_iter()? {
                                        vec.push(item?.unbind());
                                    }
                                    return Ok(vec);
                                }
                            }
                        }
                        Ok::<Vec<Py<PyAny>>, PyErr>(Vec::new())
                    })?;

                    for hook in hook_list {
                        let fut = Python::attach(|py| {
                            let h = hook.bind(py);
                            let req = Py::new(py, current_req.clone())?;
                            let ret = h.call1((req,))?;
                            let is_awaitable = py
                                .import("inspect")?
                                .call_method1("isawaitable", (&ret,))?
                                .extract::<bool>()?;
                            if is_awaitable {
                                Ok(Some(pyo3_async_runtimes::tokio::into_future(ret)?))
                            } else {
                                Ok::<_, PyErr>(None)
                            }
                        })?;
                        if let Some(f) = fut {
                            f.await?;
                        }
                    }
                }

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

                let response = Python::attach(|py| {
                    let mut resp: Response = response_obj.bind(py).extract()?;
                    resp.request = Some(current_req.clone());
                    resp.elapsed = Some(start.elapsed().as_secs_f64());

                    // extensions initialized lazily on first access

                    let resp_handle = Py::new(py, resp)?;
                    Ok::<Py<crate::models::Response>, PyErr>(resp_handle)
                })?;

                if let Some(hooks) = &event_hooks {
                    let hook_list: Vec<Py<PyAny>> = Python::attach(|py| {
                        let h: &Bound<'_, PyAny> = hooks.bind(py);
                        if let Ok(d) = h.cast::<PyDict>() {
                            if let Ok(Some(resp_hooks)) = d.get_item("response") {
                                if let Ok(l) = resp_hooks.cast::<pyo3::types::PyList>() {
                                    let mut vec = Vec::new();
                                    for item in l.try_iter()? {
                                        vec.push(item?.unbind());
                                    }
                                    return Ok(vec);
                                }
                            }
                        }
                        Ok::<Vec<Py<PyAny>>, PyErr>(Vec::new())
                    })?;

                    for hook in hook_list {
                        let fut = Python::attach(|py| {
                            let h = hook.bind(py);
                            let r = response.bind(py);
                            let ret = h.call1((r,))?;
                            let is_awaitable = py
                                .import("inspect")?
                                .call_method1("isawaitable", (&ret,))?
                                .extract::<bool>()?;
                            if is_awaitable {
                                Ok(Some(pyo3_async_runtimes::tokio::into_future(ret)?))
                            } else {
                                Ok::<_, PyErr>(None)
                            }
                        })?;
                        if let Some(f) = fut {
                            f.await?;
                        }
                    }
                }

                let maybe_redirect = Python::attach(|py| {
                    if !follow_redirects || !response.bind(py).borrow_mut().has_redirect_check(py) {
                        return Ok((None, Some(response)));
                    }

                    if redirect_count >= max_redirects {
                        return Err(crate::exceptions::TooManyRedirects::new_err(format!(
                            "Exceeded maximum number of redirects ({})",
                            max_redirects
                        )));
                    }

                    let location = response
                        .bind(py)
                        .borrow_mut()
                        .headers(py).unwrap()
                        .bind(py)
                        .borrow()
                        .get_first_value("location")
                        .ok_or_else(|| {
                            pyo3::exceptions::PyValueError::new_err(
                                "Redirect without Location header",
                            )
                        })?;
                    let mut redirect_url =
                        current_req.url.join_relative(&location).map_err(|e| {
                            crate::exceptions::RemoteProtocolError::new_err(e.to_string())
                        })?;

                    if redirect_url.parsed.fragment.is_none() {
                        redirect_url.parsed.fragment = current_req.url.parsed.fragment.clone();
                    }

                    let status_code = response.bind(py).borrow().status_code;
                    let redirect_method = if status_code == 303 {
                        "GET".to_string()
                    } else {
                        current_req.method.clone()
                    };
                    let redirect_body = if redirect_method == "GET" {
                        None
                    } else {
                        current_req.content_body.clone()
                    };

                    let mut redirect_headers = current_req.headers.bind(py).borrow().clone();
                    let current_host = current_req.url.get_raw_host().to_lowercase();
                    let redirect_host = redirect_url.get_raw_host().to_lowercase();
                    if current_host != redirect_host {
                        redirect_headers.remove_header("authorization");
                    }

                    redirect_headers.set_header("host", &build_host_header(&redirect_url));

                    let new_req = Request {
                        method: redirect_method,
                        url: redirect_url,
                        headers: Py::new(py, redirect_headers)?,
                        extensions: PyDict::new(py).into(),
                        content_body: redirect_body,
                        stream: None,
                        stream_response: false,
                    };

                    let new_resp = response.clone_ref(py);
                    let mut history = new_resp.bind(py).borrow().history.clone();
                    history.push(new_resp.bind(py).borrow().clone());

                    return Ok((Some((new_req, history)), None));
                })?;

                if let (Some((new_req, _history)), _) = maybe_redirect {
                    current_req = new_req;
                    redirect_count += 1;
                } else {
                    let final_response = maybe_redirect.1.unwrap();
                    if !is_streaming {
                        let has_async_stream = Python::attach(|py| {
                            let resp_bound = final_response.bind(py);
                            let has_stream = resp_bound.borrow().stream.is_some();
                            let has_content = resp_bound.borrow().content_bytes.is_some();
                            Ok::<bool, PyErr>(has_stream && !has_content)
                        })?;
                        if has_async_stream {
                            let result_future = Python::attach(|py| {
                                let resp_bound = final_response.bind(py);
                                let loop_bound = event_loop.as_ref().unwrap().bind(py);
                                let ns = pyo3::types::PyDict::new(py);
                                ns.set_item("_resp", resp_bound)?;
                                ns.set_item("_loop", loop_bound)?;
                                py.run(
                                    c"
async def _eagerly_read(resp, exc_holder):
    stream = resp._take_stream()
    if stream is None:
        resp._set_aread_result(b'')
        return True
    buf = b''
    aiter_obj = stream.__aiter__()
    try:
        async for chunk in aiter_obj:
            buf += bytes(chunk)
    except BaseException as _exc:
        resp._mark_closed()
        exc_holder.append(_exc)
        if hasattr(aiter_obj, 'aclose'):
            try:
                await aiter_obj.aclose()
            except Exception:
                pass
        if hasattr(stream, 'aclose'):
            try:
                await stream.aclose()
            except Exception:
                pass
        return False
    resp._set_aread_result(buf)
    return True

_exc_holder = []
_result_fut = _loop.create_future()
_task = _loop.create_task(_eagerly_read(_resp, _exc_holder))

def _on_done(t):
    if _result_fut.done():
        return
    _result_fut.set_result(t.result())

_task.add_done_callback(_on_done)
",
                                    Some(&ns),
                                    Some(&ns),
                                )?;
                                let result_fut = ns.get_item("_result_fut")?.unwrap();
                                let exc_holder = ns.get_item("_exc_holder")?.unwrap().unbind();
                                let fut =
                                    pyo3_async_runtimes::tokio::into_future(result_fut.clone())?;
                                Ok::<_, PyErr>((fut, exc_holder))
                            })?;
                            let success_obj = result_future.0.await?;
                            let success =
                                Python::attach(|py| success_obj.bind(py).extract::<bool>())?;
                            if !success {
                                let err = Python::attach(|py| -> PyResult<PyErr> {
                                    let holder = result_future.1.bind(py);
                                    let exc_obj = holder.get_item(0)?;
                                    Ok(PyErr::from_value(exc_obj.unbind().into_bound(py)))
                                })?;
                                return Err(err);
                            }
                        }
                    }
                    return Ok(final_response);
                }
            }
        })
    }
}

