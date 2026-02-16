use pyo3::prelude::*;
use pyo3::types::{PyAnyMethods, PyDictMethods, PyDict, PyBytes};
use std::time::Instant;

use crate::config::{Limits, Timeout};
use crate::models::{Headers, Request, Response, Cookies};
use crate::urls::URL;
use crate::stream_ctx::StreamContextManager;

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
            hdrs.set_header("user-agent", &format!("python-httpr/{}", env!("CARGO_PKG_VERSION")));
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

        Ok(AsyncClient {
            base_url: base,
            auth: auth.map(|a| {
                if let AuthArg::Custom(p) = a {
                    // Validate custom auth
                    if let Err(e) = validate_auth_type(py, p.bind(py)) {
                        return Err(e);
                    }
                    Ok(Some(p))
                } else {
                    Ok(None)
                }
            }).transpose()?.flatten(),
            params,
            default_headers: hdrs_py,
            cookies: ckies,
            max_redirects: max_redirects.unwrap_or(DEFAULT_MAX_REDIRECTS),
            follow_redirects,
            transport: transport_obj,
            mounts: mounts_dict,
            timeout: if let Some(t) = timeout { t.extract::<Timeout>().ok() } else { None },
            is_closed: false,
            trust_env,
            event_hooks: event_hooks.map(|e| e.clone().unbind()),
            default_encoding: default_encoding.map(|de| de.clone().unbind()),
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
        if stream { req.stream_response = true; }

        // Apply client-level timeout to request if it doesn't already have one
        if let Some(ref t) = self.timeout {
            let ext = req.extensions.bind(py);
            if let Ok(d) = ext.cast::<PyDict>() {
                if !d.contains("timeout")? {
                    d.set_item("timeout", Py::new(py, t.clone())?)?;
                }
            }
        }

        let transport = self.transport.clone_ref(py);
        let mounts = self.mounts.clone_ref(py);
        let max_redirects = self.max_redirects;
        let follow_redirects = follow_redirects.unwrap_or(self.follow_redirects);
        let event_hooks = self.event_hooks.as_ref().map(|h| h.clone_ref(py));
        let is_streaming = stream;

        // Capture the running asyncio event loop for non-streaming requests.
        // Only needed for the eager-read Task that handles BaseException properly.
        // Cannot capture unconditionally because send() may be called from a
        // tokio-managed thread (e.g., StreamContextManager.__aenter__) where
        // there is no running event loop.
        let event_loop: Option<Py<PyAny>> = if !is_streaming {
            Some(py.import("asyncio")?
                .call_method0("get_running_loop")?
                .unbind())
        } else {
            None
        };

        pyo3_async_runtimes::tokio::future_into_py(py, async move {
            let start = Instant::now();

            // If we have an auth flow, drive it
            if let Some(flow) = auth {
                return Self::_send_with_auth_flow(
                    start, flow, req, transport, mounts, max_redirects,
                    follow_redirects, event_hooks, None,
                ).await;
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
                            let is_awaitable = py.import("inspect")?.call_method1("isawaitable", (&ret,))?.extract::<bool>()?;
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
                    let transport_to_use = select_transport(mounts.bind(py).clone(), transport.clone_ref(py), &current_req.url)?;
                    let t_bound = transport_to_use.bind(py);
                    let req_py = Py::new(py, current_req.clone())?;
                    t_bound.call_method1("handle_async_request", (req_py,)).map(|b| b.unbind())
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

                    // Apply default_encoding (not stored in AsyncClient currently)

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
                            let is_awaitable = py.import("inspect")?.call_method1("isawaitable", (&ret,))?.extract::<bool>()?;
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
                        return Err(crate::exceptions::TooManyRedirects::new_err(format!("Exceeded maximum number of redirects ({})", max_redirects)));
                    }

                    let location = response.bind(py).borrow().headers.bind(py).borrow().get_first_value("location").ok_or_else(|| pyo3::exceptions::PyValueError::new_err("Redirect without Location header"))?;
                    let mut redirect_url = current_req.url.join_relative(&location)
                        .map_err(|e| crate::exceptions::RemoteProtocolError::new_err(e.to_string()))?;

                    // Fragment preservation
                    if redirect_url.parsed.fragment.is_none() {
                        redirect_url.parsed.fragment = current_req.url.parsed.fragment.clone();
                    }

                    let status_code = response.bind(py).borrow().status_code;
                    let redirect_method = if status_code == 303 { "GET".to_string() } else { current_req.method.clone() };
                    let redirect_body = if redirect_method == "GET" { None } else { current_req.content_body.clone() };

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
                            // Create an asyncio Task to run aread() and bridge the result
                            // via an asyncio.Future + callback. This avoids pyo3_async_runtimes
                            // from polling the coroutine directly, which doesn't properly
                            // handle BaseException (KeyboardInterrupt, SystemExit).
                            //
                            // Flow:
                            // 1. Create asyncio.Future on the event loop
                            // 2. Create asyncio.Task for the aread() coroutine
                            // 3. Add done-callback on Task that resolves the Future
                            // 4. Await the Future via into_future (no KeyboardInterrupt here)
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
        # Do NOT re-raise — store exception and return False.
        # Re-raising BaseException (especially KeyboardInterrupt) inside
        # an asyncio Task causes it to escape Task.__step and crash the
        # event loop.
        resp._mark_closed()
        exc_holder.append(_exc)
        # Cleanup: close iterator and stream
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
                                let fut = pyo3_async_runtimes::tokio::into_future(result_fut.clone())?;
                                Ok::<_, PyErr>((fut, exc_holder))
                            })?;
                            // Await the result future — it resolves with True/False,
                            // never with an exception (to avoid KeyboardInterrupt escaping)
                            let success_obj = result_future.0.await?;
                            let success = Python::attach(|py| {
                                success_obj.bind(py).extract::<bool>()
                            })?;
                            if !success {
                                // Retrieve the stored exception from exc_holder
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
             let coro = Python::attach(|py| flow.bind(py).call_method0("__anext__").map(|b| b.unbind()))?;
             let fut = Python::attach(|py| pyo3_async_runtimes::tokio::into_future(coro.bind(py).clone()))?;
             match fut.await {
                 Ok(v) => Python::attach(|py| Ok(Some(v.bind(py).extract::<Request>()?))),
                 Err(e) => {
                     let is_stop = Python::attach(|py| e.is_instance_of::<pyo3::exceptions::PyStopAsyncIteration>(py));
                     if is_stop { Ok(None) } else { Err(e) }
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
                    start, original_req, transport, mounts, default_encoding,
                ).await;
            }
        };

        loop {
            // Send the request
            let response_coro = Python::attach(|py| {
                let transport_to_use = select_transport(mounts.bind(py).clone(), transport.clone_ref(py), &current_req.url)?;
                let t_bound = transport_to_use.bind(py);
                let req_py = Py::new(py, current_req.clone())?;
                t_bound.call_method1("handle_async_request", (req_py,)).map(|b| b.unbind())
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
                    if current_encoding == "utf-8" { // Only override if default
                        let new_encoding: String = de.bind(py).extract()?;
                        resp.default_encoding = pyo3::types::PyString::new(py, &new_encoding).into_any().unbind();
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
                flow_bound.getattr("requires_response_body").ok()
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
                 let coro = Python::attach(|py| flow.bind(py).call_method1("asend", (response_obj.bind(py),)).map(|b| b.unbind()))?;
                 let fut = Python::attach(|py| pyo3_async_runtimes::tokio::into_future(coro.bind(py).clone()))?;
                 match fut.await {
                     Ok(v) => Python::attach(|py| Ok(Some(v.bind(py).extract::<Request>()?))),
                     Err(e) => {
                         let is_stop = Python::attach(|py| e.is_instance_of::<pyo3::exceptions::PyStopAsyncIteration>(py));
                         if is_stop { Ok(None) } else { Err(e) }
                     }
                 }?
            } else {
                 Python::attach(|py| {
                    match flow.bind(py).call_method1("send", (response_obj.bind(py),)) {
                        Ok(r) => Ok(Some(r.extract::<Request>()?)),
                        Err(e) if e.is_instance_of::<pyo3::exceptions::PyStopIteration>(py) => Ok(None),
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
                return Python::attach(|py| {
                    Py::new(py, response)
                });
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
            let transport_to_use = select_transport(mounts.bind(py).clone(), transport.clone_ref(py), &request.url)?;
            let t_bound = transport_to_use.bind(py);
            let req_py = Py::new(py, request.clone())?;
            t_bound.call_method1("handle_async_request", (req_py,)).map(|b| b.unbind())
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
                let method = &resp.request.as_ref().map(|r| r.method.clone()).unwrap_or_default();
                let url = resp.request.as_ref().map(|r| r.url.to_string()).unwrap_or_default();
                let http_version = {
                    let ext = resp.extensions.bind(py);
                    if let Ok(d) = ext.cast::<PyDict>() {
                        d.get_item("http_version").ok().flatten()
                            .and_then(|v| v.extract::<Vec<u8>>().ok())
                            .map(|b| String::from_utf8_lossy(&b).to_string())
                            .unwrap_or_else(|| "HTTP/1.1".to_string())
                    } else {
                        "HTTP/1.1".to_string()
                    }
                };
                let status = resp.status_code;
                let reason = resp.reason_phrase();
                log::info!(target: "httpr", "HTTP Request: {} {} \"{} {} {}\"", method, url, http_version, status, reason);
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
        let mut request = client.call_method1("build_request", (method, url)).map_err(|e| e)?.extract::<Request>()?;
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
            return Err(pyo3::exceptions::PyRuntimeError::new_err("The client is closed"));
        }
        let _files = files;
        let method_str = method.to_uppercase();
        let url_str = url.str()?.extract::<String>()?;

        // Build URL
        let mut target_url = match resolve_url(&self.base_url, &url_str) {
            Ok(u) => u,
            Err(e) => {
                let msg = e.to_string();
                if msg.contains("RelativeUrlWithoutBase") || msg.contains("EmptyHost") || msg.contains("empty host") {
                    return Err(crate::exceptions::LocalProtocolError::new_err(
                        format!("Invalid URL '{}'", url_str)
                    ));
                }
                return Err(crate::exceptions::UnsupportedProtocol::new_err(
                    format!("Request URL has an unsupported protocol '{}': {}", url_str, msg)
                ));
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
            return Err(crate::exceptions::UnsupportedProtocol::new_err(
                format!("Request URL is missing a scheme for URL '{}'", url_str)
            ));
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
                return Err(crate::exceptions::UnsupportedProtocol::new_err(
                    format!("Request URL has an unsupported protocol '{}': for URL '{}'", scheme, url_str)
                ));
            }
        }
        if host.is_empty() && ["http", "https"].contains(&&*scheme) {
            return Err(crate::exceptions::LocalProtocolError::new_err(
                format!("Invalid URL '{}'", url_str)
            ));
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
                        "The content argument must be an async iterator."
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
                } else { None }
            } else { None };

            let multipart = crate::multipart::MultipartStream::new(py, data, Some(f), boundary.as_deref())?;
            if !merged_headers.contains_header("content-type") {
                merged_headers.set_header("content-type", &multipart.content_type());
            }
            (Some(multipart.get_content()), None)
        } else if let Some(d) = data {
            let urllib = py.import("urllib.parse")?;
            let s: String = urllib.call_method1("urlencode", (d, true))?.extract()?;
            merged_headers.set_header("content-type", "application/x-www-form-urlencoded");
            (Some(s.into_bytes()), None)
        } else { (None, None) };
 
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
            if t_arg.is_none() { None }
            else { Some(crate::config::Timeout::new(py, Some(t_arg), None, None, None, None)?) }
        } else {
            self.timeout.clone()
        };

        if let Some(t) = t_val {
            let _ = request.extensions.bind(py).set_item("timeout", Py::new(py, t)?);
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
        let auth_result = apply_auth_async(py, auth_val.as_ref(), self.auth.as_ref(), &mut request, auth_explicitly_none)?; 

        // Get optional auth flow for passing to send
        let auth_flow = match auth_result {
            AuthResult::Flow(flow) => Some(flow),
            _ => None,
        };

        let req_py = Py::new(py, request)?;
        self.send(py, req_py.into_bound(py).into_any(), stream, auth_flow, follow_redirects)
    }





    #[pyo3(signature = (url, *, params=None, headers=None, cookies=None, follow_redirects=None, timeout=None, extensions=None, **kwargs))]
    fn get<'py>(
        &self, py: Python<'py>, url: &Bound<'_, PyAny>,
        params: Option<&Bound<'_, PyAny>>, headers: Option<&Bound<'_, PyAny>>,
        cookies: Option<&Bound<'_, PyAny>>,
        follow_redirects: Option<bool>, timeout: Option<&Bound<'_, PyAny>>,
        extensions: Option<&Bound<'_, PyAny>>, kwargs: Option<&Bound<'_, PyDict>>,
    ) -> PyResult<Bound<'py, PyAny>> {
        let timeout_kw = extract_from_kwargs(kwargs, "timeout");
        let t = if let Some(t) = timeout { Some(t) } else { timeout_kw.as_ref() };
        self.request(py, "GET", url, None, None, None, None, params, headers, cookies, follow_redirects, t, extensions, false, kwargs)
    }

    #[pyo3(signature = (url, *, params=None, headers=None, cookies=None, follow_redirects=None, timeout=None, extensions=None, **kwargs))]
    fn head<'py>(
        &self, py: Python<'py>, url: &Bound<'_, PyAny>,
        params: Option<&Bound<'_, PyAny>>, headers: Option<&Bound<'_, PyAny>>,
        cookies: Option<&Bound<'_, PyAny>>,
        follow_redirects: Option<bool>, timeout: Option<&Bound<'_, PyAny>>,
        extensions: Option<&Bound<'_, PyAny>>, kwargs: Option<&Bound<'_, PyDict>>,
    ) -> PyResult<Bound<'py, PyAny>> {
        let timeout_kw = extract_from_kwargs(kwargs, "timeout");
        let t = if let Some(t) = timeout { Some(t) } else { timeout_kw.as_ref() };
        let f = if let Some(f) = follow_redirects { Some(f) } else { extract_from_kwargs(kwargs, "follow_redirects").and_then(|v| v.extract::<bool>().ok()) };
        self.request(py, "HEAD", url, None, None, None, None, params, headers, cookies, f, t, extensions, false, kwargs)
    }

    #[pyo3(signature = (url, *, params=None, headers=None, cookies=None, follow_redirects=None, timeout=None, extensions=None, **kwargs))]
    fn options<'py>(
        &self, py: Python<'py>, url: &Bound<'_, PyAny>,
        params: Option<&Bound<'_, PyAny>>, headers: Option<&Bound<'_, PyAny>>,
        cookies: Option<&Bound<'_, PyAny>>,
        follow_redirects: Option<bool>, timeout: Option<&Bound<'_, PyAny>>,
        extensions: Option<&Bound<'_, PyAny>>, kwargs: Option<&Bound<'_, PyDict>>,
    ) -> PyResult<Bound<'py, PyAny>> {
        let timeout_kw = extract_from_kwargs(kwargs, "timeout");
        let t = if let Some(t) = timeout { Some(t) } else { timeout_kw.as_ref() };
        self.request(py, "OPTIONS", url, None, None, None, None, params, headers, cookies, follow_redirects, t, extensions, false, kwargs)
    }

    #[pyo3(signature = (url, *, content=None, data=None, files=None, json=None, params=None, headers=None, cookies=None, follow_redirects=None, timeout=None, extensions=None, **kwargs))]
    fn post<'py>(
        &self, py: Python<'py>, url: &Bound<'_, PyAny>,
        content: Option<&Bound<'_, PyAny>>, data: Option<&Bound<'_, PyAny>>,
        files: Option<&Bound<'_, PyAny>>, json: Option<&Bound<'_, PyAny>>,
        params: Option<&Bound<'_, PyAny>>, headers: Option<&Bound<'_, PyAny>>,
        cookies: Option<&Bound<'_, PyAny>>,
        follow_redirects: Option<bool>, timeout: Option<&Bound<'_, PyAny>>,
        extensions: Option<&Bound<'_, PyAny>>, kwargs: Option<&Bound<'_, PyDict>>,
    ) -> PyResult<Bound<'py, PyAny>> {
        let timeout_kw = extract_from_kwargs(kwargs, "timeout");
        let t = if let Some(t) = timeout { Some(t) } else { timeout_kw.as_ref() };
        let f = if let Some(f) = follow_redirects { Some(f) } else { extract_from_kwargs(kwargs, "follow_redirects").and_then(|v| v.extract::<bool>().ok()) };
        self.request(py, "POST", url, content, data, files, json, params, headers, cookies, f, t, extensions, false, kwargs)
    }

    #[pyo3(signature = (url, *, content=None, data=None, files=None, json=None, params=None, headers=None, cookies=None, follow_redirects=None, timeout=None, extensions=None, **kwargs))]
    fn put<'py>(
        &self, py: Python<'py>, url: &Bound<'_, PyAny>,
        content: Option<&Bound<'_, PyAny>>, data: Option<&Bound<'_, PyAny>>,
        files: Option<&Bound<'_, PyAny>>, json: Option<&Bound<'_, PyAny>>,
        params: Option<&Bound<'_, PyAny>>, headers: Option<&Bound<'_, PyAny>>,
        cookies: Option<&Bound<'_, PyAny>>,
        follow_redirects: Option<bool>, timeout: Option<&Bound<'_, PyAny>>,
        extensions: Option<&Bound<'_, PyAny>>, kwargs: Option<&Bound<'_, PyDict>>,
    ) -> PyResult<Bound<'py, PyAny>> {
        let timeout_kw = extract_from_kwargs(kwargs, "timeout");
        let t = if let Some(t) = timeout { Some(t) } else { timeout_kw.as_ref() };
        self.request(py, "PUT", url, content, data, files, json, params, headers, cookies, follow_redirects, t, extensions, false, kwargs)
    }

    #[pyo3(signature = (url, *, content=None, data=None, files=None, json=None, params=None, headers=None, cookies=None, follow_redirects=None, timeout=None, extensions=None, **kwargs))]
    fn patch<'py>(
        &self, py: Python<'py>, url: &Bound<'_, PyAny>,
        content: Option<&Bound<'_, PyAny>>, data: Option<&Bound<'_, PyAny>>,
        files: Option<&Bound<'_, PyAny>>, json: Option<&Bound<'_, PyAny>>,
        params: Option<&Bound<'_, PyAny>>, headers: Option<&Bound<'_, PyAny>>,
        cookies: Option<&Bound<'_, PyAny>>,
        follow_redirects: Option<bool>, timeout: Option<&Bound<'_, PyAny>>,
        extensions: Option<&Bound<'_, PyAny>>, kwargs: Option<&Bound<'_, PyDict>>,
    ) -> PyResult<Bound<'py, PyAny>> {
        let timeout_kw = extract_from_kwargs(kwargs, "timeout");
        let t = if let Some(t) = timeout { Some(t) } else { timeout_kw.as_ref() };
        self.request(py, "PATCH", url, content, data, files, json, params, headers, cookies, follow_redirects, t, extensions, false, kwargs)
    }

    #[pyo3(signature = (url, *, params=None, headers=None, cookies=None, follow_redirects=None, timeout=None, extensions=None, **kwargs))]
    fn delete<'py>(
        &self, py: Python<'py>, url: &Bound<'_, PyAny>,
        params: Option<&Bound<'_, PyAny>>, headers: Option<&Bound<'_, PyAny>>,
        cookies: Option<&Bound<'_, PyAny>>,
        follow_redirects: Option<bool>, timeout: Option<&Bound<'_, PyAny>>,
        extensions: Option<&Bound<'_, PyAny>>, kwargs: Option<&Bound<'_, PyDict>>,
    ) -> PyResult<Bound<'py, PyAny>> {
        let timeout_kw = extract_from_kwargs(kwargs, "timeout");
        let t = if let Some(t) = timeout { Some(t) } else { timeout_kw.as_ref() };
        let f = if let Some(f) = follow_redirects { Some(f) } else { extract_from_kwargs(kwargs, "follow_redirects").and_then(|v| v.extract::<bool>().ok()) };
        self.request(py, "DELETE", url, None, None, None, None, params, headers, cookies, f, t, extensions, false, kwargs)
    }

    fn aclose<'py>(&mut self, py: Python<'py>) -> PyResult<Bound<'py, PyAny>> {
        self.is_closed = true;
        pyo3_async_runtimes::tokio::future_into_py(py, async move {
            Ok(())
        })
    }

    fn close(&mut self) -> PyResult<()> { self.is_closed = true; Ok(()) }

    #[getter] fn is_closed(&self) -> bool { self.is_closed }

    #[getter]
    fn headers(&self, py: Python<'_>) -> Py<Headers> { self.default_headers.clone_ref(py) }
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
        self.base_url = if s.is_empty() { None } else { Some(URL::create_from_str(&s)?) };
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
    fn get_cookies(&self, _py: Python<'_>) -> Cookies { self.cookies.clone() }
    #[setter]
    fn set_cookies(&mut self, py: Python<'_>, value: &Bound<'_, PyAny>) -> PyResult<()> {
        self.cookies = Cookies::create(py, Some(value))?;
        Ok(())
    }

    #[getter]
    fn get_timeout(&self, py: Python<'_>) -> Py<PyAny> {
        self.timeout.as_ref().map(|t| Py::new(py, t.clone()).ok()).flatten().map(|p| p.into()).unwrap_or_else(|| py.None())
    }

    #[getter]
    fn get_event_hooks(&self, py: Python<'_>) -> Py<PyAny> {
        self.event_hooks.as_ref().map(|e| e.clone_ref(py)).unwrap_or_else(|| {
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
    fn get_trust_env(&self) -> bool { self.trust_env }

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
            } else { None }
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
        } else { None };

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
                    true, None, false, None,
                    Some(&crate::config::Proxy { url: proxy_url, auth: None, headers: None }),
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
            return Err(pyo3::exceptions::PyRuntimeError::new_err("Cannot open a closed client"));
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
                    (t_exc.clone_ref(py).into_bound(py), v.clone_ref(py).into_bound(py), tb.clone_ref(py).into_bound(py))
                } else {
                    (py.None().into_bound(py), py.None().into_bound(py), py.None().into_bound(py))
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
                        (t_exc.clone_ref(py).into_bound(py), v.clone_ref(py).into_bound(py), tb.clone_ref(py).into_bound(py))
                    } else {
                        (py.None().into_bound(py), py.None().into_bound(py), py.None().into_bound(py))
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
        if self.is_closed { "<AsyncClient [closed]>".to_string() } else { "<AsyncClient>".to_string() }
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
