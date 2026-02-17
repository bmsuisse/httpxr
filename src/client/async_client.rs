use pyo3::prelude::*;
use pyo3::types::{PyAnyMethods, PyBytes, PyDict, PyDictMethods, PyString};
use std::time::Instant;

use crate::config::{Limits, Timeout};
use crate::models::{Cookies, Headers, Request, Response};
use crate::stream_ctx::StreamContextManager;
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
    // Cached values for fast path (pre-computed at construction time)
    pub(crate) cached_raw_headers: Vec<(Vec<u8>, Vec<u8>)>,
    pub(crate) cached_timeout: Option<std::time::Duration>,
    pub(crate) cached_connect_timeout: Option<f64>,
    pub(crate) cached_read_timeout: Option<f64>,
    pub(crate) cached_write_timeout: Option<f64>,
    pub(crate) cached_pool_timeout: Option<f64>,
    pub(crate) cached_base_url_str: Option<String>,
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
        event_hooks=None
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
        // Pre-compute cached raw headers for fast path
        let cached_raw_headers = hdrs.get_raw_items_owned();
        let hdrs_py = Py::new(py, hdrs)?;
        let ckies = Cookies::create(py, cookies)?;
        let base = parse_base_url(base_url)?;

        // Pre-compute cached base URL string
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

        // Pre-compute cached timeout values
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
                        // Validate custom auth
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
        })
    }

    #[pyo3(signature = (request, *, stream=false, auth=None, follow_redirects=None))]
    fn send<'py>(
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

        // Apply client-level timeout to request if it doesn't already have one
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

        // ── FAST PATH ──────────────────────────────────────────────────
        // For simple non-streaming requests with no auth flow, no event hooks,
        // and no custom mounts: bypass all Python overhead and call reqwest
        // directly from Rust. This reduces GIL acquisitions from ~6 to 2.
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

        // Try to extract the reqwest::Client directly from our transport
        let can_fast_path = !has_auth_flow && !has_hooks && !has_custom_mounts && !is_streaming;

        if can_fast_path {
            if let Ok(transport_ref) = self.transport.bind(py)
                .cast::<crate::transports::default::AsyncHTTPTransport>()
            {
                let transport_borrow = transport_ref.borrow();
                let client = transport_borrow.get_client();
                let pool_sem = transport_borrow.get_pool_semaphore();

                // Extract all request data under THIS GIL hold (no additional Python::attach needed)
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

                // All data extracted — now release GIL and do everything in Rust
                return pyo3_async_runtimes::tokio::future_into_py(py, async move {
                    let start = Instant::now();

                    // Pool semaphore
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

                    // Handle redirects in Rust (no GIL needed for simple redirect logic)
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

                        // Check for redirect (in Rust, no GIL)
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

                                // Read body for history entry
                                let body_bytes = response.bytes().await
                                    .map_err(|e| crate::exceptions::ReadError::new_err(format!("{}", e)))?;
                                history_entries.push((status_code, resp_headers, Vec::from(body_bytes), current_url.clone()));

                                // Resolve redirect URL
                                let redirect_url = req_url.join_relative(&loc)
                                    .map_err(|e| crate::exceptions::RemoteProtocolError::new_err(e.to_string()))?;
                                current_url = redirect_url.to_string();

                                // Method change for 303
                                if status_code == 303 {
                                    current_method = "GET".to_string();
                                    current_body = None;
                                }

                                // Strip auth on cross-origin redirect
                                let original_host = req_url.get_raw_host().to_lowercase();
                                let redirect_host = redirect_url.get_raw_host().to_lowercase();
                                if original_host != redirect_host {
                                    current_headers.retain(|(k, _)| {
                                        let key_lower = String::from_utf8_lossy(k).to_lowercase();
                                        key_lower != "authorization"
                                    });
                                }
                                // Update host header
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

                        // Final response — read body
                        let body_bytes = response.bytes().await.map_err(|e| {
                            if e.is_timeout() {
                                crate::exceptions::ReadTimeout::new_err(format!("{}", e))
                            } else {
                                crate::exceptions::ReadError::new_err(format!("{}", e))
                            }
                        })?;

                        let elapsed = start.elapsed().as_secs_f64();

                        // Single GIL acquisition to build Response
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

                            // Build history if we had redirects
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
                                        headers: Py::new(py, h_hdrs)?,
                                        extensions: h_ext.into(),
                                        request: Some(req.clone()),
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
                                headers: Py::new(py, hdrs)?,
                                extensions: ext.into(),
                                request: Some(req),
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

        // ── SLOW PATH ──────────────────────────────────────────────────
        // Full-featured path with auth flows, event hooks, streaming, etc.
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

            // If we have an auth flow, drive it
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

            // No auth flow, send normally with redirect handling
            let mut current_req: Request = req;
            let mut redirect_count: u32 = 0;

            loop {
                // Event hooks: request
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

                    // Set http_version
                    let ext = resp.extensions.bind(py);
                    if let Ok(d) = ext.cast::<PyDict>() {
                        if !d.contains("http_version")? {
                            d.set_item("http_version", PyBytes::new(py, b"HTTP/1.1"))?;
                        }
                    }

                    let resp_handle = Py::new(py, resp)?;
                    Ok::<Py<crate::models::Response>, PyErr>(resp_handle)
                })?;

                // Response hooks
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
                    if !follow_redirects || !response.bind(py).borrow().has_redirect_check(py) {
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
                        .borrow()
                        .headers
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

                    // Fragment preservation
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

                    // Strip authorization on cross-origin redirect
                    let mut redirect_headers = current_req.headers.bind(py).borrow().clone();
                    let current_host = current_req.url.get_raw_host().to_lowercase();
                    let redirect_host = redirect_url.get_raw_host().to_lowercase();
                    if current_host != redirect_host {
                        redirect_headers.remove_header("authorization");
                    }

                    // Update host header
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
                    // Eagerly read body for non-streaming requests
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

// Non-pymethods helper functions for AsyncClient
impl AsyncClient {
    async fn _send_with_auth_flow(
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

        // Check if flow is async generator
        let is_async_flow = Python::attach(|py| flow.bind(py).hasattr("__anext__"))?;

        // Get the first request from the flow
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
                // Empty flow, send original request directly
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
            // Send the request
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
                        // Only override if default
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

            // Check requires_response_body
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

            // Feed response to the flow and get next request
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

    async fn _send_single_async(
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

            // Log the request/response
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
    fn request<'py>(
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

        // Build URL
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

        // Validate resolved URL has valid scheme and host
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

        // Build merged headers
        let mut merged_headers = self.default_headers.bind(py).borrow().clone();
        if let Some(h) = headers {
            merged_headers.update_from(Some(h))?;
        }

        if !merged_headers.contains_header("host") {
            merged_headers.set_header("host", &build_host_header(&target_url));
        }

        // Build body
        // Build body and stream
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
                // It is an async iterator (or treated as such)
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

        // URL auth
        apply_url_auth(&target_url, &mut merged_headers);

        // Apply cookies
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

        // Attach timeout
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

        // Apply auth
        // auth default is True (use client auth).
        // auth=None or False means "Disable Auth".
        // auth=object means "Use Object".
        // Apply auth
        // AuthArg handles usage.
        // None (missing arg) -> Use Client Auth.
        // Some(AuthArg::Boolean(true)) -> Use Client Auth.
        // Some(AuthArg::Boolean(false)) -> Disable Auth.
        // Some(AuthArg::Custom(obj)) -> If None: Disable, Else: Use Object.
        // Extract auth from kwargs
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

        // Get optional auth flow for passing to send
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
        // ULTRA-FAST PATH: bypass both request() and send() entirely
        let has_no_extras = params.is_none()
            && cookies.is_none()
            && extensions.is_none()
            && (kwargs.is_none() || kwargs.map_or(true, |k| k.is_empty()));

        // Extract URL string early for validation
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

                    // Resolve URL using cached base URL string
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

                    // Use pre-cached raw headers
                    let raw_headers = self.cached_raw_headers.clone();

                    // Merge per-request headers if any
                    let extra_raw: Vec<(Vec<u8>, Vec<u8>)> = if let Some(h) = headers {
                        let extra = Headers::create(Some(h), "utf-8")?;
                        extra.get_raw_items_owned()
                    } else {
                        Vec::new()
                    };

                    // Extract timeout — use cached values unless per-request override
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

                    // ALL data extracted — release GIL, do everything in Rust
                    return pyo3_async_runtimes::tokio::future_into_py(py, async move {
                        let start = Instant::now();

                        // Pool semaphore
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

                        // Build and send request (with redirect handling in Rust)
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
                            // No host header needed — reqwest adds it automatically
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

                        // Single GIL acquisition to build Response
                        Python::attach(|py| {
                            // Build history Response objects from redirect data
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

                            // Build the request object for response.request
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

        // SLOW PATH: fall through to normal request() → send()
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

    #[pyo3(signature = (url, *, params=None, headers=None, cookies=None, follow_redirects=None, timeout=None, extensions=None, **kwargs))]
    fn head<'py>(
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
            false, kwargs,
        )
    }

    #[pyo3(signature = (url, *, params=None, headers=None, cookies=None, follow_redirects=None, timeout=None, extensions=None, **kwargs))]
    fn options<'py>(
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
        let timeout_kw = extract_from_kwargs(kwargs, "timeout");
        let t = if let Some(t) = timeout {
            Some(t)
        } else {
            timeout_kw.as_ref()
        };
        self.request(
            py,
            "OPTIONS",
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

    #[pyo3(signature = (url, *, content=None, data=None, files=None, json=None, params=None, headers=None, cookies=None, follow_redirects=None, timeout=None, extensions=None, **kwargs))]
    fn post<'py>(
        &self,
        py: Python<'py>,
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
    ) -> PyResult<Bound<'py, PyAny>> {
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
            extensions, false, kwargs,
        )
    }

    #[pyo3(signature = (url, *, content=None, data=None, files=None, json=None, params=None, headers=None, cookies=None, follow_redirects=None, timeout=None, extensions=None, **kwargs))]
    fn put<'py>(
        &self,
        py: Python<'py>,
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
    ) -> PyResult<Bound<'py, PyAny>> {
        let timeout_kw = extract_from_kwargs(kwargs, "timeout");
        let t = if let Some(t) = timeout {
            Some(t)
        } else {
            timeout_kw.as_ref()
        };
        self.request(
            py,
            "PUT",
            url,
            content,
            data,
            files,
            json,
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

    #[pyo3(signature = (url, *, content=None, data=None, files=None, json=None, params=None, headers=None, cookies=None, follow_redirects=None, timeout=None, extensions=None, **kwargs))]
    fn patch<'py>(
        &self,
        py: Python<'py>,
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
    ) -> PyResult<Bound<'py, PyAny>> {
        let timeout_kw = extract_from_kwargs(kwargs, "timeout");
        let t = if let Some(t) = timeout {
            Some(t)
        } else {
            timeout_kw.as_ref()
        };
        self.request(
            py,
            "PATCH",
            url,
            content,
            data,
            files,
            json,
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

    #[pyo3(signature = (url, *, params=None, headers=None, cookies=None, follow_redirects=None, timeout=None, extensions=None, **kwargs))]
    fn delete<'py>(
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
            false, kwargs,
        )
    }

    fn aclose<'py>(&mut self, py: Python<'py>) -> PyResult<Bound<'py, PyAny>> {
        self.is_closed = true;
        pyo3_async_runtimes::tokio::future_into_py(py, async move { Ok(()) })
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
            .map(|t| Py::new(py, t.clone()).ok())
            .flatten()
            .map(|p| p.into())
            .unwrap_or_else(|| py.None())
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

    #[pyo3(signature = (method, url, *, content=None, data=None, files=None, json=None, params=None, headers=None, extensions=None))]
    pub fn build_request(
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
        extensions: Option<&Bound<'_, PyAny>>,
    ) -> PyResult<Request> {
        let _ = files;
        let _ = params;
        let _ = extensions;
        let url_str = url.str()?.extract::<String>()?;
        let target_url = if let Some(ref base) = self.base_url {
            let full = format!("{}{}", base.to_string().trim_end_matches('/'), &url_str);
            URL::create_from_str(&full)?
        } else {
            URL::create_from_str(&url_str)?
        };

        let mut merged_headers = self.default_headers.bind(py).borrow().clone();
        if let Some(h) = headers {
            merged_headers.update_from(Some(h))?;
        }

        let body = if let Some(c) = content {
            if let Ok(b) = c.cast::<pyo3::types::PyBytes>() {
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

        Ok(Request {
            method: method.to_uppercase(),
            url: target_url,
            headers: Py::new(py, merged_headers)?,
            extensions: PyDict::new(py).into(),
            content_body: body,
            stream: None,
            stream_response: false,
        })
    }

    /// Dispatch multiple requests concurrently using the batch transport.
    /// Returns a list of Response objects (or exceptions if return_exceptions=True).
    /// This is an httpxr extension — not available in httpx.
    #[pyo3(signature = (requests, *, max_concurrency=10, return_exceptions=false))]
    fn gather<'py>(
        &self,
        py: Python<'py>,
        requests: Vec<Py<Request>>,
        max_concurrency: usize,
        return_exceptions: bool,
    ) -> PyResult<Bound<'py, PyAny>> {
        if self.is_closed {
            return Err(pyo3::exceptions::PyRuntimeError::new_err(
                "Cannot send requests, as the client has been closed.",
            ));
        }

        if requests.is_empty() {
            return pyo3_async_runtimes::tokio::future_into_py(py, async move {
                Python::attach(|py| {
                    Ok(pyo3::types::PyList::empty(py).unbind())
                })
            });
        }

        let transport = self.transport.clone_ref(py);
        let mounts = self.mounts.clone_ref(py);

        // Check if the transport supports batch requests (our optimized path)
        let has_batch = transport
            .bind(py)
            .hasattr("handle_async_requests_batch")
            .unwrap_or(false);

        // Check that no request uses a custom mount (all use default transport)
        let all_default_transport = requests.iter().all(|req_py| {
            let req = req_py.bind(py).borrow();
            let selected = select_transport(
                mounts.bind(py).clone(),
                transport.clone_ref(py),
                &req.url,
            );
            match selected {
                Ok(t) => t.bind(py).is(&transport.bind(py)),
                Err(_) => false,
            }
        });

        if has_batch && all_default_transport && !return_exceptions {
            // Fast path: use Rust batch transport — minimal GIL contention
            let t_bound = transport.bind(py);
            let req_list = pyo3::types::PyList::new(py, &requests)?;
            return t_bound.call_method1(
                "handle_async_requests_batch",
                (req_list, max_concurrency),
            ).map(|b| b.clone());
        }

        // Fallback path: use asyncio.gather (for custom transports or return_exceptions)
        let coros = pyo3::types::PyList::empty(py);
        for req_py in &requests {
            let transport_to_use = {
                let req_bound = req_py.bind(py);
                let req_ref = req_bound.borrow();
                select_transport(
                    mounts.bind(py).clone(),
                    transport.clone_ref(py),
                    &req_ref.url,
                )?
            };
            let t_bound = transport_to_use.bind(py);
            let coro = t_bound.call_method1("handle_async_request", (req_py,))?;
            coros.append(coro)?;
        }

        let ns = pyo3::types::PyDict::new(py);
        let asyncio = py.import("asyncio")?;
        ns.set_item("asyncio", asyncio)?;
        ns.set_item("coros", &coros)?;
        ns.set_item("max_concurrency", max_concurrency)?;
        ns.set_item("return_exceptions", return_exceptions)?;

        py.run(
            c"
import asyncio

_sem = asyncio.Semaphore(max_concurrency)

async def _limited(coro):
    async with _sem:
        return await coro

_gather_coro = asyncio.gather(
    *[_limited(c) for c in coros],
    return_exceptions=return_exceptions,
)
",
            Some(&ns),
            Some(&ns),
        )?;

        let gather_coro = ns.get_item("_gather_coro")?.unwrap().unbind();

        pyo3_async_runtimes::tokio::future_into_py(py, async move {
            // Await the gather coroutine (created in the main thread)
            let gather_future = Python::attach(|py| {
                pyo3_async_runtimes::tokio::into_future(gather_coro.bind(py).clone())
            })?;

            let results_obj = gather_future.await?;

            // Convert results to a proper list
            Python::attach(|py| {
                let results_bound = results_obj.bind(py);
                let py_list = pyo3::types::PyList::empty(py);

                if let Ok(result_list) = results_bound.cast::<pyo3::types::PyList>() {
                    for (i, item) in result_list.iter().enumerate() {
                        // Set request on response if possible
                        if let Ok(mut resp) = item.extract::<Response>() {
                            let req_bound = requests[i].bind(py);
                            let req_ref = req_bound.borrow();
                            resp.request = Some(req_ref.clone());
                            py_list.append(Py::new(py, resp)?)?;
                        } else {
                            // Error case (return_exceptions=True)
                            py_list.append(&item)?;
                        }
                    }
                }
                Ok(py_list.unbind())
            })
        })
    }

    /// Auto-follow pagination links, returning an async iterator.
    /// Each iteration fetches the next page — memory-efficient for large result sets.
    /// Supports JSON key extraction, Link header parsing, and custom callables.
    /// This is an httpxr extension — not available in httpx.
    ///
    /// Usage:
    ///   async for page in client.paginate("GET", url, next_header="link"):
    ///       process(page.json())
    #[pyo3(signature = (method, url, *, next_url=None, next_header=None, next_func=None, max_pages=100, params=None, headers=None, cookies=None, timeout=None, extensions=None, **kwargs))]
    #[allow(clippy::too_many_arguments, unused_variables)]
    fn paginate<'py>(
        slf: &Bound<'py, Self>,
        py: Python<'py>,
        method: &str,
        url: &Bound<'_, PyAny>,
        next_url: Option<String>,
        next_header: Option<String>,
        next_func: Option<Py<PyAny>>,
        max_pages: usize,
        params: Option<Py<PyAny>>,
        headers: Option<Py<PyAny>>,
        cookies: Option<Py<PyAny>>,
        timeout: Option<Py<PyAny>>,
        extensions: Option<Py<PyAny>>,
        kwargs: Option<Py<PyDict>>,
    ) -> PyResult<Bound<'py, PyAny>> {
        {
            let this = slf.borrow();
            if this.is_closed {
                return Err(pyo3::exceptions::PyRuntimeError::new_err(
                    "Cannot send requests, as the client has been closed.",
                ));
            }
        }
        if next_url.is_none() && next_header.is_none() && next_func.is_none() {
            return Err(pyo3::exceptions::PyValueError::new_err(
                "Must specify one of: next_url, next_header, or next_func",
            ));
        }

        // Use a Python async generator for the iterator — this is the simplest
        // and most correct approach since Python handles the state naturally.
        let ns = PyDict::new(py);
        ns.set_item("client", slf)?;
        ns.set_item("method", method)?;
        ns.set_item("url", url)?;
        ns.set_item("next_url", &next_url)?;
        ns.set_item("next_header", &next_header)?;
        ns.set_item("next_func", &next_func)?;
        ns.set_item("max_pages", max_pages)?;
        ns.set_item("headers", &headers)?;
        ns.set_item("timeout", &timeout)?;

        py.run(
            c"
import re as _re

def _parse_link_next(header):
    for part in header.split(','):
        part = part.strip()
        if 'rel=\"next\"' in part or \"rel='next'\" in part:
            m = _re.search(r'<([^>]+)>', part)
            if m:
                return m.group(1)
    return None

class _AsyncPageIter:
    def __init__(self):
        self._current_url = url
        self._page_count = 0
        self._done = False

    def __aiter__(self):
        return self

    async def __anext__(self):
        if self._done or self._page_count >= max_pages:
            raise StopAsyncIteration

        if self._current_url is None:
            raise StopAsyncIteration

        kw = {}
        if headers is not None:
            kw['headers'] = headers
        if timeout is not None:
            kw['timeout'] = timeout

        response = await client.request(method, self._current_url, **kw)
        self._page_count += 1

        # Extract next URL
        _next = None
        if next_url is not None:
            try:
                data = response.json()
                _next = data.get(next_url) if isinstance(data, dict) else None
            except Exception:
                _next = None
        elif next_header is not None:
            hdr = response.headers.get(next_header)
            if hdr:
                _next = _parse_link_next(hdr)
        elif next_func is not None:
            _next = next_func(response)

        if _next is None:
            self._done = True
        else:
            self._current_url = _next

        return response

    async def collect(self):
        pages = []
        async for page in self:
            pages.append(page)
        return pages

    @property
    def pages_fetched(self):
        return self._page_count

_paginator = _AsyncPageIter()
",
            Some(&ns),
            Some(&ns),
        )?;

        let paginator = ns.get_item("_paginator")?.unwrap();
        Ok(paginator.into_any())
    }

    fn _transport_for_url(&self, url: &Bound<'_, PyAny>) -> PyResult<Py<PyAny>> {
        let py = url.py();
        let url_str: String = url.str()?.extract()?;
        let u = URL::create_from_str(&url_str)?;
        self.get_transport(py, &u)
    }

    fn get_transport(&self, py: Python<'_>, url: &URL) -> PyResult<Py<PyAny>> {
        let url_str = url.to_string();
        let mounts_bound = self.mounts.bind(py);

        // Check mounts with proper pattern matching
        let mut best_transport: Option<Py<PyAny>> = None;
        let mut best_priority = 0u32;

        for (pattern_obj, transport_obj) in mounts_bound.iter() {
            let pattern: String = pattern_obj.extract()?;
            if url_matches_pattern(&url_str, &pattern) {
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
                    best_transport = Some(transport_obj.unbind());
                }
            }
        }

        if let Some(t) = best_transport {
            return Ok(t);
        }

        // Check environment proxy if trust_env is true
        if self.trust_env {
            if let Some(proxy_url) = get_env_proxy_url(&url_str) {
                let transport = crate::transports::default::AsyncHTTPTransport::create(
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
    fn _mounts(&self, py: Python<'_>) -> Py<PyAny> {
        self.mounts.clone_ref(py).into()
    }

    fn __aenter__<'py>(slf: Bound<'py, Self>, py: Python<'py>) -> PyResult<Bound<'py, PyAny>> {
        let slf_py = slf.clone().unbind();
        if slf.borrow().is_closed {
            return Err(pyo3::exceptions::PyRuntimeError::new_err(
                "Cannot open a closed client",
            ));
        }
        let transport = slf.borrow().transport.clone_ref(py);
        let mounts = slf.borrow().mounts.clone_ref(py);

        pyo3_async_runtimes::tokio::future_into_py(py, async move {
            // Call __aenter__ on main transport
            let coro = Python::attach(|py| {
                let t = transport.bind(py);
                let coro = t.call_method0("__aenter__")?;
                pyo3_async_runtimes::tokio::into_future(coro)
            })?;
            coro.await?;

            // Call __aenter__ on all mounted transports
            let mount_transports: Vec<Py<PyAny>> = Python::attach(|py| {
                let m = mounts.bind(py);
                let mut transports = Vec::new();
                for (_pattern, transport_obj) in m.iter() {
                    transports.push(transport_obj.unbind());
                }
                Ok::<_, PyErr>(transports)
            })?;

            for mt in mount_transports {
                let coro = Python::attach(|py| {
                    let t = mt.bind(py);
                    let coro = t.call_method0("__aenter__")?;
                    pyo3_async_runtimes::tokio::into_future(coro)
                })?;
                coro.await?;
            }

            Ok(slf_py)
        })
    }

    fn __aexit__<'py>(
        &mut self,
        py: Python<'py>,
        exc_type: Option<Bound<'py, PyAny>>,
        exc_val: Option<Bound<'py, PyAny>>,
        exc_tb: Option<Bound<'py, PyAny>>,
    ) -> PyResult<Bound<'py, PyAny>> {
        self.is_closed = true;
        let transport = self.transport.clone_ref(py);
        let mounts = self.mounts.clone_ref(py);

        let exc_info = if let (Some(t), Some(v), Some(tb)) = (exc_type, exc_val, exc_tb) {
            Some((t.unbind(), v.unbind(), tb.unbind()))
        } else {
            None
        };

        pyo3_async_runtimes::tokio::future_into_py(py, async move {
            // Call __aexit__ on main transport
            let coro = Python::attach(|py| {
                let t = transport.bind(py);
                let args = if let Some((ref t_exc, ref v, ref tb)) = exc_info {
                    (
                        t_exc.clone_ref(py).into_bound(py),
                        v.clone_ref(py).into_bound(py),
                        tb.clone_ref(py).into_bound(py),
                    )
                } else {
                    (
                        py.None().into_bound(py),
                        py.None().into_bound(py),
                        py.None().into_bound(py),
                    )
                };
                let coro = t.call_method1("__aexit__", args)?;
                pyo3_async_runtimes::tokio::into_future(coro)
            })?;
            coro.await?;

            // Handle mounted transports
            let mount_transports: Vec<Py<PyAny>> = Python::attach(|py| {
                let m = mounts.bind(py);
                let mut transports = Vec::new();
                for (_pattern, transport_obj) in m.iter() {
                    transports.push(transport_obj.unbind());
                }
                Ok::<_, PyErr>(transports)
            })?;

            for mt in mount_transports {
                // Call __aexit__ on mounted transport
                let coro = Python::attach(|py| {
                    let t = mt.bind(py);
                    let args = if let Some((ref t_exc, ref v, ref tb)) = exc_info {
                        (
                            t_exc.clone_ref(py).into_bound(py),
                            v.clone_ref(py).into_bound(py),
                            tb.clone_ref(py).into_bound(py),
                        )
                    } else {
                        (
                            py.None().into_bound(py),
                            py.None().into_bound(py),
                            py.None().into_bound(py),
                        )
                    };
                    let coro = t.call_method1("__aexit__", args)?;
                    pyo3_async_runtimes::tokio::into_future(coro)
                })?;
                coro.await?;
            }

            Ok(())
        })
    }

    fn __repr__(&self) -> String {
        if self.is_closed {
            "<AsyncClient [closed]>".to_string()
        } else {
            "<AsyncClient>".to_string()
        }
    }
}

#[allow(dead_code)]
struct ReleaseGuard {
    pub stream: Option<Py<PyAny>>,
}

impl Drop for ReleaseGuard {
    fn drop(&mut self) {
        // Don't call aclose here - it should be called explicitly with proper await
        // Calling aclose in Drop using ensure_future doesn't work correctly
        // because it doesn't properly await the coroutine
        if let Some(_stream) = self.stream.take() {
            // Stream will be left unclosed if not explicitly handled
            // This is intentional - aclose should be called explicitly before Drop
        }
    }
}

