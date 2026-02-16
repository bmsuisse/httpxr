use pyo3::prelude::*;
use pyo3::types::PyDict;
use pyo3::Python;

use crate::models::{Headers, Request, Response};
use std::io::Read;
use std::sync::Arc;

/// Synchronous HTTP Transport backed by ureq.
#[pyclass]
pub struct HTTPTransport {
    agent: ureq::Agent,
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
        let mut builder = ureq::AgentBuilder::new()
            .redirects(0); // Disable redirects to let httpr handle them manually if needed

        let mut tls_builder = native_tls::TlsConnector::builder();

        if let Some(path) = root_cert_path {
             let mut buf = Vec::new();
             std::fs::File::open(path)
                 .map_err(|e| pyo3::exceptions::PyIOError::new_err(format!("Failed to open cert file: {}", e)))?
                 .read_to_end(&mut buf)
                 .map_err(|e| pyo3::exceptions::PyIOError::new_err(format!("Failed to read cert file: {}", e)))?;
             
             let cert = native_tls::Certificate::from_pem(&buf)
                 .map_err(|e| pyo3::exceptions::PyValueError::new_err(format!("Invalid certificate: {}", e)))?;
             
             tls_builder.add_root_certificate(cert);
        }

        if !verify {
            tls_builder.danger_accept_invalid_certs(true);
            tls_builder.danger_accept_invalid_hostnames(true);
        }

        let connector = tls_builder.build()
            .map_err(|e| pyo3::exceptions::PyRuntimeError::new_err(format!("Failed to build TLS connector: {}", e)))?;
        
        // ureq 2.x expects the TLS connector to be wrapped in Arc for the trait implementation to match.
        builder = builder.tls_connector(Arc::new(connector));

        if let Some(p) = proxy {
            // Only configure ureq proxy for HTTP/HTTPS proxies (not SOCKS)
            if p.url.starts_with("http://") || p.url.starts_with("https://") {
                let proxy_obj = ureq::Proxy::new(&p.url)
                    .map_err(|e| pyo3::exceptions::PyValueError::new_err(format!("Invalid proxy {}: {}", p.url, e)))?;
                builder = builder.proxy(proxy_obj);
            }
        }

        let agent = builder.build();
        let proxy_url_str = proxy.map(|p| p.url.clone());
        Ok(HTTPTransport { agent, proxy_url: proxy_url_str })
    }

    pub fn send_request(&self, py: Python<'_>, request: &Request) -> PyResult<Response> {
        let url_str = request.url.to_string();
        // Apply timeout
        let mut timeout_duration = None;
        if let Ok(ext) = request.extensions.bind(py).cast::<PyDict>() {
                if let Some(timeout_obj) = ext.get_item("timeout").ok().flatten() {
                    if let Ok(t) = timeout_obj.extract::<crate::config::Timeout>() {
                         if let Some(read) = t.read {
                             timeout_duration = Some(std::time::Duration::from_secs_f64(read));
                         }
                    }
                }
        }

        // Take ownership of body bytes to avoid clone
        let body_bytes = request.content_body.clone();
        let agent = self.agent.clone();
        let method = request.method.clone();
        let stream_response = request.stream_response;
        
        // Pre-collect headers as owned data for the detached closure
        let header_list = request.headers.bind(py).borrow().get_multi_items();

        enum ResponseBody {
            Bytes(Vec<u8>),
            Stream(Box<dyn Read + Send>),
        }

        // Release GIL for blocking I/O
        let (status_code, resp_headers, body_enum) = py.detach(move || {
             let run = || -> Result<_, String> {
                 let mut req = agent.request(&method, &url_str);
                 for (key, value) in &header_list { req = req.set(key, value); }
                 if let Some(d) = timeout_duration { req = req.timeout(d); }
                 
                 let response_result = if let Some(ref b) = body_bytes { req.send_bytes(b) } else { req.call() };
                 let response = match response_result {
                     Ok(resp) => resp,
                     Err(ureq::Error::Status(_code, resp)) => resp,
                     Err(e) => return Err(format!("Request failed: {}", e)),
                 };
                 
                 let status = response.status();
                 // Pre-allocate headers vec with reasonable capacity
                 let header_names = response.headers_names();
                 let mut headers_vec = Vec::with_capacity(header_names.len());
                 for name in header_names {
                     if let Some(value) = response.header(&name) {
                         headers_vec.push((name, value.to_string()));
                     }
                 }
                 
                 if stream_response {
                     let reader = response.into_reader();
                     // ureq reader is Box<dyn Read + Send + Sync>, so it satisfies Send
                     Ok((status, headers_vec, ResponseBody::Stream(reader)))
                 } else {
                     // Pre-allocate body buffer using content-length hint
                     let content_len = response.header("content-length")
                         .and_then(|v| v.parse::<usize>().ok())
                         .unwrap_or(4096);
                     let mut reader = response.into_reader();
                     let mut bytes = Vec::with_capacity(content_len);
                     std::io::Read::read_to_end(&mut reader, &mut bytes).map_err(|e| format!("Failed to read body: {}", e))?;
                     Ok((status, headers_vec, ResponseBody::Bytes(bytes)))
                 }
             };
             
             run()
        }).map_err(|e: String| {
            if e.contains("Unknown Scheme") {
                crate::exceptions::UnsupportedProtocol::new_err(e)
            } else if e.contains("Connect") || e.contains("Connection") || e.contains("dns error") || e.contains("resolve") {
                crate::exceptions::ConnectError::new_err(e)
            } else if e.contains("timed out") || e.contains("Timeout") || e.contains("timeout") {
                crate::exceptions::ReadTimeout::new_err(e)
            } else {
                crate::exceptions::NetworkError::new_err(e)
            }
        })?;

        // Build Response (requires GIL)
        let mut hdrs = Headers::create(None, "utf-8")?;
        for (k, v) in &resp_headers {
            hdrs.set_header(k, v);
        }

        let ext = PyDict::new(py);
        
        let (content_bytes, stream_obj) = match body_enum {
            ResponseBody::Bytes(b) => (Some(b), None),
            ResponseBody::Stream(r) => {
                let blocking_iter = crate::types::BlockingBytesIterator::new(r);
                let py_iter = Py::new(py, blocking_iter)?.into_any();
                (None, Some(py_iter))
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
            default_encoding: "utf-8".into_pyobject(py).unwrap().into_any().unbind(),
            default_encoding_override: None,
            elapsed: None,
            is_closed_flag: false,
            is_stream_consumed: false,
            was_streaming: has_stream,
            text_accessed: std::sync::atomic::AtomicBool::new(false),
            num_bytes_downloaded_counter: std::sync::Arc::new(std::sync::atomic::AtomicUsize::new(0)),
        })
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

    fn close(&self) -> PyResult<()> { Ok(()) }

    #[getter]
    fn _pool(&self, py: Python<'_>) -> PyResult<Py<PyAny>> {
        if let Some(ref proxy_url) = self.proxy_url {
            if proxy_url.starts_with("socks5://") || proxy_url.starts_with("socks5h://") {
                let httpcore = py.import("httpcore")?;
                let pool = httpcore.getattr("SOCKSProxy")?.call1((proxy_url.as_str(),))?;
                Ok(pool.unbind())
            } else {
                let httpcore = py.import("httpcore")?;
                let pool = httpcore.getattr("HTTPProxy")?.call1((proxy_url.as_str(),))?;
                Ok(pool.unbind())
            }
        } else {
            Ok(py.None())
        }
    }

    fn __enter__(slf: PyRef<'_, Self>) -> PyRef<'_, Self> { slf }
    fn __exit__(
        &self,
        _exc_type: Option<&Bound<'_, PyAny>>,
        _exc_value: Option<&Bound<'_, PyAny>>,
        _traceback: Option<&Bound<'_, PyAny>>,
    ) -> PyResult<()> { self.close() }
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
                Some(std::sync::Arc::new(tokio::sync::Semaphore::new(max_conn as usize)))
            } else {
                None
            }
        } else {
            None
        };

        if let Some(cert_path) = root_cert_path {
            let mut buf = Vec::new();
            std::fs::File::open(cert_path)
                .map_err(|e| pyo3::exceptions::PyIOError::new_err(format!("Failed to open cert file: {}", e)))?
                .read_to_end(&mut buf)
                .map_err(|e| pyo3::exceptions::PyIOError::new_err(format!("Failed to read cert file: {}", e)))?;
            let cert = reqwest::Certificate::from_pem(&buf)
                .map_err(|e| pyo3::exceptions::PyValueError::new_err(format!("Invalid certificate: {}", e)))?;
            builder = builder.add_root_certificate(cert);
        }

        if let Some(p) = proxy {
            let proxy_conf = reqwest::Proxy::all(&p.url)
                .map_err(|e| pyo3::exceptions::PyValueError::new_err(format!("Invalid proxy: {}", e)))?;
            builder = builder.proxy(proxy_conf);
        }

        let client = builder.build()
            .map_err(|e| pyo3::exceptions::PyRuntimeError::new_err(format!("Failed to create client: {}", e)))?;
        let proxy_url_str = proxy.map(|p| p.url.clone());
        Ok(AsyncHTTPTransport { client, proxy_url: proxy_url_str, pool_semaphore })
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
        let header_pairs = request.headers.bind(py).borrow().get_multi_items();
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
        let timeout_val: Option<std::time::Duration> = [connect_timeout_val, read_timeout_val, write_timeout_val]
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
                            "Connection pool is closed".to_string()
                        ));
                    }
                    Err(_elapsed) => {
                        return Err(crate::exceptions::PoolTimeout::new_err(
                            "Timed out waiting for a connection from the pool".to_string()
                        ));
                    }
                }
            } else {
                None
            };

            let method = reqwest::Method::from_bytes(method_str.as_bytes())
                .map_err(|e| pyo3::exceptions::PyValueError::new_err(format!("Invalid method: {}", e)))?;

            let mut req_builder = client.request(method, &url_str);
            for (key, value) in &header_pairs {
                req_builder = req_builder.header(key.as_str(), value.as_str());
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
                    if e.is_connect() || (connect_timeout_val.is_some() && read_timeout_val.is_none() && write_timeout_val.is_none()) {
                        crate::exceptions::ConnectTimeout::new_err(format!("{}", e))
                    } else if write_timeout_val.is_some() && body.is_some() && read_timeout_val.is_none() {
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
            let resp_headers: Vec<(String, String)> = response.headers().iter()
                .map(|(k, v)| (k.as_str().to_string(), v.to_str().unwrap_or("").to_string()))
                .collect();
            
            enum BodyType {
                Bytes(Vec<u8>),
                Stream(Box<dyn futures::Stream<Item = reqwest::Result<bytes::Bytes>> + Send + Unpin>),
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
                let mut hdrs = Headers::create(None, "utf-8")?;
                for (k, v) in &resp_headers {
                    hdrs.set_header(k, v);
                }
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
                    default_encoding: "utf-8".into_pyobject(py).unwrap().into_any().unbind(),
                    default_encoding_override: None,
                    elapsed: None,
                    is_closed_flag: false,
                    is_stream_consumed: false,
                    was_streaming: has_stream,
            text_accessed: std::sync::atomic::AtomicBool::new(false),
            num_bytes_downloaded_counter: std::sync::Arc::new(std::sync::atomic::AtomicUsize::new(0)),
                })
            })
        })
    }

    fn aclose(&self) -> PyResult<()> { Ok(()) }

    #[getter]
    fn _pool(&self, py: Python<'_>) -> PyResult<Py<PyAny>> {
        if let Some(ref proxy_url) = self.proxy_url {
            if proxy_url.starts_with("socks5://") || proxy_url.starts_with("socks5h://") {
                let httpcore = py.import("httpcore")?;
                let pool = httpcore.getattr("AsyncSOCKSProxy")?.call1((proxy_url.as_str(),))?;
                Ok(pool.unbind())
            } else {
                let httpcore = py.import("httpcore")?;
                let pool = httpcore.getattr("AsyncHTTPProxy")?.call1((proxy_url.as_str(),))?;
                Ok(pool.unbind())
            }
        } else {
            Ok(py.None())
        }
    }

    fn __aenter__<'py>(slf: Bound<'py, Self>, py: Python<'py>) -> PyResult<Bound<'py, PyAny>> {
        let slf_py = slf.unbind();
        pyo3_async_runtimes::tokio::future_into_py(py, async move {
            Ok(slf_py)
        })
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
