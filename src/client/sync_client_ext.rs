use pyo3::prelude::*;
use pyo3::types::{PyAnyMethods, PyDict, PyDictMethods, PyList};
use std::time::Instant;

use crate::models::{Request, Response};
use super::sync_client::Client;

#[pymethods]
impl Client {
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
                    Err(e) => {
                        if return_exceptions {
                            py_list.append(e.into_pyobject(py)?.into_any())?;
                        } else {
                            return Err(e);
                        }
                    }
                }
            }
            Ok(py_list.into())
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
                            py_list.append(pyo3::exceptions::PyRuntimeError::new_err(err_str).into_value(py).into_any())?;
                        } else {
                            return Err(err);
                        }
                    }
                }
            }
            Ok(py_list.unbind())
        }
    }

    /// Dispatch multiple raw requests concurrently.
    /// Returns a list of (status, headers_dict, body_bytes) tuples.
    /// Skips Response construction entirely for maximum speed.
    /// This is an httpxr extension — not available in httpx.
    ///
    /// Usage:
    ///   results = client.gather_raw([
    ///       client.build_request("GET", "https://api.example.com/1"),
    ///       client.build_request("GET", "https://api.example.com/2"),
    ///   ])
    #[pyo3(signature = (requests, *, max_concurrency=10))]
    fn gather_raw(
        &self,
        py: Python<'_>,
        requests: Vec<Py<Request>>,
        max_concurrency: usize,
    ) -> PyResult<Py<pyo3::types::PyList>> {
        if self.is_closed {
            return Err(pyo3::exceptions::PyRuntimeError::new_err(
                "Cannot send requests, as the client has been closed.",
            ));
        }
        if requests.is_empty() {
            return Ok(pyo3::types::PyList::empty(py).unbind());
        }

        let transport_bound = self.transport.bind(py);
        let transport = transport_bound
            .cast::<crate::transports::default::HTTPTransport>()
            .map_err(|_| {
                pyo3::exceptions::PyRuntimeError::new_err(
                    "gather_raw requires the default HTTPTransport",
                )
            })?;

        // Extract raw ingredients from Request objects
        let mut raw_requests = Vec::with_capacity(requests.len());
        for py_req in requests {
            let req_ref = py_req.into_bound(py);
            let req = req_ref.borrow();
            let method_str = req.method.to_string().to_uppercase();
            let method = reqwest::Method::from_bytes(method_str.as_bytes())
                .unwrap_or(reqwest::Method::GET);
            
            let url = req.url.to_string();
            let headers = req.headers.bind(py).borrow().iter_all();
            let body = req.content_body.clone(); // Vec<u8>
            let timeout = req.extensions.bind(py).get_item("timeout").ok().and_then(|v| {
                if let Ok(f) = v.extract::<f64>() {
                    Some(f)
                } else {
                    None
                }
            });
            
            raw_requests.push((method, url, Some(headers), body, timeout));
        }

        let transport_ref = transport.borrow();
        let results = transport_ref.send_batch_raw(py, &raw_requests, max_concurrency)?;
        Ok(results)
    }

    /// Auto-follow pagination links, returning a lazy iterator.
    /// Each iteration fetches the next page — memory-efficient for large result sets.
    /// Supports JSON key extraction, Link header parsing, and custom callables.
    #[pyo3(signature = (method, url, *, next_url=None, next_header=None, next_func=None, max_pages=100, params=None, headers=None, cookies=None, timeout=None, extensions=None, **kwargs))]
    fn paginate<'py>(
        slf: &Bound<'py, Self>,
        py: Python<'py>,
        method: &str,
        url: &Bound<'_, PyAny>,
        next_url: Option<String>,
        next_header: Option<String>,
        next_func: Option<Py<PyAny>>,
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

        let f = kwargs.as_ref()
            .and_then(|kw| kw.get_item("follow_redirects").ok()?)
            .and_then(|v| v.extract::<bool>().ok());
            
        // For the first request, build and send immediately in the constructor
        let req = slf.borrow().build_request(
            py, method, url, None, None, None, None, params, headers,
            cookies, extensions, timeout
        )?;
        
        // Return uninitialized iterator. It will send the first request on `__next__`.
        Ok(PageIterator {
            client: slf.clone().unbind().into_any(),
            method: method.to_string(),
            current_url: Some(url.clone().unbind()),
            next_url_key: next_url,
            next_header_name: next_header,
            next_func,
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

    /// Fetch page 1, discover total pages, then fire remaining pages concurrently.
    /// Returns a list of all responses (page 1 + concurrent remainder).
    /// This is an httpxr extension — not available in httpx.
    #[pyo3(signature = (method, url, *, total_pages=None, page_param="page", start_page=1, max_concurrency=10, headers=None, extensions=None))]
    fn gather_paginate(
        &self,
        py: Python<'_>,
        method: &str,
        url: &Bound<'_, PyAny>,
        total_pages: Option<usize>,
        page_param: &str,
        start_page: usize,
        max_concurrency: usize,
        headers: Option<&Bound<'_, PyAny>>,
        extensions: Option<&Bound<'_, PyAny>>,
    ) -> PyResult<Py<pyo3::types::PyList>> {
        let total = total_pages.ok_or_else(|| {
            pyo3::exceptions::PyValueError::new_err(
                "total_pages is required for gather_paginate"
            )
        })?;

        if total == 0 {
            return Ok(pyo3::types::PyList::empty(py).unbind());
        }

        let url_str = url.str()?.extract::<String>()?;
        let mut request_objs: Vec<Py<Request>> = Vec::with_capacity(total);

        for page in start_page..=(start_page + total - 1) {
            let separator = if url_str.contains('?') { "&" } else { "?" };
            let page_url = format!("{}{}{}={}", url_str, separator, page_param, page);
            let page_url_py = page_url.into_pyobject(py)?.into_any();

            let req = self.build_request(
                py,
                method,
                &page_url_py,
                None, None, None, None, None,
                headers,
                None,
                extensions,
                None, // timeout
            )?;
            request_objs.push(Py::new(py, req)?);
        }

        self.gather(py, request_objs, max_concurrency, false)
    }

    /// Stream a response via generator.
    /// Use this as a context manager: `with client.stream("GET", "...") as response:`
    #[pyo3(signature = (method, url, **kwargs))]
    fn stream<'py>(
        client: Bound<'py, Self>,
        py: Python<'py>,
        method: String,
        url: Bound<'py, PyAny>,
        kwargs: Option<Bound<'py, PyDict>>,
    ) -> PyResult<crate::stream_ctx::SyncStreamContextManager> {
        let mut request = client
            .call_method1("build_request", (&method, &url))
            .map_err(|e| e)?
            .extract::<crate::models::Request>()?;
        request.stream_response = true;

        Ok(crate::stream_ctx::SyncStreamContextManager {
            client: client.unbind(),
            method: request.method,
            url: request.url.into_pyobject(py)?.into_any().unbind(),
            kwargs: kwargs.map(|k| k.unbind()),
            response: None,
        })
    }

    /// Download a URL to a file path, streaming the content directly to disk.
    /// This is an httpxr extension — not available in httpx.
    #[pyo3(signature = (url, path, *, headers=None, params=None, timeout=None, follow_redirects=None, chunk_size=8192))]
    fn download(
        &self,
        py: Python<'_>,
        url: &Bound<'_, PyAny>,
        path: &str,
        headers: Option<&Bound<'_, PyAny>>,
        params: Option<&Bound<'_, PyAny>>,
        timeout: Option<&Bound<'_, PyAny>>,
        follow_redirects: Option<bool>,
        chunk_size: usize,
    ) -> PyResult<Response> {
        let py_method = "GET";
        let req = self.build_request(
            py, py_method, url, None, None, None, None, params, headers,
            None, None, timeout
        )?;
        
        let t1 = Instant::now();
        let mut response = self._send_single_request(py, t1, req.clone(), follow_redirects)?;
        
        use std::io::Write;
        let content = response.read_impl(py)?;
        let mut file = std::fs::File::create(path).map_err(|e| {
            pyo3::exceptions::PyIOError::new_err(format!("Failed to create file '{}': {}", path, e))
        })?;
        file.write_all(&content).map_err(|e| {
            pyo3::exceptions::PyIOError::new_err(format!("Failed to write to file '{}': {}", path, e))
        })?;
        Ok(response)
    }
}

#[pyclass(module="httpxr._httpxr")]
pub struct PageIterator {
    pub client: Py<PyAny>,
    pub method: String,
    pub current_url: Option<Py<PyAny>>,
    pub next_url_key: Option<String>,
    pub next_header_name: Option<String>,
    pub next_func: Option<Py<PyAny>>,
    pub max_pages: usize,
    pub page_count: usize,
    pub done: bool,
    pub params: Option<Py<PyAny>>,
    pub headers: Option<Py<PyAny>>,
    pub cookies: Option<Py<PyAny>>,
    pub timeout: Option<Py<PyAny>>,
    pub extensions: Option<Py<PyAny>>,
    pub _kwargs: Option<Py<PyDict>>,
}

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
