use pyo3::prelude::*;
use pyo3::types::{PyDict, PyString};
use pyo3::Python;

use crate::models::{Headers, Request, Response};
use std::io::Read;

/// Synchronous HTTP Transport backed by reqwest with a persistent tokio runtime.
///
/// Uses async `reqwest::Client` driven by a shared single-threaded tokio runtime.
/// The runtime is created once during `create()` and reused for all requests,
/// avoiding the ~300μs overhead of per-request runtime creation.
#[pyclass]
pub struct HTTPTransport {
    client: reqwest::Client,
    rt: tokio::runtime::Runtime,
    proxy_url: Option<String>,
}

impl HTTPTransport {
    pub fn create(
        verify: bool,
        root_cert_path: Option<&str>,
        _http2: bool,
        _limits: Option<&crate::config::Limits>,
        proxy: Option<&crate::config::Proxy>,
        _retries: u32,
    ) -> PyResult<Self> {
        let mut builder = reqwest::Client::builder()
            .danger_accept_invalid_certs(!verify)
            .redirect(reqwest::redirect::Policy::none());

        if let Some(cert_path) = root_cert_path {
            let mut buf = Vec::new();
            std::fs::File::open(cert_path)
                .map_err(|e| {
                    pyo3::exceptions::PyIOError::new_err(format!("Failed to open cert file: {}", e))
                })?
                .read_to_end(&mut buf)
                .map_err(|e| {
                    pyo3::exceptions::PyIOError::new_err(format!("Failed to read cert file: {}", e))
                })?;
            let cert = reqwest::Certificate::from_pem(&buf).map_err(|e| {
                pyo3::exceptions::PyValueError::new_err(format!("Invalid certificate: {}", e))
            })?;
            builder = builder.add_root_certificate(cert);
        }

        if let Some(p) = proxy {
            let proxy_conf = reqwest::Proxy::all(&p.url).map_err(|e| {
                pyo3::exceptions::PyValueError::new_err(format!("Invalid proxy: {}", e))
            })?;
            builder = builder.proxy(proxy_conf);
        }

        let client = builder.build().map_err(|e| {
            pyo3::exceptions::PyRuntimeError::new_err(format!("Failed to create client: {}", e))
        })?;

        // Create a persistent multi-thread tokio runtime (1 worker) for this transport.
        // Must be multi-thread because py.detach() runs on a separate OS thread,
        // and current_thread runtime can only be driven from its creating thread.
        let rt = tokio::runtime::Builder::new_multi_thread()
            .worker_threads(1)
            .enable_all()
            .build()
            .map_err(|e| {
                pyo3::exceptions::PyRuntimeError::new_err(format!(
                    "Failed to create runtime: {}",
                    e
                ))
            })?;

        let proxy_url_str = proxy.map(|p| p.url.clone());
        Ok(HTTPTransport {
            client,
            rt,
            proxy_url: proxy_url_str,
        })
    }

    pub fn send_request(&self, py: Python<'_>, request: &Request) -> PyResult<Response> {
        let url_str = request.url.to_string();
        let parsed_url = reqwest::Url::parse(&url_str).map_err(|e| {
            crate::exceptions::InvalidURL::new_err(format!("Invalid URL: {}", e))
        })?;

        // Extract timeout configuration
        let mut connect_timeout_val = None;
        let mut read_timeout_val = None;
        let mut write_timeout_val = None;
        if let Ok(ext) = request.extensions.bind(py).cast::<PyDict>() {
            if let Some(timeout_obj) = ext.get_item("timeout").ok().flatten() {
                if let Ok(t) = timeout_obj.extract::<crate::config::Timeout>() {
                    connect_timeout_val = t.connect;
                    read_timeout_val = t.read;
                    write_timeout_val = t.write;
                }
            }
        }

        // Use minimum of all specified timeouts as overall request timeout
        let timeout_val: Option<std::time::Duration> =
            [connect_timeout_val, read_timeout_val, write_timeout_val]
                .iter()
                .filter_map(|t| *t)
                .reduce(f64::min)
                .map(std::time::Duration::from_secs_f64);

        let body_bytes = request.content_body.clone();
        let has_body = body_bytes.is_some();
        let stream_response = request.stream_response;

        let method_str = request.method.as_str();
        let method = match method_str {
            "GET" => reqwest::Method::GET,
            "POST" => reqwest::Method::POST,
            "PUT" => reqwest::Method::PUT,
            "DELETE" => reqwest::Method::DELETE,
            "PATCH" => reqwest::Method::PATCH,
            "HEAD" => reqwest::Method::HEAD,
            "OPTIONS" => reqwest::Method::OPTIONS,
            _ => reqwest::Method::from_bytes(method_str.as_bytes())
                .map_err(|e| pyo3::exceptions::PyValueError::new_err(format!("Invalid method: {}", e)))?,
        };

        // Build the reqwest Request under the GIL — borrows headers, avoids clone.
        // Pass pre-parsed URL to skip reqwest's internal URL re-parsing.
        let hdrs = request.headers.bind(py).borrow();
        let mut req_builder = self.client.request(method, parsed_url);
        for (key, value) in &hdrs.raw {
            req_builder = req_builder.header(key.as_slice(), value.as_slice());
        }
        drop(hdrs); // release borrow before moving into closure

        if let Some(b) = body_bytes {
            req_builder = req_builder.body(b);
        }

        if let Some(t) = timeout_val {
            req_builder = req_builder.timeout(t);
        }

        let built_request = req_builder.build().map_err(|e| {
            pyo3::exceptions::PyRuntimeError::new_err(format!("Failed to build request: {}", e))
        })?;

        // Release GIL for blocking I/O only — request already built.
        // handle.block_on() is faster than spawn+oneshot (tested: 6236 vs 5720 req/s).
        let client = self.client.clone();
        let (status_code, resp_headers, body_bytes_result) = py
            .detach(move || {
                self.rt.handle().block_on(async {
                    let response = client.execute(built_request).await?;

                    let status = response.status().as_u16();
                    let headers = response.headers();
                    let mut resp_headers: Vec<(Vec<u8>, Vec<u8>)> =
                        Vec::with_capacity(headers.len());
                    for (k, v) in headers.iter() {
                        resp_headers.push((
                            k.as_str().as_bytes().to_vec(),
                            v.as_bytes().to_vec(),
                        ));
                    }

                    let bytes = response.bytes().await?;
                    Ok((status, resp_headers, Vec::from(bytes)))
                })
            })
            .map_err(|e: reqwest::Error| {
                let msg = format!("{}", e);
                if e.is_timeout() {
                    if e.is_connect()
                        || (connect_timeout_val.is_some()
                            && read_timeout_val.is_none()
                            && write_timeout_val.is_none())
                    {
                        crate::exceptions::ConnectTimeout::new_err(msg)
                    } else if write_timeout_val.is_some()
                        && has_body
                        && read_timeout_val.is_none()
                    {
                        crate::exceptions::WriteTimeout::new_err(msg)
                    } else {
                        crate::exceptions::ReadTimeout::new_err(msg)
                    }
                } else if e.is_connect() {
                    crate::exceptions::ConnectError::new_err(msg)
                } else if e.is_body() || e.is_decode() {
                    if read_timeout_val.is_some() || write_timeout_val.is_some() {
                        crate::exceptions::ReadTimeout::new_err(msg)
                    } else {
                        crate::exceptions::NetworkError::new_err(msg)
                    }
                } else if msg.contains("Unknown Scheme") || msg.contains("URL scheme is not allowed") {
                    crate::exceptions::UnsupportedProtocol::new_err(msg)
                } else {
                    crate::exceptions::NetworkError::new_err(msg)
                }
            })?;

        // Build Response (requires GIL) — use from_raw_byte_pairs for zero-conversion
        let hdrs = Headers::from_raw_byte_pairs(resp_headers);

        let ext = PyDict::new(py);

        // For streaming, wrap the bytes in a cursor-based iterator
        let (content_bytes, stream_obj) = if stream_response {
            let cursor = std::io::Cursor::new(body_bytes_result);
            let blocking_iter =
                crate::types::BlockingBytesIterator::new(Box::new(cursor) as Box<dyn Read + Send>);
            let py_iter = Py::new(py, blocking_iter)?.into_any();
            (None, Some(py_iter))
        } else {
            (Some(body_bytes_result), None)
        };

        let has_stream = stream_obj.is_some();

        Ok(Response {
            status_code,
            headers: Py::new(py, hdrs)?,
            extensions: ext.into(),
            request: None,
            history: Vec::new(),
            content_bytes,
            stream: stream_obj,
            default_encoding: PyString::intern(py, "utf-8").into_any().unbind(),
            default_encoding_override: None,
            elapsed: None,
            is_closed_flag: false,
            is_stream_consumed: false,
            was_streaming: has_stream,
            text_accessed: std::sync::atomic::AtomicBool::new(false),
            num_bytes_downloaded_counter: std::sync::Arc::new(std::sync::atomic::AtomicUsize::new(
                0,
            )),
        })
    }
}

impl HTTPTransport {
    /// Send multiple requests concurrently using tokio::spawn + Semaphore.
    /// All requests are built under the GIL, then dispatched concurrently with GIL released.
    /// Results are collected in order and converted back to Response objects.
    pub fn send_batch_requests(
        &self,
        py: Python<'_>,
        requests: &[Request],
        max_concurrency: usize,
    ) -> PyResult<Vec<Result<Response, PyErr>>> {
        // Phase 1: Build all reqwest requests under the GIL
        let mut built_requests = Vec::with_capacity(requests.len());
        for request in requests {
            let url_str = request.url.to_string();
            let parsed_url = reqwest::Url::parse(&url_str).map_err(|e| {
                crate::exceptions::InvalidURL::new_err(format!("Invalid URL: {}", e))
            })?;

            // Extract timeout
            let mut timeout_val: Option<std::time::Duration> = None;
            if let Ok(ext) = request.extensions.bind(py).cast::<PyDict>() {
                if let Some(timeout_obj) = ext.get_item("timeout").ok().flatten() {
                    if let Ok(t) = timeout_obj.extract::<crate::config::Timeout>() {
                        timeout_val = [t.connect, t.read, t.write]
                            .iter()
                            .filter_map(|t| *t)
                            .reduce(f64::min)
                            .map(std::time::Duration::from_secs_f64);
                    }
                }
            }

            let method_str = request.method.as_str();
            let method = match method_str {
                "GET" => reqwest::Method::GET,
                "POST" => reqwest::Method::POST,
                "PUT" => reqwest::Method::PUT,
                "DELETE" => reqwest::Method::DELETE,
                "PATCH" => reqwest::Method::PATCH,
                "HEAD" => reqwest::Method::HEAD,
                "OPTIONS" => reqwest::Method::OPTIONS,
                _ => reqwest::Method::from_bytes(method_str.as_bytes())
                    .map_err(|e| pyo3::exceptions::PyValueError::new_err(format!("Invalid method: {}", e)))?,
            };

            let hdrs = request.headers.bind(py).borrow();
            let mut req_builder = self.client.request(method, parsed_url);
            for (key, value) in &hdrs.raw {
                req_builder = req_builder.header(key.as_slice(), value.as_slice());
            }
            drop(hdrs);

            if let Some(b) = &request.content_body {
                req_builder = req_builder.body(b.clone());
            }
            if let Some(t) = timeout_val {
                req_builder = req_builder.timeout(t);
            }

            let built = req_builder.build().map_err(|e| {
                pyo3::exceptions::PyRuntimeError::new_err(format!("Failed to build request: {}", e))
            })?;
            built_requests.push(built);
        }

        // Phase 2: Release GIL, dispatch all concurrently
        let client = self.client.clone();
        let results: Vec<Result<(u16, Vec<(Vec<u8>, Vec<u8>)>, Vec<u8>), String>> = py
            .detach(move || {
                self.rt.handle().block_on(async {
                    let semaphore = std::sync::Arc::new(tokio::sync::Semaphore::new(max_concurrency));
                    let mut handles = Vec::with_capacity(built_requests.len());

                    for built_request in built_requests {
                        let client = client.clone();
                        let sem = semaphore.clone();
                        handles.push(tokio::spawn(async move {
                            let _permit = sem.acquire().await.unwrap();
                            let response = client.execute(built_request).await
                                .map_err(|e| format!("{}", e))?;
                            let status = response.status().as_u16();
                            let headers = response.headers();
                            let mut resp_headers: Vec<(Vec<u8>, Vec<u8>)> =
                                Vec::with_capacity(headers.len());
                            for (k, v) in headers.iter() {
                                resp_headers.push((
                                    k.as_str().as_bytes().to_vec(),
                                    v.as_bytes().to_vec(),
                                ));
                            }
                            let bytes = response.bytes().await
                                .map_err(|e| format!("{}", e))?;
                            Ok::<_, String>((status, resp_headers, Vec::from(bytes)))
                        }));
                    }

                    let mut results = Vec::with_capacity(handles.len());
                    for handle in handles {
                        match handle.await {
                            Ok(result) => results.push(result),
                            Err(e) => results.push(Err(format!("Task panicked: {}", e))),
                        }
                    }
                    results
                })
            });

        // Phase 3: Re-acquire GIL, build Response objects
        let mut responses = Vec::with_capacity(results.len());
        for result in results {
            match result {
                Ok((status_code, resp_headers, body_bytes)) => {
                    let hdrs = Headers::from_raw_byte_pairs(resp_headers);
                    let ext = PyDict::new(py);
                    let response = Response {
                        status_code,
                        headers: Py::new(py, hdrs)?,
                        extensions: ext.into(),
                        request: None,
                        history: Vec::new(),
                        content_bytes: Some(body_bytes),
                        stream: None,
                        default_encoding: PyString::intern(py, "utf-8").into_any().unbind(),
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
                    responses.push(Ok(response));
                }
                Err(msg) => {
                    responses.push(Err(
                        crate::exceptions::NetworkError::new_err(msg),
                    ));
                }
            }
        }

        Ok(responses)
    }
}

#[pymethods]
impl HTTPTransport {
    #[new]
    #[pyo3(signature = (verify=true, cert=None, http2=false, limits=None, proxy=None, retries=0))]
    fn new(
        verify: bool,
        cert: Option<&str>,
        http2: bool,
        limits: Option<&crate::config::Limits>,
        proxy: Option<&Bound<'_, PyAny>>,
        retries: u32,
    ) -> PyResult<Self> {
        let proxy_obj = if let Some(p) = proxy {
            if let Ok(proxy_ref) = p.extract::<crate::config::Proxy>() {
                Some(proxy_ref)
            } else if let Ok(s) = p.extract::<String>() {
                Some(crate::config::Proxy::create_from_url(&s)?)
            } else {
                None
            }
        } else {
            None
        };
        Self::create(verify, cert, http2, limits, proxy_obj.as_ref(), retries)
    }

    fn handle_request(&self, py: Python<'_>, request: &Request) -> PyResult<Response> {
        self.send_request(py, request)
    }

    fn close(&self) -> PyResult<()> {
        Ok(())
    }

    #[getter]
    fn _pool(&self, py: Python<'_>) -> PyResult<Py<PyAny>> {
        if let Some(ref proxy_url) = self.proxy_url {
            if proxy_url.starts_with("socks5://") || proxy_url.starts_with("socks5h://") {
                let httpcore = py.import("httpcore")?;
                let pool = httpcore
                    .getattr("SOCKSProxy")?
                    .call1((proxy_url.as_str(),))?;
                Ok(pool.unbind())
            } else {
                let httpcore = py.import("httpcore")?;
                let pool = httpcore
                    .getattr("HTTPProxy")?
                    .call1((proxy_url.as_str(),))?;
                Ok(pool.unbind())
            }
        } else {
            Ok(py.None())
        }
    }

    fn __enter__(slf: PyRef<'_, Self>) -> PyRef<'_, Self> {
        slf
    }
    fn __exit__(
        &self,
        _exc_type: Option<&Bound<'_, PyAny>>,
        _exc_value: Option<&Bound<'_, PyAny>>,
        _traceback: Option<&Bound<'_, PyAny>>,
    ) -> PyResult<()> {
        self.close()
    }
}

#[pyclass]
pub struct AsyncHTTPTransport {
    client: reqwest::Client,
    proxy_url: Option<String>,
    pool_semaphore: Option<std::sync::Arc<tokio::sync::Semaphore>>,
}

impl AsyncHTTPTransport {
    pub fn create(
        verify: bool,
        root_cert_path: Option<&str>,
        _http2: bool,
        limits: Option<&crate::config::Limits>,
        proxy: Option<&crate::config::Proxy>,
        _retries: u32,
    ) -> PyResult<Self> {
        let mut builder = reqwest::Client::builder()
            .danger_accept_invalid_certs(!verify)
            .redirect(reqwest::redirect::Policy::none());

        // Apply pool size limits
        let pool_semaphore = if let Some(l) = limits {
            if let Some(max_conn) = l.max_connections {
                builder = builder.pool_max_idle_per_host(max_conn as usize);
                Some(std::sync::Arc::new(tokio::sync::Semaphore::new(
                    max_conn as usize,
                )))
            } else {
                None
            }
        } else {
            None
        };

        if let Some(cert_path) = root_cert_path {
            let mut buf = Vec::new();
            std::fs::File::open(cert_path)
                .map_err(|e| {
                    pyo3::exceptions::PyIOError::new_err(format!("Failed to open cert file: {}", e))
                })?
                .read_to_end(&mut buf)
                .map_err(|e| {
                    pyo3::exceptions::PyIOError::new_err(format!("Failed to read cert file: {}", e))
                })?;
            let cert = reqwest::Certificate::from_pem(&buf).map_err(|e| {
                pyo3::exceptions::PyValueError::new_err(format!("Invalid certificate: {}", e))
            })?;
            builder = builder.add_root_certificate(cert);
        }

        if let Some(p) = proxy {
            let proxy_conf = reqwest::Proxy::all(&p.url).map_err(|e| {
                pyo3::exceptions::PyValueError::new_err(format!("Invalid proxy: {}", e))
            })?;
            builder = builder.proxy(proxy_conf);
        }

        let client = builder.build().map_err(|e| {
            pyo3::exceptions::PyRuntimeError::new_err(format!("Failed to create client: {}", e))
        })?;
        let proxy_url_str = proxy.map(|p| p.url.clone());
        Ok(AsyncHTTPTransport {
            client,
            proxy_url: proxy_url_str,
            pool_semaphore,
        })
    }
}

#[pymethods]
impl AsyncHTTPTransport {
    #[new]
    #[pyo3(signature = (verify=true, cert=None, http2=false, limits=None, proxy=None, retries=0))]
    fn new(
        verify: bool,
        cert: Option<&str>,
        http2: bool,
        limits: Option<&crate::config::Limits>,
        proxy: Option<&Bound<'_, PyAny>>,
        retries: u32,
    ) -> PyResult<Self> {
        let proxy_obj = if let Some(p) = proxy {
            if let Ok(proxy_ref) = p.extract::<crate::config::Proxy>() {
                Some(proxy_ref)
            } else if let Ok(s) = p.extract::<String>() {
                Some(crate::config::Proxy::create_from_url(&s)?)
            } else {
                None
            }
        } else {
            None
        };
        let transport = Self::create(verify, cert, http2, limits, proxy_obj.as_ref(), retries)?;
        Ok(transport)
    }

    fn handle_async_request<'py>(
        &self,
        py: Python<'py>,
        request: &Request,
    ) -> PyResult<Bound<'py, PyAny>> {
        let client = self.client.clone();
        let url_str = request.url.to_string();
        let method_str = request.method.clone();
        let raw_headers = request.headers.bind(py).borrow().get_raw_items_owned();
        let body = request.content_body.clone();

        let mut connect_timeout_val = None;
        let mut read_timeout_val = None;
        let mut write_timeout_val = None;
        let mut pool_timeout_val = None;
        if let Ok(ext) = request.extensions.bind(py).cast::<PyDict>() {
            if let Some(timeout_obj) = ext.get_item("timeout").ok().flatten() {
                if let Ok(t) = timeout_obj.extract::<crate::config::Timeout>() {
                    connect_timeout_val = t.connect;
                    read_timeout_val = t.read;
                    write_timeout_val = t.write;
                    pool_timeout_val = t.pool;
                }
            }
        }

        // Use the minimum of all specified timeouts as the overall request timeout
        let timeout_val: Option<std::time::Duration> =
            [connect_timeout_val, read_timeout_val, write_timeout_val]
                .iter()
                .filter_map(|t| *t)
                .reduce(f64::min)
                .map(std::time::Duration::from_secs_f64);

        let pool_sem = self.pool_semaphore.clone();

        let stream_response_flag = request.stream_response;

        pyo3_async_runtimes::tokio::future_into_py(py, async move {
            // Acquire pool semaphore permit if we have one
            let pool_permit = if let Some(ref sem) = pool_sem {
                let pool_dur = pool_timeout_val
                    .map(std::time::Duration::from_secs_f64)
                    .unwrap_or(std::time::Duration::from_secs(30));
                match tokio::time::timeout(pool_dur, sem.clone().acquire_owned()).await {
                    Ok(Ok(permit)) => Some(permit),
                    Ok(Err(_closed)) => {
                        return Err(crate::exceptions::PoolTimeout::new_err(
                            "Connection pool is closed".to_string(),
                        ));
                    }
                    Err(_elapsed) => {
                        return Err(crate::exceptions::PoolTimeout::new_err(
                            "Timed out waiting for a connection from the pool".to_string(),
                        ));
                    }
                }
            } else {
                None
            };

            let method = match method_str.as_str() {
                "GET" => reqwest::Method::GET,
                "POST" => reqwest::Method::POST,
                "PUT" => reqwest::Method::PUT,
                "DELETE" => reqwest::Method::DELETE,
                "PATCH" => reqwest::Method::PATCH,
                "HEAD" => reqwest::Method::HEAD,
                "OPTIONS" => reqwest::Method::OPTIONS,
                _ => reqwest::Method::from_bytes(method_str.as_bytes()).map_err(|e| {
                    pyo3::exceptions::PyValueError::new_err(format!("Invalid method: {}", e))
                })?,
            };

            let mut req_builder = client.request(method, &url_str);
            for (key, value) in &raw_headers {
                req_builder = req_builder.header(key.as_slice(), value.as_slice());
            }
            if let Some(ref b) = body {
                req_builder = req_builder.body(b.clone());
            }

            if let Some(t) = timeout_val {
                req_builder = req_builder.timeout(t);
            }

            let response = req_builder.send().await.map_err(|e| {
                if e.is_timeout() {
                    // Determine which type of timeout based on what was configured
                    // and whether it's a connection-phase error
                    if e.is_connect()
                        || (connect_timeout_val.is_some()
                            && read_timeout_val.is_none()
                            && write_timeout_val.is_none())
                    {
                        crate::exceptions::ConnectTimeout::new_err(format!("{}", e))
                    } else if write_timeout_val.is_some()
                        && body.is_some()
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
            let headers = response.headers();
            let mut resp_headers: Vec<(Vec<u8>, Vec<u8>)> =
                Vec::with_capacity(headers.len());
            for (k, v) in headers.iter() {
                resp_headers.push((
                    k.as_str().as_bytes().to_vec(),
                    v.as_bytes().to_vec(),
                ));
            }

            enum BodyType {
                Bytes(Vec<u8>),
                Stream(
                    Box<dyn futures::Stream<Item = reqwest::Result<bytes::Bytes>> + Send + Unpin>,
                ),
            }

            let body_val = if stream_response_flag {
                BodyType::Stream(Box::new(response.bytes_stream()))
            } else {
                let bytes = response.bytes().await.map_err(|e| {
                    if e.is_timeout() {
                        crate::exceptions::ReadTimeout::new_err(format!("{}", e))
                    } else {
                        crate::exceptions::ReadError::new_err(format!("{}", e))
                    }
                })?;
                BodyType::Bytes(Vec::from(bytes))
            };

            pyo3::Python::attach(|py| {
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

                let (content_bytes, stream_obj) = match body_val {
                    BodyType::Bytes(b) => (Some(b), None),
                    BodyType::Stream(s) => {
                        let stream_wrapper = if let Some(permit) = pool_permit {
                            crate::types::AsyncResponseStream::new_with_permit(s, permit)
                        } else {
                            crate::types::AsyncResponseStream::new(s)
                        };
                        let py_stream = Py::new(py, stream_wrapper)?.into_any();
                        (None, Some(py_stream))
                    }
                };

                let has_stream = stream_obj.is_some();

                Ok(Response {
                    status_code,
                    headers: Py::new(py, hdrs)?,
                    extensions: ext.into(),
                    request: None,
                    history: Vec::new(),
                    content_bytes: content_bytes,
                    stream: stream_obj,
                    default_encoding: PyString::intern(py, "utf-8").into_any().unbind(),
                    default_encoding_override: None,
                    elapsed: None,
                    is_closed_flag: false,
                    is_stream_consumed: false,
                    was_streaming: has_stream,
                    text_accessed: std::sync::atomic::AtomicBool::new(false),
                    num_bytes_downloaded_counter: std::sync::Arc::new(
                        std::sync::atomic::AtomicUsize::new(0),
                    ),
                })
            })
        })
    }

    fn aclose(&self) -> PyResult<()> {
        Ok(())
    }

    #[getter]
    fn _pool(&self, py: Python<'_>) -> PyResult<Py<PyAny>> {
        if let Some(ref proxy_url) = self.proxy_url {
            if proxy_url.starts_with("socks5://") || proxy_url.starts_with("socks5h://") {
                let httpcore = py.import("httpcore")?;
                let pool = httpcore
                    .getattr("AsyncSOCKSProxy")?
                    .call1((proxy_url.as_str(),))?;
                Ok(pool.unbind())
            } else {
                let httpcore = py.import("httpcore")?;
                let pool = httpcore
                    .getattr("AsyncHTTPProxy")?
                    .call1((proxy_url.as_str(),))?;
                Ok(pool.unbind())
            }
        } else {
            Ok(py.None())
        }
    }

    fn __aenter__<'py>(slf: Bound<'py, Self>, py: Python<'py>) -> PyResult<Bound<'py, PyAny>> {
        let slf_py = slf.unbind();
        pyo3_async_runtimes::tokio::future_into_py(py, async move { Ok(slf_py) })
    }

    fn __aexit__<'py>(
        slf: Bound<'py, Self>,
        py: Python<'py>,
        _exc_type: Option<Bound<'py, PyAny>>,
        _exc_value: Option<Bound<'py, PyAny>>,
        _traceback: Option<Bound<'py, PyAny>>,
    ) -> PyResult<Bound<'py, PyAny>> {
        let slf_py = slf.clone().unbind();
        pyo3_async_runtimes::tokio::future_into_py(py, async move {
            Python::attach(|py| {
                let t = slf_py.bind(py);
                t.call_method0("aclose")?;
                Ok(())
            })
        })
    }
}

pub fn register(m: &Bound<'_, PyModule>) -> PyResult<()> {
    m.add_class::<HTTPTransport>()?;
    m.add_class::<AsyncHTTPTransport>()?;
    Ok(())
}
