use pyo3::prelude::*;
use pyo3::types::{PyBytes, PyDict, PyList};

use super::headers::Headers;

/// Convert a single value for URL encoding, handling bools and None.
fn convert_value_for_urlencode(py: Python<'_>, v: &Bound<'_, PyAny>) -> Py<PyAny> {
    if v.is_none() {
        pyo3::types::PyString::new(py, "").into_any().unbind()
    } else if let Ok(b) = v.extract::<bool>() {
        pyo3::types::PyString::new(py, if b { "true" } else { "false" })
            .into_any()
            .unbind()
    } else {
        v.clone().unbind()
    }
}

/// Encode form data (dict or list of tuples) into URL-encoded bytes.
/// Handles list values, booleans (lowercased), and None values.
fn encode_form_data(py: Python<'_>, d: &Bound<'_, PyAny>) -> PyResult<Option<Vec<u8>>> {
    let mut items: Vec<(Py<PyAny>, Py<PyAny>)> = Vec::new();
    if let Ok(dict) = d.cast::<pyo3::types::PyDict>() {
        for (k, v) in dict.iter() {
            let k_obj = k.clone().unbind();
            if let Ok(list) = v.cast::<pyo3::types::PyList>() {
                for item in list.iter() {
                    items.push((k_obj.clone_ref(py), convert_value_for_urlencode(py, &item)));
                }
            } else {
                items.push((k_obj, convert_value_for_urlencode(py, &v)));
            }
        }
    } else if let Ok(seq) = d.cast::<pyo3::types::PyList>() {
        for item in seq.iter() {
            if let Ok(pair) = item.cast::<pyo3::types::PyTuple>() {
                if pair.len() == 2 {
                    let k = pair.get_item(0)?;
                    let v = pair.get_item(1)?;
                    let k_obj = k.clone().unbind();
                    items.push((k_obj, convert_value_for_urlencode(py, &v)));
                }
            }
        }
    }

    if items.is_empty() {
        return Ok(None);
    }

    let urllib = py.import("urllib.parse")?;
    let s: String = urllib.call_method1("urlencode", (items,))?.extract()?;
    if !s.is_empty() {
        Ok(Some(s.into_bytes()))
    } else {
        Ok(Some(Vec::new()))
    }
}

/// HTTP Request object.
#[pyclass(from_py_object, module = "httpxr._httpxr")]
pub struct Request {
    #[pyo3(get, set)]
    pub method: String,
    #[pyo3(get, set)]
    pub url: crate::urls::URL,
    #[pyo3(get, set)]
    pub headers: Py<Headers>,
    #[pyo3(get, set)]
    pub extensions: Py<PyAny>,
    pub content_body: Option<Vec<u8>>,
    pub stream: Option<Py<PyAny>>,
    pub stream_response: bool,
}

impl Clone for Request {
    fn clone(&self) -> Self {
        Python::attach(|py| Request {
            method: self.method.clone(),
            url: self.url.clone(),
            headers: self.headers.clone_ref(py),
            extensions: self.extensions.clone_ref(py),
            content_body: self.content_body.clone(),
            stream: self.stream.as_ref().map(|s| s.clone_ref(py)),
            stream_response: self.stream_response,
        })
    }
}

#[pymethods]
impl Request {
    #[new]
    #[pyo3(signature = (method, url, *, params=None, headers=None, stream=None, content=None, data=None, files=None, json=None, extensions=None))]
    fn new(
        py: Python<'_>,
        method: &str,
        url: &Bound<'_, PyAny>,
        params: Option<&Bound<'_, PyAny>>,
        headers: Option<&Bound<'_, PyAny>>,
        stream: Option<Py<PyAny>>,
        content: Option<&Bound<'_, PyAny>>,
        data: Option<&Bound<'_, PyAny>>,
        files: Option<&Bound<'_, PyAny>>,
        json: Option<&Bound<'_, PyAny>>,
        extensions: Option<Py<PyAny>>,
    ) -> PyResult<Self> {
        let mut url_obj = if let Ok(u) = url.extract::<crate::urls::URL>() {
            u
        } else {
            crate::urls::URL::create_from_str(&url.str()?.extract::<String>()?)?
        };

        if let Some(p) = params {
            if !p.is_none() {
                let qp = crate::query_params::QueryParams::create(Some(p))?;
                let qs = qp.encode();
                let host = url_obj.get_host();
                let authority = if let Some(port) = url_obj.get_port() {
                    format!("{}:{}", host, port)
                } else {
                    host
                };
                if qs.is_empty() {
                    url_obj = crate::urls::URL::create_from_str(&format!(
                        "{}://{}{}",
                        url_obj.get_scheme(),
                        authority,
                        url_obj.path_str()
                    ))?;
                } else {
                    url_obj = crate::urls::URL::create_from_str(&format!(
                        "{}://{}{}?{}",
                        url_obj.get_scheme(),
                        authority,
                        url_obj.path_str(),
                        qs
                    ))?;
                }
            }
        }

        let mut hdrs = Headers::create(headers, "utf-8")?;
        let ext = extensions.unwrap_or_else(|| PyDict::new(py).into());

        let mut stream_obj: Option<Py<PyAny>> = stream;

        let (actual_content, used_data_as_content) = if content.is_some() {
            (content, false)
        } else if let Some(d) = data {
            if !d.is_none() && !d.is_instance_of::<pyo3::types::PyDict>() {
                let warnings = py.import("warnings")?;
                warnings.call_method1(
                    "warn",
                    (
                        "Use 'content=...' to upload raw bytes/text content.",
                        py.get_type::<pyo3::exceptions::PyDeprecationWarning>(),
                    ),
                )?;
                (Some(d), true)
            } else {
                (None, false)
            }
        } else {
            (None, false)
        };

        let data_for_encoding = if used_data_as_content { None } else { data };

        let body = if let Some(c) = actual_content {
            if let Ok(b) = c.cast::<PyBytes>() {
                Some(b.as_bytes().to_vec())
            } else if let Ok(s) = c.extract::<String>() {
                Some(s.into_bytes())
            } else if c.is_instance_of::<pyo3::types::PyDict>() {
                return Err(pyo3::exceptions::PyTypeError::new_err(format!(
                    "Unexpected type for 'content': {}",
                    c.get_type().name()?
                )));
            } else if c.hasattr("read").unwrap_or(false) && !c.hasattr("__aiter__").unwrap_or(false)
            {
                let data_bytes: Vec<u8> = c.call_method0("read")?.extract()?;
                let py_bytes = PyBytes::new(py, &data_bytes);
                let list = pyo3::types::PyList::new(py, &[py_bytes.into_any()])?;
                let iter = list.call_method0("__iter__")?;
                let wrapper = crate::types::ResponseIteratorStream::new(iter.unbind());
                let wrapper_py = Py::new(py, wrapper)?;
                stream_obj = Some(wrapper_py.into_any());
                if !hdrs.contains_header("content-length")
                    && !hdrs.contains_header("transfer-encoding")
                {
                    hdrs.set_header("content-length", &data_bytes.len().to_string());
                }
                None
            } else {
                let has_aiter = c.hasattr("__aiter__").unwrap_or(false);
                let has_iter = c.hasattr("__iter__").unwrap_or(false);
                let is_iterator = has_iter
                    && !c.is_instance_of::<pyo3::types::PyBytes>()
                    && !c.is_instance_of::<pyo3::types::PyString>();

                if has_aiter && !is_iterator {
                    let wrapper =
                        crate::types::ResponseAsyncIteratorStream::new(c.clone().unbind());
                    let wrapper_py = Py::new(py, wrapper)?;
                    stream_obj = Some(wrapper_py.into_any());
                    if !hdrs.contains_header("transfer-encoding")
                        && !hdrs.contains_header("content-length")
                    {
                        hdrs.set_header("transfer-encoding", "chunked");
                    }
                    None // body is None, content is streaming
                } else if is_iterator {
                    let wrapper = crate::types::ResponseIteratorStream::new(c.clone().unbind());
                    let wrapper_py = Py::new(py, wrapper)?;
                    stream_obj = Some(wrapper_py.into_any());
                    if !hdrs.contains_header("transfer-encoding")
                        && !hdrs.contains_header("content-length")
                    {
                        hdrs.set_header("transfer-encoding", "chunked");
                    }
                    None // body is None, content is streaming
                } else {
                    return Err(pyo3::exceptions::PyTypeError::new_err(format!(
                        "Unexpected type for 'content': {}",
                        c.get_type().name()?
                    )));
                }
            }
        } else if let Some(j) = json {
            if !j.is_none() {
                let json_mod = py.import("json")?;
                let kwargs = PyDict::new(py);
                let separators = (",", ":").into_pyobject(py)?;
                kwargs.set_item("separators", separators)?;
                kwargs.set_item("ensure_ascii", false)?;
                kwargs.set_item("allow_nan", false)?;
                let s: String = json_mod
                    .call_method("dumps", (j,), Some(&kwargs))?
                    .extract()?;
                if !hdrs.contains_header("content-type") {
                    hdrs.set_header("content-type", "application/json");
                }
                Some(s.into_bytes())
            } else {
                None
            }
        } else if let Some(f) = files {
            let files_empty = if let Ok(dict) = f.cast::<pyo3::types::PyDict>() {
                dict.len() == 0
            } else if let Ok(list) = f.cast::<pyo3::types::PyList>() {
                list.len() == 0
            } else {
                f.is_none()
            };

            let data_empty = if let Some(d) = data_for_encoding {
                if let Ok(dict) = d.cast::<pyo3::types::PyDict>() {
                    dict.len() == 0
                } else {
                    d.is_none()
                }
            } else {
                true
            };

            if !f.is_none() && !(files_empty && data_empty) {
                let boundary = if let Some(ct) = hdrs.get("content-type", None) {
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

                let multipart = crate::multipart::MultipartStream::new(
                    py,
                    data_for_encoding,
                    Some(f),
                    boundary.as_deref(),
                )?;
                if !hdrs.contains_header("content-type") {
                    hdrs.set_header("content-type", &multipart.content_type());
                }
                Some(multipart.get_content())
            } else if let Some(d) = data_for_encoding {
                if !d.is_none() {
                    encode_form_data(py, d)?
                } else {
                    None
                }
            } else {
                None
            }
        } else if let Some(d) = data_for_encoding {
            if !d.is_none() {
                encode_form_data(py, d)?
            } else {
                None
            }
        } else {
            None
        };

        if body.is_some() && !hdrs.contains_header("content-type") {
            if data_for_encoding.is_some() {
                hdrs.set_header("content-type", "application/x-www-form-urlencoded");
            }
        }

        let method_upper = method.to_uppercase();

        if let Some(ref b) = body {
            if !hdrs.contains_header("content-length") && !hdrs.contains_header("transfer-encoding")
            {
                let len_str = b.len().to_string();
                hdrs.set_header("content-length", &len_str);
            }
        } else if stream_obj.is_none()
            && ["POST", "PUT", "PATCH", "DELETE"].contains(&method_upper.as_str())
        {
            if !hdrs.contains_header("content-length") && !hdrs.contains_header("transfer-encoding")
            {
                hdrs.set_header("content-length", "0");
            }
        }

        if !hdrs.contains_header("host") {
            let host = url_obj.get_host();
            if !host.is_empty() {
                let host_val = if let Some(port) = url_obj.get_port() {
                    let scheme = url_obj.get_scheme();
                    let default_port = match scheme {
                        "http" => Some(80),
                        "https" => Some(443),
                        _ => None,
                    };
                    if default_port == Some(port) {
                        host.clone()
                    } else {
                        format!("{}:{}", host, port)
                    }
                } else {
                    host.clone()
                };
                hdrs.set_header("host", &host_val);
            }
        }

        let hdrs_py = Py::new(py, hdrs)?;

        Ok(Request {
            method: method_upper,
            url: url_obj,
            headers: hdrs_py,
            extensions: ext,
            content_body: body,
            stream: stream_obj,
            stream_response: false,
        })
    }

    #[getter]
    fn content(&self, py: Python<'_>) -> PyResult<Py<PyAny>> {
        if let Some(ref c) = self.content_body {
            Ok(PyBytes::new(py, c).into())
        } else if self.stream.is_some() {
            Err(crate::exceptions::RequestNotRead::new_err(
                "Attempted to access streaming request content without having called `.read()`.",
            ))
        } else {
            Ok(PyBytes::new(py, b"").into())
        }
    }
    #[getter]
    fn stream(&self, py: Python<'_>) -> PyResult<Py<PyAny>> {
        if let Some(ref s) = self.stream {
            return Ok(s.clone_ref(py));
        }
        let content = self.content_body.clone().unwrap_or_default();
        let bs = crate::types::ByteStream {
            data: content,
            pos: 0,
        };
        let init = pyo3::PyClassInitializer::from(crate::types::AsyncByteStream)
            .add_subclass(crate::types::SyncByteStream)
            .add_subclass(bs);
        Ok(Py::new(py, init)?.into_any())
    }

    fn read(&mut self, py: Python<'_>) -> PyResult<Vec<u8>> {
        if self.content_body.is_some() {
            return Ok(self.content_body.clone().unwrap_or_default());
        }
        if let Some(ref stream) = self.stream {
            let stream_bound = stream.bind(py);
            let mut buf = Vec::new();
            let iter = stream_bound.call_method0("__iter__")?;
            loop {
                match iter.call_method0("__next__") {
                    Ok(item) => {
                        if let Ok(b) = item.cast::<PyBytes>() {
                            buf.extend_from_slice(b.as_bytes());
                        } else if let Ok(s) = item.extract::<String>() {
                            buf.extend_from_slice(s.as_bytes());
                        }
                    }
                    Err(e) => {
                        if e.is_instance_of::<pyo3::exceptions::PyStopIteration>(py) {
                            break;
                        }
                        return Err(e);
                    }
                }
            }
            self.content_body = Some(buf.clone());
            return Ok(buf);
        }
        Ok(Vec::new())
    }

    fn __repr__(&self) -> String {
        format!("<Request('{}', '{}')>", self.method, self.url.to_string())
    }

    fn __eq__(&self, other: &Request) -> bool {
        self.method == other.method && self.url.to_string() == other.url.to_string()
    }

    fn aread<'py>(slf: &Bound<'py, Self>, py: Python<'py>) -> PyResult<Bound<'py, PyAny>> {
        let mut self_mut = slf.borrow_mut();
        if let Some(ref c) = self_mut.content_body {
            let content = c.clone();
            return pyo3_async_runtimes::tokio::future_into_py(py, async move { Ok(content) });
        }
        if let Some(ref stream) = self_mut.stream {
            let stream_bound = stream.bind(py);
            let has_iter = stream_bound.hasattr("__iter__").unwrap_or(false);
            let has_aiter = stream_bound.hasattr("__aiter__").unwrap_or(false);
            if has_iter && !has_aiter {
                let iter = stream_bound.call_method0("__iter__")?;
                let mut buf = Vec::new();
                loop {
                    match iter.call_method0("__next__") {
                        Ok(item) => {
                            if let Ok(b) = item.cast::<PyBytes>() {
                                buf.extend_from_slice(b.as_bytes());
                            } else if let Ok(s) = item.extract::<String>() {
                                buf.extend_from_slice(s.as_bytes());
                            }
                        }
                        Err(e) => {
                            if e.is_instance_of::<pyo3::exceptions::PyStopIteration>(py) {
                                break;
                            }
                            return Err(e);
                        }
                    }
                }
                self_mut.content_body = Some(buf.clone());
                return pyo3_async_runtimes::tokio::future_into_py(py, async move { Ok(buf) });
            }
            let stream_ref = stream.clone_ref(py);
            drop(self_mut); // Release the borrow before passing slf to Python
            let ns = pyo3::types::PyDict::new(py);
            ns.set_item("_stream", stream_ref)?;
            ns.set_item("_req", slf)?;
            py.run(c"async def _consume_and_store(req, s):\n    buf = b''\n    async for chunk in s:\n        buf += chunk\n    req._set_content_body(buf)\n    return buf\n", None, Some(&ns))?;
            let consume_fn = ns.get_item("_consume_and_store")?.unwrap();
            let stream_arg = ns.get_item("_stream")?.unwrap();
            let req_arg = ns.get_item("_req")?.unwrap();
            let coro = consume_fn.call1((&req_arg, &stream_arg))?;
            return Ok(coro.unbind().into_bound(py));
        }
        drop(self_mut);
        let content: Vec<u8> = Vec::new();
        pyo3_async_runtimes::tokio::future_into_py(py, async move { Ok(content) })
    }

    fn _set_content_body(&mut self, content: Vec<u8>) {
        self.content_body = Some(content);
    }

    fn __reduce__(&self, py: Python<'_>) -> PyResult<Py<PyAny>> {
        let cls = py.import("httpxr")?.getattr("Request")?;
        let args = pyo3::types::PyTuple::new(
            py,
            &[
                self.method.clone().into_pyobject(py)?.into_any(),
                self.url.to_string().into_pyobject(py)?.into_any(),
            ],
        )?;
        let state = pyo3::types::PyDict::new(py);
        let hdrs = self.headers.borrow(py);
        let h_list = PyList::empty(py);
        for (k, v) in &hdrs.raw {
            let t = pyo3::types::PyTuple::new(
                py,
                &[
                    PyBytes::new(py, k).into_any(),
                    PyBytes::new(py, v).into_any(),
                ],
            )?;
            h_list.append(t)?;
        }
        state.set_item("headers", h_list)?;
        if let Some(ref c) = self.content_body {
            state.set_item("content", PyBytes::new(py, c))?;
        }
        if self.stream.is_some() && self.content_body.is_none() {
            state.set_item("had_stream", true)?;
        }
        let result =
            pyo3::types::PyTuple::new(py, &[cls.into_any(), args.into_any(), state.into_any()])?;
        Ok(result.into())
    }

    fn __setstate__(&mut self, py: Python<'_>, state: &Bound<'_, PyAny>) -> PyResult<()> {
        let dict = state.cast::<pyo3::types::PyDict>()?;
        if let Some(content) = dict.get_item("content")? {
            self.content_body = Some(content.extract::<Vec<u8>>()?);
        }
        if let Some(headers) = dict.get_item("headers")? {
            let h_list: Vec<(Vec<u8>, Vec<u8>)> = headers.extract()?;
            let hdrs = Headers {
                raw: h_list,
                encoding: "utf-8".to_string(),
            };
            self.headers = Py::new(py, hdrs)?;
        }
        if let Some(had_stream) = dict.get_item("had_stream")? {
            if had_stream.extract::<bool>().unwrap_or(false) && self.content_body.is_none() {
                let ns = pyo3::types::PyDict::new(py);
                py.run(c"class _ClosedStream:\n    def __iter__(self): raise __import__('httpxr').StreamClosed(self)\n    def __aiter__(self): return self\n    async def __anext__(self): raise __import__('httpxr').StreamClosed(self)\n_result = _ClosedStream()\n", None, Some(&ns))?;
                let closed_stream = ns.get_item("_result")?.unwrap();
                self.stream = Some(closed_stream.unbind());
            }
        }
        Ok(())
    }
}
