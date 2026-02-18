use pyo3::prelude::*;
use pyo3::types::{PyDict};

use crate::models::{Cookies, Headers, Request, Response};
use crate::urls::URL;

use super::common::*;
use super::async_client::AsyncClient;

// -- Macros for async HTTP convenience methods --
// Each macro generates a separate #[pymethods] impl block because PyO3's proc macro
// does not support macro invocations inside #[pymethods] blocks.

macro_rules! async_no_body_method {
    ($name:ident, $method:expr) => {
        #[pymethods]
        impl AsyncClient {
            #[pyo3(signature = (url, *, params=None, headers=None, cookies=None, follow_redirects=None, timeout=None, extensions=None, **kwargs))]
            fn $name<'py>(
                &self, py: Python<'py>, url: &Bound<'_, PyAny>,
                params: Option<&Bound<'_, PyAny>>, headers: Option<&Bound<'_, PyAny>>,
                cookies: Option<&Bound<'_, PyAny>>, follow_redirects: Option<bool>,
                timeout: Option<&Bound<'_, PyAny>>, extensions: Option<&Bound<'_, PyAny>>,
                kwargs: Option<&Bound<'_, PyDict>>,
            ) -> PyResult<Bound<'py, PyAny>> {
                let timeout_kw = extract_from_kwargs(kwargs, "timeout");
                let t = timeout.or_else(|| timeout_kw.as_ref());
                let f = follow_redirects.or_else(|| {
                    extract_from_kwargs(kwargs, "follow_redirects").and_then(|v| v.extract::<bool>().ok())
                });
                self.request(py, $method, url, None, None, None, None, params, headers, cookies, f, t, extensions, false, kwargs)
            }
        }
    };
}

macro_rules! async_body_method {
    ($name:ident, $method:expr) => {
        #[pymethods]
        impl AsyncClient {
            #[pyo3(signature = (url, *, content=None, data=None, files=None, json=None, params=None, headers=None, cookies=None, follow_redirects=None, timeout=None, extensions=None, **kwargs))]
            fn $name<'py>(
                &self, py: Python<'py>, url: &Bound<'_, PyAny>,
                content: Option<&Bound<'_, PyAny>>, data: Option<&Bound<'_, PyAny>>,
                files: Option<&Bound<'_, PyAny>>, json: Option<&Bound<'_, PyAny>>,
                params: Option<&Bound<'_, PyAny>>, headers: Option<&Bound<'_, PyAny>>,
                cookies: Option<&Bound<'_, PyAny>>, follow_redirects: Option<bool>,
                timeout: Option<&Bound<'_, PyAny>>, extensions: Option<&Bound<'_, PyAny>>,
                kwargs: Option<&Bound<'_, PyDict>>,
            ) -> PyResult<Bound<'py, PyAny>> {
                let timeout_kw = extract_from_kwargs(kwargs, "timeout");
                let t = timeout.or_else(|| timeout_kw.as_ref());
                let f = follow_redirects.or_else(|| {
                    extract_from_kwargs(kwargs, "follow_redirects").and_then(|v| v.extract::<bool>().ok())
                });
                self.request(py, $method, url, content, data, files, json, params, headers, cookies, f, t, extensions, false, kwargs)
            }
        }
    };
}

async_no_body_method!(head, "HEAD");
async_no_body_method!(options, "OPTIONS");
async_no_body_method!(delete, "DELETE");
async_body_method!(post, "POST");
async_body_method!(put, "PUT");
async_body_method!(patch, "PATCH");

#[pymethods]
impl AsyncClient {

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

        let has_batch = transport
            .bind(py)
            .hasattr("handle_async_requests_batch")
            .unwrap_or(false);

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
            let t_bound = transport.bind(py);
            let req_list = pyo3::types::PyList::new(py, &requests)?;
            return t_bound.call_method1(
                "handle_async_requests_batch",
                (req_list, max_concurrency),
            ).map(|b| b.clone());
        }

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
            let gather_future = Python::attach(|py| {
                pyo3_async_runtimes::tokio::into_future(gather_coro.bind(py).clone())
            })?;

            let results_obj = gather_future.await?;

            Python::attach(|py| {
                let results_bound = results_obj.bind(py);
                let py_list = pyo3::types::PyList::empty(py);

                if let Ok(result_list) = results_bound.cast::<pyo3::types::PyList>() {
                    for (i, item) in result_list.iter().enumerate() {
                        if let Ok(mut resp) = item.extract::<Response>() {
                            let req_bound = requests[i].bind(py);
                            let req_ref = req_bound.borrow();
                            resp.request = Some(req_ref.clone());
                            py_list.append(Py::new(py, resp)?)?;
                        } else {
                            py_list.append(&item)?;
                        }
                    }
                }
                Ok(py_list.unbind())
            })
        })
    }

    /// Dispatch multiple raw requests concurrently.
    /// Returns a list of (status, headers_dict, body_bytes) tuples.
    /// Skips Response construction entirely for maximum speed.
    /// This is an httpxr extension — not available in httpx.
    #[pyo3(signature = (requests, *, max_concurrency=10))]
    fn gather_raw<'py>(
        &self,
        py: Python<'py>,
        requests: Vec<Py<Request>>,
        max_concurrency: usize,
    ) -> PyResult<Bound<'py, PyAny>> {
        if self.is_closed {
            return Err(pyo3::exceptions::PyRuntimeError::new_err(
                "Cannot send requests, as the client has been closed.",
            ));
        }

        let transport = self.transport.clone_ref(py);

        // Extract request data under GIL
        let mut prepared: Vec<(String, String, Vec<(Vec<u8>, Vec<u8>)>, Option<Vec<u8>>, Option<std::time::Duration>)> = Vec::with_capacity(requests.len());
        for req_py in &requests {
            let req = req_py.bind(py).borrow();
            let url_str = req.url.to_string();
            let method_str = req.method.clone();
            let raw_headers = req.headers.bind(py).borrow().get_raw_items_owned();
            let body = req.content_body.clone();

            let (connect_t, read_t, write_t, _pool_t) =
                crate::transports::helpers::extract_timeout_from_extensions(py, &req.extensions);
            let timeout_val = crate::transports::helpers::compute_effective_timeout(connect_t, read_t, write_t);

            prepared.push((url_str, method_str, raw_headers, body, timeout_val));
        }

        pyo3_async_runtimes::tokio::future_into_py(py, async move {
            // Get client from transport
            let client = Python::attach(|py| -> PyResult<reqwest::Client> {
                let t_bound = transport.bind(py);
                let async_transport = t_bound
                    .cast::<crate::transports::default::AsyncHTTPTransport>()
                    .map_err(|_| {
                        pyo3::exceptions::PyRuntimeError::new_err(
                            "gather_raw requires the default AsyncHTTPTransport",
                        )
                    })?;
                Ok(async_transport.borrow().get_client())
            })?;

            let semaphore = std::sync::Arc::new(tokio::sync::Semaphore::new(max_concurrency));
            let mut handles = Vec::with_capacity(prepared.len());

            for (url_str, method_str, raw_headers, body, timeout_val) in prepared {
                let client = client.clone();
                let sem = semaphore.clone();
                handles.push(tokio::spawn(async move {
                    let _permit = sem.acquire().await.unwrap();

                    let method = crate::transports::helpers::parse_method(method_str.as_str())
                        .map_err(|e| format!("{}", e))?;

                    let mut req_builder = client.request(method, &url_str);
                    for (key, value) in &raw_headers {
                        req_builder = req_builder.header(key.as_slice(), value.as_slice());
                    }
                    if let Some(b) = body {
                        req_builder = req_builder.body(b);
                    }
                    if let Some(t) = timeout_val {
                        req_builder = req_builder.timeout(t);
                    }

                    let response = req_builder.send().await.map_err(|e| format!("{}", e))?;
                    let status = response.status().as_u16();
                    let owned_hdrs: Vec<(String, Vec<u8>)> = response
                        .headers()
                        .iter()
                        .map(|(k, v)| (k.as_str().to_owned(), v.as_bytes().to_vec()))
                        .collect();
                    let bytes = response.bytes().await.map_err(|e| format!("{}", e))?;
                    Ok::<_, String>((status, owned_hdrs, Vec::from(bytes)))
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
                crate::transports::helpers::raw_results_to_pylist(py, raw_results)
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
            let coro = Python::attach(|py| {
                let t = transport.bind(py);
                let coro = t.call_method0("__aenter__")?;
                pyo3_async_runtimes::tokio::into_future(coro)
            })?;
            coro.await?;

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

// ---------------------------------------------------------------------------
// Paginate convenience methods (paginate_get, paginate_post)
// ---------------------------------------------------------------------------

macro_rules! async_paginate_method {
    ($name:ident, $method:expr) => {
        #[pymethods]
        impl AsyncClient {
            #[pyo3(signature = (url, *, next_url=None, next_header=None, next_func=None, max_pages=100, params=None, headers=None, cookies=None, timeout=None, extensions=None, **kwargs))]
            #[allow(clippy::too_many_arguments)]
            fn $name<'py>(
                slf: &Bound<'py, Self>,
                py: Python<'py>,
                url: &Bound<'py, PyAny>,
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
                let kwargs_dict = PyDict::new(py);
                if let Some(v) = next_url    { kwargs_dict.set_item("next_url", v)?; }
                if let Some(v) = next_header { kwargs_dict.set_item("next_header", v)?; }
                if let Some(v) = next_func   { kwargs_dict.set_item("next_func", v)?; }
                kwargs_dict.set_item("max_pages", max_pages)?;
                if let Some(v) = params      { kwargs_dict.set_item("params", v)?; }
                if let Some(v) = headers     { kwargs_dict.set_item("headers", v)?; }
                if let Some(v) = cookies     { kwargs_dict.set_item("cookies", v)?; }
                if let Some(v) = timeout     { kwargs_dict.set_item("timeout", v)?; }
                if let Some(v) = extensions  { kwargs_dict.set_item("extensions", v)?; }
                if let Some(extra) = kwargs {
                    kwargs_dict.update(extra.bind(py).as_mapping())?;
                }

                slf.call_method("paginate", ($method, url), Some(&kwargs_dict))
            }
        }
    };
}

async_paginate_method!(paginate_get, "GET");
async_paginate_method!(paginate_post, "POST");

#[allow(dead_code)]
struct ReleaseGuard {
    pub stream: Option<Py<PyAny>>,
}

impl Drop for ReleaseGuard {
    fn drop(&mut self) {
        if let Some(_stream) = self.stream.take() {
        }
    }
}
