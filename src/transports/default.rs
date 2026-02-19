use pyo3::prelude::*;
use pyo3::types::{PyDict, PyString};
use pyo3::Python;

use crate::models::{Headers, Request, Response};
use super::helpers::{
    parse_method, extract_timeout_from_extensions, compute_effective_timeout,
    map_reqwest_error, map_reqwest_error_simple, build_default_response, http_version_str,
    raw_results_to_pylist,
};
use std::io::Read;

/// Synchronous HTTP Transport backed by reqwest with a persistent tokio runtime.
///
/// Uses async `reqwest::Client` driven by a shared single-threaded tokio runtime.
/// The runtime is created once during `create()` and reused for all requests,
/// avoiding the ~300μs overhead of per-request runtime creation.
#[pyclass]
pub struct HTTPTransport {
    pub(crate) client: reqwest::Client,
    /// Kept alive for RAII — the `handle` references this runtime.
    _rt: tokio::runtime::Runtime,
    pub(crate) handle: tokio::runtime::Handle,
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
            .redirect(reqwest::redirect::Policy::none())
            .pool_max_idle_per_host(64)
            .pool_idle_timeout(None)
            .tcp_nodelay(true)
            .tcp_keepalive(std::time::Duration::from_secs(60))
            .http1_only()
            .no_proxy();

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
        let handle = rt.handle().clone();
        Ok(HTTPTransport {
            client,
            _rt: rt,
            handle,
            proxy_url: proxy_url_str,
        })
    }

    pub fn send_request(&self, py: Python<'_>, request: &Request) -> PyResult<Response> {
        let url_str = request.url.to_string();
        let parsed_url = reqwest::Url::parse(&url_str).map_err(|e| {
            crate::exceptions::InvalidURL::new_err(format!("Invalid URL: {}", e))
        })?;

        let (connect_timeout_val, read_timeout_val, write_timeout_val, _pool) =
            extract_timeout_from_extensions(py, &request.extensions);
        let timeout_val = compute_effective_timeout(connect_timeout_val, read_timeout_val, write_timeout_val);

        let body_bytes = request.content_body.clone();
        let has_body = body_bytes.is_some();
        let stream_response = request.stream_response;

        let method = parse_method(request.method.as_str())?;

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

        let client = self.client.clone();
        let (status_code, resp_headers, body_bytes_result) = py
            .detach(move || {
                self.handle.block_on(async {
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
                    Ok((status, resp_headers, bytes.to_vec()))
                })
            })
            .map_err(|e: reqwest::Error| {
                map_reqwest_error(e, connect_timeout_val, read_timeout_val, write_timeout_val, has_body)
            })?;

        if stream_response {
            let hdrs = Headers::from_raw_byte_pairs(resp_headers);
            let ext = PyDict::new(py);
            let cursor = std::io::Cursor::new(body_bytes_result);
            let blocking_iter =
                crate::types::BlockingBytesIterator::new(Box::new(cursor) as Box<dyn Read + Send>);
            let py_iter = Py::new(py, blocking_iter)?.into_any();
            Ok(Response {
                status_code,
                headers: Some(Py::new(py, hdrs)?),
                    lazy_headers: None,
                extensions: Some(ext.into()),
                request: None,
                lazy_request_method: None,
                lazy_request_url: None,
                history: Vec::new(),
                content_bytes: None,
                stream: Some(py_iter),
                default_encoding: PyString::intern(py, "utf-8").into_any().unbind(),
                default_encoding_override: None,
                elapsed: None,
                is_closed_flag: false,
                is_stream_consumed: false,
                was_streaming: true,
                text_accessed: std::sync::atomic::AtomicBool::new(false),
                num_bytes_downloaded_counter: std::sync::Arc::new(std::sync::atomic::AtomicUsize::new(0)),
            })
        } else {
            build_default_response(py, status_code, resp_headers, body_bytes_result)
        }
    }

    /// Fast-path: send a raw request, return (status, headers_dict, body) Python tuple directly.
    /// Eliminates ALL intermediate allocations by building Python objects immediately.
    pub fn send_raw(
        &self,
        py: Python<'_>,
        method: reqwest::Method,
        url: &str,
        headers: Option<&pyo3::Bound<'_, pyo3::types::PyDict>>,
        body: Option<Vec<u8>>,
        timeout_secs: Option<f64>,
    ) -> PyResult<Py<PyAny>> {
        use pyo3::types::{PyBytes, PyDict, PyTuple};
        use pyo3::IntoPyObject;

        let parsed_url = reqwest::Url::parse(url).map_err(|e| {
            crate::exceptions::InvalidURL::new_err(format!("Invalid URL: {}", e))
        })?;

        let mut req_builder = self.client.request(method, parsed_url);

        if let Some(h) = headers {
            for (k, v) in h.iter() {
                let key: &str = k.extract()?;
                let val: &str = v.extract()?;
                req_builder = req_builder.header(key, val);
            }
        }

        if let Some(b) = body {
            req_builder = req_builder.body(b);
        }

        if let Some(t) = timeout_secs {
            req_builder = req_builder.timeout(std::time::Duration::from_secs_f64(t));
        }

        let built_request = req_builder.build().map_err(|e| {
            pyo3::exceptions::PyRuntimeError::new_err(format!("Failed to build request: {}", e))
        })?;

        let handle = self.handle.clone();
        let client = self.client.clone();
        let result = py
            .detach(move || {
                handle.block_on(async {
                    let response = client.execute(built_request).await?;
                    let status = response.status().as_u16();
                    let owned_hdrs: Vec<(String, Vec<u8>)> = response
                        .headers()
                        .iter()
                        .map(|(k, v)| (k.as_str().to_owned(), v.as_bytes().to_vec()))
                        .collect();
                    let body = response.bytes().await?;
                    Ok::<_, reqwest::Error>((status, owned_hdrs, body))
                })
            })
            .map_err(|e: reqwest::Error| map_reqwest_error_simple(e))?;

        let (status, resp_headers, body_bytes) = result;

        let dict = PyDict::new(py);
        for (k, v) in &resp_headers {
            let val_str = std::str::from_utf8(v).unwrap_or("");
            let key_intern = crate::utils::intern_header_key(py, k.as_str());
            dict.set_item(key_intern.as_any(), val_str)?;
        }
        let body_py = PyBytes::new(py, &body_bytes);
        let tuple = PyTuple::new(py, &[
            status.into_pyobject(py)?.into_any().as_borrowed(),
            dict.as_any().as_borrowed(),
            body_py.as_any().as_borrowed(),
        ])?;
        Ok(tuple.unbind().into())
    }

    /// Fast-path returning an actual `Response`, avoiding Python `Request` tracking overhead
    pub fn send_fast(
        &self,
        py: Python<'_>,
        method: reqwest::Method,
        url: &str,
        headers: Vec<(String, String)>,
        timeout_secs: Option<f64>,
    ) -> PyResult<Response> {
        let parsed_url = reqwest::Url::parse(url).map_err(|e| {
            crate::exceptions::InvalidURL::new_err(format!("Invalid URL: {}", e))
        })?;

        let mut req_builder = self.client.request(method, parsed_url);
        for (k, v) in headers {
            req_builder = req_builder.header(k, v);
        }
        if let Some(t) = timeout_secs {
            req_builder = req_builder.timeout(std::time::Duration::from_secs_f64(t));
        }

        let built_request = req_builder.build().map_err(|e| {
            pyo3::exceptions::PyRuntimeError::new_err(format!("Failed to build request: {}", e))
        })?;

        let handle = self.handle.clone();
        let client = self.client.clone();
        
        let (status_code, resp_headers, body_bytes_result) = py
            .detach(move || {
                handle.block_on(async {
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
                    Ok::<_, reqwest::Error>((status, resp_headers, bytes.to_vec()))
                })
            })
            .map_err(|e: reqwest::Error| map_reqwest_error_simple(e))?;

        let res = build_default_response(py, status_code, resp_headers, body_bytes_result)?;
        let ext = res.extensions.as_ref().unwrap().bind(py);
        if let Ok(d) = ext.cast::<pyo3::types::PyDict>() {
            if !d.contains("http_version")? {
                d.set_item("http_version", pyo3::types::PyBytes::new(py, b"HTTP/1.1"))?;
            }
        }
        Ok(res)
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
        let mut built_requests = Vec::with_capacity(requests.len());
        for request in requests {
            let url_str = request.url.to_string();
            let parsed_url = reqwest::Url::parse(&url_str).map_err(|e| {
                crate::exceptions::InvalidURL::new_err(format!("Invalid URL: {}", e))
            })?;

            let (connect_t, read_t, write_t, _pool_t) =
                extract_timeout_from_extensions(py, &request.extensions);
            let timeout_val = compute_effective_timeout(connect_t, read_t, write_t);

            let method_str = request.method.as_str();
            let method = parse_method(method_str)?;

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

        let client = self.client.clone();
        let results: Vec<Result<(u16, Vec<(Vec<u8>, Vec<u8>)>, Vec<u8>), String>> = py
            .detach(move || {
                self.handle.block_on(async {
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

        let mut responses = Vec::with_capacity(results.len());
        for result in results {
            match result {
                Ok((status_code, resp_headers, body_bytes)) => {
                    let response = build_default_response(py, status_code, resp_headers, body_bytes)?;
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

    /// Like send_batch_requests, but returns raw (status, headers_dict, body) tuples.
    /// Skips Response construction entirely for maximum speed.
    pub fn send_batch_raw(
        &self,
        py: Python<'_>,
        requests: &[(reqwest::Method, String, Option<Vec<(String, String)>>, Option<Vec<u8>>, Option<f64>)],
        max_concurrency: usize,
    ) -> PyResult<Py<pyo3::types::PyList>> {

        let mut built_requests = Vec::with_capacity(requests.len());
        for (method, url, headers, body, timeout) in requests {
            let parsed_url = reqwest::Url::parse(url).map_err(|e| {
                crate::exceptions::InvalidURL::new_err(format!("Invalid URL: {}", e))
            })?;
            let mut req_builder = self.client.request(method.clone(), parsed_url);
            if let Some(hdrs) = headers {
                for (k, v) in hdrs {
                    req_builder = req_builder.header(k.as_str(), v.as_str());
                }
            }
            if let Some(b) = body {
                req_builder = req_builder.body(b.clone());
            }
            if let Some(t) = timeout {
                req_builder = req_builder.timeout(std::time::Duration::from_secs_f64(*t));
            }
            let built = req_builder.build().map_err(|e| {
                pyo3::exceptions::PyRuntimeError::new_err(format!("Failed to build request: {}", e))
            })?;
            built_requests.push(built);
        }

        let client = self.client.clone();
        let results: Vec<Result<(u16, Vec<(String, Vec<u8>)>, Vec<u8>), String>> = py
            .detach(move || {
                self.handle.block_on(async {
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
                            let owned_hdrs: Vec<(String, Vec<u8>)> = response
                                .headers()
                                .iter()
                                .map(|(k, v)| (k.as_str().to_owned(), v.as_bytes().to_vec()))
                                .collect();
                            let body = response.bytes().await
                                .map_err(|e| format!("{}", e))?;
                            Ok::<_, String>((status, owned_hdrs, Vec::from(body)))
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

        let py_list = raw_results_to_pylist(py, results)?;
        Ok(py_list)
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
            .redirect(reqwest::redirect::Policy::none())
            .pool_max_idle_per_host(64)      // default=1, keep many warm connections
            .pool_idle_timeout(None)          // never expire idle connections
            .tcp_nodelay(true)                // disable Nagle: send immediately
            .tcp_keepalive(std::time::Duration::from_secs(60))
            .http1_only()                     // skip HTTP/2 negotiation overhead
            .no_proxy();                      // skip proxy env var checks

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

    /// Get a clone of the underlying reqwest::Client for fast-path usage.
    pub fn get_client(&self) -> reqwest::Client {
        self.client.clone()
    }

    /// Get a clone of the pool semaphore for fast-path usage.
    pub fn get_pool_semaphore(&self) -> Option<std::sync::Arc<tokio::sync::Semaphore>> {
        self.pool_semaphore.clone()
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

        let (connect_timeout_val, read_timeout_val, write_timeout_val, pool_timeout_val) =
            extract_timeout_from_extensions(py, &request.extensions);
        let timeout_val = compute_effective_timeout(connect_timeout_val, read_timeout_val, write_timeout_val);

        let pool_sem = self.pool_semaphore.clone();

        let stream_response_flag = request.stream_response;

        pyo3_async_runtimes::tokio::future_into_py(py, async move {
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

            let method = parse_method(method_str.as_str())?;

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
                map_reqwest_error(e, connect_timeout_val, read_timeout_val, write_timeout_val, body.is_some())
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
                BodyType::Bytes(bytes.to_vec())
            };

            pyo3::Python::attach(|py| {
                let hdrs = Headers::from_raw_byte_pairs(resp_headers);
                let ext = PyDict::new(py);
                ext.set_item("http_version", http_version_str(version).as_bytes())?;

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
                    lazy_headers: None,
                    headers: Some(Py::new(py, hdrs)?),
                    extensions: Some(ext.into()),
                    request: None,
                    lazy_request_method: None,
                    lazy_request_url: None,
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
                    num_bytes_downloaded_counter: std::sync::Arc::new(
                        std::sync::atomic::AtomicUsize::new(0),
                    ),
                })
            })
        })
    }

    /// Send multiple requests concurrently in a single batch.
    /// Phase 1: Extract all request data under one GIL hold.
    /// Phase 2: Dispatch all concurrently in Rust (no GIL needed).
    /// Phase 3: Re-acquire GIL once to build all Response objects.
    /// This minimizes GIL contention for concurrent workloads.
    #[pyo3(signature = (requests, max_concurrency=50))]
    fn handle_async_requests_batch<'py>(
        &self,
        py: Python<'py>,
        requests: Vec<Py<Request>>,
        max_concurrency: usize,
    ) -> PyResult<Bound<'py, PyAny>> {
        let client = self.client.clone();

        struct PreparedRequest {
            url_str: String,
            method_str: String,
            raw_headers: Vec<(Vec<u8>, Vec<u8>)>,
            body: Option<Vec<u8>>,
            timeout_val: Option<std::time::Duration>,
        }

        let mut prepared: Vec<PreparedRequest> = Vec::with_capacity(requests.len());
        for req_py in &requests {
            let req = req_py.bind(py).borrow();
            let url_str = req.url.to_string();
            let method_str = req.method.clone();
            let raw_headers = req.headers.bind(py).borrow().get_raw_items_owned();
            let body = req.content_body.clone();

            let (connect_t, read_t, write_t, _pool_t) =
                extract_timeout_from_extensions(py, &req.extensions);
            let timeout_val = compute_effective_timeout(connect_t, read_t, write_t);

            prepared.push(PreparedRequest {
                url_str,
                method_str,
                raw_headers,
                body,
                timeout_val,
            });
        }

        pyo3_async_runtimes::tokio::future_into_py(py, async move {
            let semaphore = std::sync::Arc::new(tokio::sync::Semaphore::new(max_concurrency));
            let mut handles = Vec::with_capacity(prepared.len());

            for prep in prepared {
                let client = client.clone();
                let sem = semaphore.clone();
                handles.push(tokio::spawn(async move {
                    let _permit = sem.acquire().await.unwrap();

                    let method = parse_method(prep.method_str.as_str())
                        .map_err(|e| format!("{}", e))?;

                    let mut req_builder = client.request(method, &prep.url_str);
                    for (key, value) in &prep.raw_headers {
                        req_builder = req_builder.header(key.as_slice(), value.as_slice());
                    }
                    if let Some(b) = prep.body {
                        req_builder = req_builder.body(b);
                    }
                    if let Some(t) = prep.timeout_val {
                        req_builder = req_builder.timeout(t);
                    }

                    let response = req_builder.send().await.map_err(|e| format!("{}", e))?;
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
                    let bytes = response.bytes().await.map_err(|e| format!("{}", e))?;
                    Ok::<_, String>((status, resp_headers, Vec::from(bytes)))
                }));
            }

            let mut raw_results = Vec::with_capacity(handles.len());
            for handle in handles {
                match handle.await {
                    Ok(result) => raw_results.push(result),
                    Err(e) => raw_results.push(Err(format!("Task panicked: {}", e))),
                }
            }

            Python::attach(|py| {
                let py_list = pyo3::types::PyList::empty(py);
                for (i, result) in raw_results.into_iter().enumerate() {
                    match result {
                        Ok((status_code, resp_headers, body_bytes)) => {
                            let mut response = build_default_response(py, status_code, resp_headers, body_bytes)?;
                            if let Ok(ext) = response.extensions.as_ref().unwrap().bind(py).cast::<PyDict>() {
                                ext.set_item("http_version", pyo3::types::PyBytes::new(py, b"HTTP/1.1"))?;
                            }
                            let req_ref = requests[i].bind(py).borrow();
                            response.request = Some(req_ref.clone());
                            py_list.append(Py::new(py, response)?)?;
                        }
                        Err(msg) => {
                            let err = crate::exceptions::NetworkError::new_err(msg);
                            py_list.append(err.value(py))?;
                        }
                    }
                }
                Ok(py_list.unbind())
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
