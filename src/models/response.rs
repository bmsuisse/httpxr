use pyo3::prelude::*;
use pyo3::types::{PyBytes, PyDict};

use super::cookies::Cookies;
use super::headers::{serde_value_to_py, simd_value_to_py, Headers};
use super::request::Request;

/// HTTP Response object.
#[pyclass(from_py_object, module = "httpxr._httpxr")]
pub struct Response {
    #[pyo3(get, set)]
    pub status_code: u16,
    #[pyo3(get, set)]
    pub headers: Py<Headers>,
    pub extensions: Py<PyAny>,
    pub request: Option<Request>,
    /// Lazy request fields — set by the fast path to defer Request construction.
    /// When set, the Request is materialized on first `.request` property access.
    pub lazy_request_method: Option<String>,
    pub lazy_request_url: Option<String>,
    #[pyo3(get, set)]
    pub history: Vec<Response>,
    pub content_bytes: Option<Vec<u8>>,
    pub stream: Option<Py<PyAny>>,
    pub default_encoding: Py<PyAny>,
    pub default_encoding_override: Option<String>,
    pub elapsed: Option<f64>,
    pub is_closed_flag: bool,
    pub is_stream_consumed: bool,
    pub was_streaming: bool,
    pub text_accessed: std::sync::atomic::AtomicBool,
    pub num_bytes_downloaded_counter: std::sync::Arc<std::sync::atomic::AtomicUsize>,
}

impl Clone for Response {
    fn clone(&self) -> Self {
        Python::attach(|py| Response {
            status_code: self.status_code,
            headers: self.headers.clone_ref(py),
            extensions: self.extensions.clone_ref(py),
            request: self.request.clone(),
            lazy_request_method: self.lazy_request_method.clone(),
            lazy_request_url: self.lazy_request_url.clone(),
            history: self.history.clone(),
            content_bytes: self.content_bytes.clone(),
            stream: self.stream.as_ref().map(|s| s.clone_ref(py)),
            default_encoding: self.default_encoding.clone_ref(py),
            default_encoding_override: self.default_encoding_override.clone(),
            elapsed: self.elapsed,
            is_closed_flag: self.is_closed_flag,
            is_stream_consumed: self.is_stream_consumed,
            was_streaming: self.was_streaming,
            text_accessed: std::sync::atomic::AtomicBool::new(
                self.text_accessed
                    .load(std::sync::atomic::Ordering::Relaxed),
            ),
            num_bytes_downloaded_counter: std::sync::Arc::new(std::sync::atomic::AtomicUsize::new(
                self.num_bytes_downloaded_counter
                    .load(std::sync::atomic::Ordering::Relaxed),
            )),
        })
    }
}

impl Response {
    pub fn has_redirect_check(&self, py: Python<'_>) -> bool {
        (300..400).contains(&self.status_code)
            && self.headers.bind(py).borrow().contains_header("location")
    }

    /// Get method string for comparison — works with both eager and lazy request fields.
    fn get_method_str(&self) -> Option<String> {
        if let Some(ref r) = self.request {
            Some(r.method.clone())
        } else {
            self.lazy_request_method.clone()
        }
    }

    /// Get URL string for comparison — works with both eager and lazy request fields.
    fn get_url_str(&self) -> Option<String> {
        if let Some(ref r) = self.request {
            Some(r.url.to_string())
        } else {
            self.lazy_request_url.clone()
        }
    }

    pub fn create(
        status_code: u16,
        headers: Py<Headers>,
        extensions: Py<PyAny>,
        request: Option<Request>,
        content_bytes: Option<Vec<u8>>,
    ) -> Self {
        Python::attach(|py| Response {
            status_code,
            headers,
            extensions,
            request,
            lazy_request_method: None,
            lazy_request_url: None,
            history: Vec::new(),
            content_bytes,
            stream: None,
            default_encoding: "utf-8".into_pyobject(py).unwrap().into_any().unbind(),
            default_encoding_override: None,
            elapsed: None,
            is_closed_flag: false,
            is_stream_consumed: false,
            was_streaming: false,
            text_accessed: std::sync::atomic::AtomicBool::new(false),
            num_bytes_downloaded_counter: std::sync::Arc::new(std::sync::atomic::AtomicUsize::new(
                0,
            )),
        })
    }
}

impl Response {
    /// Decompress content based on Content-Encoding header.
    /// Supports: gzip, deflate, br (brotli), zstd, identity, and chained encodings.
    /// Decompress content based on Content-Encoding header.
    /// Supports: gzip, deflate, br (brotli), zstd, identity, and chained encodings.
    /// Uses pure Rust crates to release the GIL.
    pub fn decompress_content(
        _py: Python<'_>,
        data: &[u8],
        headers: &Headers,
    ) -> PyResult<Vec<u8>> {
        if data.is_empty() {
            return Ok(Vec::new());
        }

        let encoding = headers
            .get_first_value("content-encoding")
            .unwrap_or_default();
        if encoding.is_empty() {
            return Ok(data.to_vec());
        }

        let mut encodings: Vec<&str> = encoding.split(',').map(|s| s.trim()).collect();
        encodings.reverse();

        let mut result = data.to_vec();
        for enc in encodings.iter() {
            let enc_lower = enc.to_lowercase();
            result = match enc_lower.as_str() {
                "identity" => result,
                "gzip" => {
                    use std::io::Read;
                    let mut decoder = flate2::read::GzDecoder::new(&result[..]);
                    let mut decoded = Vec::new();
                    decoder.read_to_end(&mut decoded).map_err(|e| {
                        crate::exceptions::DecodingError::new_err(format!(
                            "Gzip decompression failed: {}",
                            e
                        ))
                    })?;
                    decoded
                }
                "deflate" => {
                    use std::io::Read;
                    let mut decoder = flate2::read::ZlibDecoder::new(&result[..]);
                    let mut decoded = Vec::new();
                    if decoder.read_to_end(&mut decoded).is_ok() {
                        decoded
                    } else {
                        let mut decoder = flate2::read::DeflateDecoder::new(&result[..]);
                        let mut decoded = Vec::new();
                        decoder.read_to_end(&mut decoded).map_err(|e| {
                            crate::exceptions::DecodingError::new_err(format!(
                                "Deflate decompression failed: {}",
                                e
                            ))
                        })?;
                        decoded
                    }
                }
                "br" => {
                    use std::io::Read;
                    let mut decoder = brotli::Decompressor::new(&result[..], 4096);
                    let mut decoded = Vec::new();
                    decoder.read_to_end(&mut decoded).map_err(|e| {
                        crate::exceptions::DecodingError::new_err(format!(
                            "Brotli decompression failed: {}",
                            e
                        ))
                    })?;
                    decoded
                }
                "zstd" => {
                    let mut output = Vec::new();
                    let mut cursor = std::io::Cursor::new(&result);
                    loop {
                        let pos = cursor.position() as usize;
                        if pos >= result.len() {
                            break;
                        }
                        let remaining = &result[pos..];
                        if remaining.len() >= 8 {
                            let magic = u32::from_le_bytes([
                                remaining[0],
                                remaining[1],
                                remaining[2],
                                remaining[3],
                            ]);
                            if (0x184D2A50..=0x184D2A5F).contains(&magic) {
                                let frame_size = u32::from_le_bytes([
                                    remaining[4],
                                    remaining[5],
                                    remaining[6],
                                    remaining[7],
                                ]) as usize;
                                cursor.set_position((pos + 8 + frame_size) as u64);
                                continue;
                            }
                        }
                        match zstd::stream::decode_all(remaining) {
                            Ok(decoded) => {
                                output.extend(decoded);
                                break; // decode_all consumes all remaining frames
                            }
                            Err(_e) => {
                                let mut decoder =
                                    zstd::stream::Decoder::new(remaining).map_err(|e| {
                                        crate::exceptions::DecodingError::new_err(format!(
                                            "Zstd decompression failed: {}",
                                            e
                                        ))
                                    })?;
                                use std::io::Read;
                                let mut frame_output = Vec::new();
                                decoder.read_to_end(&mut frame_output).map_err(|e| {
                                    crate::exceptions::DecodingError::new_err(format!(
                                        "Zstd decompression failed: {}",
                                        e
                                    ))
                                })?;
                                output.extend(frame_output);
                                break;
                            }
                        }
                    }
                    output
                }
                _ => {
                    result
                }
            };
        }

        Ok(result)
    }

    pub(crate) fn text_impl(&self, py: Python<'_>) -> PyResult<String> {
        self.text_accessed
            .store(true, std::sync::atomic::Ordering::Relaxed);
        if let Some(ref c) = self.content_bytes {
            if c.is_empty() {
                return Ok(String::new());
            }
            let encoding_name = self.resolve_encoding(py);
            let py_bytes = PyBytes::new(py, c);
            let str_obj = py_bytes.call_method1("decode", (encoding_name, "replace"))?;
            str_obj.extract()
        } else {
            Ok(String::new())
        }
    }

    pub(crate) fn read_impl(&mut self, py: Python<'_>) -> PyResult<Vec<u8>> {
        if self.was_streaming && self.is_stream_consumed && self.stream.is_none() {
            return Err(crate::exceptions::StreamConsumed::new_err(
                "Attempted to read or stream content, but the stream has already been consumed.",
            ));
        }
        if let Some(ref c) = self.content_bytes {
            return Ok(c.clone());
        }
        if self.is_closed_flag {
            return Err(crate::exceptions::StreamClosed::new_err(
                "Attempted to read or stream content, but the stream has been closed.",
            ));
        }
        if self.is_stream_consumed {
            return Err(crate::exceptions::StreamConsumed::new_err(
                "Attempted to read or stream content, but the stream has already been consumed.",
            ));
        }
        if let Some(stream_py) = self.stream.take() {
            let mut buf = Vec::new();
            let s = stream_py.bind(py);
            if s.hasattr("__aiter__")? && !s.hasattr("__iter__")? {
                return Err(pyo3::exceptions::PyRuntimeError::new_err(
                    "Attempted to read an asynchronous response using 'response.read()'. Use 'await response.aread()' instead."));
            }
            let iter_obj = s.call_method0("__iter__")?;
            loop {
                match iter_obj.call_method0("__next__") {
                    Ok(item) => {
                        if let Ok(b) = item.cast::<PyBytes>() {
                            buf.extend_from_slice(b.as_bytes());
                        } else if let Ok(v) = item.extract::<Vec<u8>>() {
                            buf.extend(v);
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
            let hdrs_ref = self.headers.borrow(py);
            let buf = Response::decompress_content(py, &buf, &hdrs_ref)?;
            self.content_bytes = Some(buf.clone());
            self.is_stream_consumed = true;
            self.is_closed_flag = true;
            self.num_bytes_downloaded_counter
                .store(buf.len(), std::sync::atomic::Ordering::Relaxed);
            return Ok(buf);
        }
        self.content_bytes = Some(Vec::new());
        self.is_closed_flag = true;
        Ok(Vec::new())
    }

    pub(crate) fn is_success_impl(&self) -> bool {
        (200..300).contains(&self.status_code)
    }

    pub(crate) fn reason_phrase_impl(&self) -> String {
        crate::status_codes::status_code_map()
            .get(&self.status_code)
            .copied()
            .unwrap_or("")
            .to_string()
    }
}

#[pymethods]
impl Response {
    #[new]
    #[pyo3(signature = (status_code, headers=None, stream=None, content=None, text=None, html=None, json=None, request=None, extensions=None, default_encoding=None, history=None))]
    fn new(
        py: Python<'_>,
        status_code: u16,
        headers: Option<&Bound<'_, PyAny>>,
        stream: Option<Py<PyAny>>,
        content: Option<&Bound<'_, PyAny>>,
        text: Option<&str>,
        html: Option<&str>,
        json: Option<&Bound<'_, PyAny>>,
        request: Option<Request>,
        extensions: Option<Py<PyAny>>,
        default_encoding: Option<&Bound<'_, PyAny>>,
        history: Option<Vec<Response>>,
    ) -> PyResult<Self> {
        let mut hdrs = Headers::create(headers, "utf-8")?;

        let ext = extensions.unwrap_or_else(|| PyDict::new(py).into());

        let (content_bytes, stream_obj) = if let Some(c) = content {
            if let Ok(b) = c.cast::<PyBytes>() {
                (Some(b.as_bytes().to_vec()), None)
            } else if let Ok(s) = c.extract::<String>() {
                (Some(s.into_bytes()), None)
            } else {
                let has_aiter = c.hasattr("__aiter__").unwrap_or(false);
                let has_iter = c.hasattr("__iter__").unwrap_or(false);
                if has_aiter && !has_iter {
                    if !hdrs.contains_header("transfer-encoding")
                        && !hdrs.contains_header("content-length")
                    {
                        hdrs.set_header("transfer-encoding", "chunked");
                    }
                    let wrapper =
                        crate::types::ResponseAsyncIteratorStream::new(c.clone().unbind());
                    let wrapper_py = Py::new(py, wrapper)?;
                    (None, Some(wrapper_py.into_any()))
                } else if has_iter {
                    if !hdrs.contains_header("transfer-encoding")
                        && !hdrs.contains_header("content-length")
                    {
                        hdrs.set_header("transfer-encoding", "chunked");
                    }
                    let wrapper = crate::types::ResponseIteratorStream::new(c.clone().unbind());
                    let wrapper_py = Py::new(py, wrapper)?;
                    (None, Some(wrapper_py.into_any()))
                } else {
                    return Err(pyo3::exceptions::PyTypeError::new_err(format!(
                        "Unexpected type for 'content': {}",
                        c.get_type().name()?
                    )));
                }
            }
        } else if let Some(t) = text {
            if !hdrs.contains_header("content-type") {
                hdrs.set_header("content-type", "text/plain; charset=utf-8");
            }
            (Some(t.as_bytes().to_vec()), None)
        } else if let Some(h) = html {
            if !hdrs.contains_header("content-type") {
                hdrs.set_header("content-type", "text/html; charset=utf-8");
            }
            (Some(h.as_bytes().to_vec()), None)
        } else if let Some(j) = json {
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
            (Some(s.into_bytes()), None)
        } else {
            (None, None)
        };

        if let Some(ref cb) = content_bytes {
            if !hdrs.contains_header("content-length") && !hdrs.contains_header("transfer-encoding")
            {
                hdrs.set_header("content-length", &cb.len().to_string());
            }
        }

        let de = if let Some(enc) = default_encoding {
            enc.clone().unbind()
        } else {
            "utf-8".into_pyobject(py).unwrap().into_any().unbind()
        };

        let hdrs_py = Py::new(py, hdrs)?;

        let final_stream = stream.or(stream_obj);
        let _is_streaming = final_stream.is_some();

        let (final_content, final_closed) = if content_bytes.is_none() && final_stream.is_none() {
            (Some(Vec::new()), true)
        } else if content_bytes.is_some() {
            (content_bytes, true) // Non-streaming responses are immediately closed
        } else {
            (None, false) // Streaming responses start open
        };

        let final_content = if let Some(ref data) = final_content {
            if !data.is_empty() {
                let hdrs_ref = hdrs_py.borrow(py);
                match Response::decompress_content(py, data, &hdrs_ref) {
                    Ok(decompressed) => Some(decompressed),
                    Err(e) => return Err(e),
                }
            } else {
                final_content
            }
        } else {
            final_content
        };

        let is_streaming = final_stream.is_some();
        let initial_consumed = !is_streaming && final_content.is_some();
        let initial_downloaded = if !is_streaming {
            final_content.as_ref().map(|c| c.len()).unwrap_or(0)
        } else {
            0
        };

        Ok(Response {
            status_code,
            headers: hdrs_py,
            extensions: ext,
            request,
            lazy_request_method: None,
            lazy_request_url: None,
            history: history.unwrap_or_default(),
            content_bytes: final_content,
            stream: final_stream,
            default_encoding: de,
            default_encoding_override: None,
            elapsed: None,
            is_closed_flag: final_closed,
            is_stream_consumed: initial_consumed,
            was_streaming: is_streaming,
            text_accessed: std::sync::atomic::AtomicBool::new(false),
            num_bytes_downloaded_counter: std::sync::Arc::new(std::sync::atomic::AtomicUsize::new(
                initial_downloaded,
            )),
        })
    }

    #[getter]
    fn content(&self, py: Python<'_>) -> PyResult<Py<PyAny>> {
        if let Some(ref c) = self.content_bytes {
            Ok(PyBytes::new(py, c).into())
        } else {
            Err(crate::exceptions::ResponseNotRead::new_err(
                "Attempted to access content, without having called `read()`.",
            ))
        }
    }

    #[getter]
    fn stream(&self, py: Python<'_>) -> PyResult<Py<PyAny>> {
        if let Some(ref s) = self.stream {
            if self.is_stream_consumed {
                return Err(crate::exceptions::StreamConsumed::new_err(
                    "Attempted to read or stream content, but the stream has already been consumed."
                ));
            }
            return Ok(s.clone_ref(py));
        }
        let content = self.content_bytes.clone().unwrap_or_default();
        let bs = crate::types::ByteStream {
            data: content,
            pos: 0,
        };
        let init = pyo3::PyClassInitializer::from(crate::types::AsyncByteStream)
            .add_subclass(crate::types::SyncByteStream)
            .add_subclass(bs);
        Ok(Py::new(py, init)?.into_any())
    }

    #[getter]
    fn text(&self, py: Python<'_>) -> PyResult<String> {
        self.text_impl(py)
    }

    fn __eq__(&self, other: &Response) -> bool {
        self.status_code == other.status_code
            && self.get_method_str() == other.get_method_str()
            && self.get_url_str() == other.get_url_str()
            && self.content_bytes.as_ref().map(|c| c.len()) == other.content_bytes.as_ref().map(|c| c.len())
    }

    #[pyo3(signature = (**kwargs))]
    fn json(&self, py: Python<'_>, kwargs: Option<&Bound<'_, PyDict>>) -> PyResult<Py<PyAny>> {
        let content = self.content_bytes.as_ref().ok_or_else(|| {
            crate::exceptions::ResponseNotRead::new_err(
                "Attempted to access json, without having called `read()`.",
            )
        })?;

        if kwargs.is_some() {
            let text = self.text(py)?;
            let json_mod = py.import("json")?;
            let loads = json_mod.getattr("loads")?;
            let py_text = text.into_pyobject(py).unwrap().into_any();
            let args = (py_text,);
            return Ok(loads.call(args, kwargs)?.into());
        }

        match serde_json::from_slice::<serde_json::Value>(content) {
            Ok(val) => serde_value_to_py(py, &val),
            Err(_) => {
                let mut buf = content.clone();
                match simd_json::to_owned_value(&mut buf) {
                    Ok(val) => simd_value_to_py(py, &val),
                    Err(_) => {
                        let text = self.text(py)?;
                        let json_mod = py.import("json")?;
                        Ok(json_mod.call_method1("loads", (text,))?.into())
                    }
                }
            }
        }
    }

    #[getter]
    fn get_request(&mut self, py: Python<'_>) -> PyResult<Request> {
        if self.request.is_none() {
            if let (Some(method), Some(url_str)) = (self.lazy_request_method.take(), self.lazy_request_url.take()) {
                let req_url = crate::urls::URL::create_from_str_fast(&url_str);
                let req = Request {
                    method,
                    url: req_url,
                    headers: Py::new(py, Headers::empty())?,
                    extensions: PyDict::new(py).into(),
                    content_body: None,
                    stream: None,
                    stream_response: false,
                };
                self.request = Some(req);
            }
        }
        match &self.request {
            Some(r) => Ok(r.clone()),
            None => Err(pyo3::exceptions::PyRuntimeError::new_err(
                "The request instance has not been set on this response.",
            )),
        }
    }

    #[setter]
    fn set_request(&mut self, value: Request) {
        self.request = Some(value);
    }

    fn read(&mut self, py: Python<'_>) -> PyResult<Vec<u8>> {
        self.read_impl(py)
    }

    fn close(&mut self, py: Python<'_>) -> PyResult<()> {
        if let Some(ref stream) = self.stream {
            let s = stream.bind(py);
            let has_aiter = s.hasattr("__aiter__").unwrap_or(false);
            let has_iter = s.hasattr("__iter__").unwrap_or(false);
            if has_aiter && !has_iter {
                return Err(pyo3::exceptions::PyRuntimeError::new_err(
                    "Attempted to close an asynchronous response using 'response.close()'. Use 'await response.aclose()' instead."));
            }
        }
        self.is_closed_flag = true;
        self.stream = None;
        Ok(())
    }

    #[getter]
    fn is_informational(&self) -> bool {
        (100..200).contains(&self.status_code)
    }
    #[getter]
    fn is_success(&self) -> bool {
        self.is_success_impl()
    }
    #[getter]
    fn is_redirect(&self) -> bool {
        (300..400).contains(&self.status_code)
    }
    #[getter]
    fn is_client_error(&self) -> bool {
        (400..500).contains(&self.status_code)
    }
    #[getter]
    fn is_server_error(&self) -> bool {
        (500..600).contains(&self.status_code)
    }
    #[getter]
    fn is_error(&self) -> bool {
        (400..600).contains(&self.status_code)
    }
    #[getter]
    fn has_redirect_location(&self, py: Python<'_>) -> bool {
        self.is_redirect() && self.headers.bind(py).borrow().contains_header("location")
    }

    #[getter]
    fn next_request<'py>(&self, py: Python<'py>) -> PyResult<Option<Request>> {
        if !self.is_redirect() {
            return Ok(None);
        }
        let location = if let Some(loc) = self.headers.bind(py).borrow().get_first_value("location")
        {
            loc
        } else {
            return Ok(None);
        };

        if let Some(req) = &self.request {
            let new_url = req.url.join_relative(&location)?;

            let (method, body, headers) = match self.status_code {
                303 => ("GET", None, None),
                301 | 302 => {
                    if req.method == "POST" {
                        ("GET", None, None)
                    } else {
                        (
                            req.method.as_str(),
                            req.content_body.clone(),
                            Some(req.headers.clone_ref(py)),
                        )
                    }
                }
                307 | 308 => (
                    req.method.as_str(),
                    req.content_body.clone(),
                    Some(req.headers.clone_ref(py)),
                ),
                _ => ("GET", None, None),
            };

            let new_headers = if let Some(h) = headers {
                h
            } else {
                req.headers.clone_ref(py)
            };

            if method == "GET" && req.method != "GET" {
                let mut h = new_headers.bind(py).borrow_mut();
                h.remove_header("content-length");
                h.remove_header("content-type");
                h.remove_header("transfer-encoding");
                drop(h);
            }


            Ok(Some(Request {
                method: method.to_string(),
                url: new_url,
                headers: new_headers,
                extensions: req.extensions.clone_ref(py),
                content_body: body,
                stream: None,
                stream_response: false,
            }))
        } else {
            Ok(None)
        }
    }

    #[getter]
    pub fn reason_phrase(&self) -> String {
        self.reason_phrase_impl()
    }

    #[getter]
    fn http_version(&self) -> String {
        "HTTP/1.1".to_string()
    }
    #[getter]
    fn is_closed(&self) -> bool {
        self.is_closed_flag
    }

    #[getter]
    fn extensions(&mut self, py: Python<'_>) -> Py<PyAny> {
        if self.extensions.is_none(py) {
            let ext = PyDict::new(py);
            let _ = ext.set_item("http_version", PyBytes::new(py, b"HTTP/1.1"));
            self.extensions = ext.into_any().unbind();
        }
        self.extensions.clone_ref(py)
    }

    #[setter]
    fn set_extensions(&mut self, value: Py<PyAny>) {
        self.extensions = value;
    }
    #[getter]
    fn is_stream_consumed_prop(&self) -> bool {
        self.is_stream_consumed
    }
    #[getter]
    fn num_bytes_downloaded(&self) -> usize {
        self.num_bytes_downloaded_counter
            .load(std::sync::atomic::Ordering::Relaxed)
    }

    #[getter]
    fn cookies(&self, py: Python<'_>) -> PyResult<Cookies> {
        let mut cookie_list = Vec::new();
        for (k, v) in self.headers.bind(py).borrow().get_multi_items() {
            if k == "set-cookie" {
                cookie_list.push(v);
            }
        }

        let cookie_jar = py.import("http.cookiejar")?;
        let jar = cookie_jar.call_method0("CookieJar")?;

        if !cookie_list.is_empty() {
            for cookie_str in &cookie_list {
                if let Some(pos) = cookie_str.find('=') {
                    let name = &cookie_str[..pos];
                    let value = &cookie_str[pos + 1..];
                    Cookies::set_cookie_on_jar_inner(py, &jar, name, value, "", "/")?;
                }
            }
        }
        Ok(Cookies { jar: jar.into() })
    }

    fn aread<'py>(slf: Py<Self>, py: Python<'py>) -> PyResult<Bound<'py, PyAny>> {
        let (content_bytes, stream_ref, is_closed, is_consumed, was_streaming) = {
            let response = slf.borrow(py);
            (
                response.content_bytes.clone(),
                response.stream.as_ref().map(|s| s.clone_ref(py)),
                response.is_closed_flag,
                response.is_stream_consumed,
                response.was_streaming,
            )
        };

        if let Some(bytes) = content_bytes {
            return pyo3_async_runtimes::tokio::future_into_py(py, async move { Ok(bytes) });
        }

        if was_streaming && is_consumed && stream_ref.is_none() {
            return Err(crate::exceptions::StreamConsumed::new_err(
                "Attempted to read or stream content, but the stream has already been consumed.",
            ));
        }

        if is_closed {
            return Err(crate::exceptions::StreamClosed::new_err(
                "Attempted to read or stream content, but the stream has been closed.",
            ));
        }
        if is_consumed {
            return Err(crate::exceptions::StreamConsumed::new_err(
                "Attempted to read or stream content, but the stream has already been consumed.",
            ));
        }

        if let Some(stream_py) = stream_ref {
            let s = stream_py.bind(py);
            if let Ok(iter) = s.try_iter() {
                let mut buf = Vec::new();
                for item in iter {
                    let item = item?;
                    if let Ok(b) = item.cast::<PyBytes>() {
                        buf.extend_from_slice(b.as_bytes());
                    } else if let Ok(v) = item.extract::<Vec<u8>>() {
                        buf.extend(v);
                    }
                }
                let hdrs_ref = {
                    let response = slf.borrow(py);
                    response.headers.clone_ref(py)
                };
                let hdrs_borrowed = hdrs_ref.borrow(py);
                let buf = Response::decompress_content(py, &buf, &hdrs_borrowed)?;
                let result = buf.clone();
                {
                    let mut response = slf.borrow_mut(py);
                    response.content_bytes = Some(buf);
                    response.is_stream_consumed = true;
                    response.is_closed_flag = true;
                    response.stream = None; // Stream is consumed
                    response
                        .num_bytes_downloaded_counter
                        .store(result.len(), std::sync::atomic::Ordering::Relaxed);
                }
                return pyo3_async_runtimes::tokio::future_into_py(py, async move { Ok(result) });
            }
        }

        let slf_bound = slf.bind(py);
        {
            let mut self_mut = slf_bound.borrow_mut();
            if self_mut.stream.is_none() {
                self_mut.content_bytes = Some(Vec::new());
                self_mut.is_closed_flag = true;
                drop(self_mut);
                return pyo3_async_runtimes::tokio::future_into_py(py, async move {
                    Ok(Vec::<u8>::new())
                });
            }
        }

        let ns = pyo3::types::PyDict::new(py);
        ns.set_item("_resp", slf_bound)?;
        py.run(
            c"
async def _aread_impl(resp):
    stream = resp._take_stream()
    if stream is None:
        return b''
    buf = b''
    try:
        async for chunk in stream:
            buf += bytes(chunk)
    except BaseException:
        # Close the stream if it has aclose
        if hasattr(stream, 'aclose'):
            try:
                await stream.aclose()
            except Exception:
                pass
        raise
    result = resp._set_aread_result(buf)
    return result
",
            Some(&ns),
            Some(&ns),
        )?;
        let aread_fn = ns.get_item("_aread_impl")?.unwrap();
        let coro = aread_fn.call1((slf_bound,))?;
        Ok(coro.unbind().into_bound(py))
    }

    /// Helper for Python-based aread: take the stream out of the response
    fn _take_stream(&mut self, _py: Python<'_>) -> Option<Py<PyAny>> {
        self.stream.take()
    }

    /// Helper: mark the response as closed (used by Python-side error cleanup)
    fn _mark_closed(&mut self) {
        self.is_closed_flag = true;
        self.stream = None;
    }

    /// Helper for Python-based aread: store the read result and apply decompression
    fn _set_aread_result(&mut self, py: Python<'_>, data: Vec<u8>) -> PyResult<Vec<u8>> {
        let hdrs_ref = self.headers.borrow(py);
        let decompressed = Response::decompress_content(py, &data, &hdrs_ref)?;
        self.content_bytes = Some(decompressed.clone());
        self.is_stream_consumed = true;
        self.is_closed_flag = true;
        self.stream = None;
        self.num_bytes_downloaded_counter
            .store(decompressed.len(), std::sync::atomic::Ordering::Relaxed);
        Ok(decompressed)
    }

    /// Helper: increment num_bytes_downloaded counter (used by Python-side async iterators)
    fn _add_bytes_downloaded(&self, n: usize) {
        self.num_bytes_downloaded_counter
            .fetch_add(n, std::sync::atomic::Ordering::Relaxed);
    }

    fn raise_for_status(slf: Bound<'_, Self>) -> PyResult<Bound<'_, Self>> {
        let py = slf.py();
        {
            let mut self_mut = slf.borrow_mut();
            if self_mut.request.is_none() {
                if let (Some(method), Some(url_str)) = (self_mut.lazy_request_method.take(), self_mut.lazy_request_url.take()) {
                    let req_url = crate::urls::URL::create_from_str_fast(&url_str);
                    let req = Request {
                        method,
                        url: req_url,
                        headers: Py::new(py, Headers::empty())?,
                        extensions: PyDict::new(py).into(),
                        content_body: None,
                        stream: None,
                        stream_response: false,
                    };
                    self_mut.request = Some(req);
                }
            }
        }
        let self_ = slf.borrow();
        if self_.request.is_none() {
            return Err(pyo3::exceptions::PyRuntimeError::new_err(
                "Cannot call `raise_for_status` as the request instance has not been set on this response."
            ));
        }
        let raise_error = |msg: String| -> PyResult<Bound<'_, Self>> {
            let py = slf.py();
            let exc_type = py.get_type::<crate::exceptions::HTTPStatusError>();
            let exc = exc_type.call1((msg,))?;

            let req = self_.request.as_ref().unwrap();
            let req_py = pyo3::Py::new(py, req.clone())?;
            exc.setattr("request", req_py)?;
            exc.setattr("response", slf.clone())?;

            Err(PyErr::from_value(exc.into()))
        };

        let request = self_.request.as_ref().unwrap();
        let url_str = request.url.to_string();
        let reason = self_.reason_phrase();
        let status_with_reason = if reason.is_empty() {
            format!("{}", self_.status_code)
        } else {
            format!("'{} {}'", self_.status_code, reason)
        };
        let mdn_url = format!(
            "https://developer.mozilla.org/en-US/docs/Web/HTTP/Status/{}",
            self_.status_code
        );

        if (100..200).contains(&self_.status_code) {
            let msg = format!(
                "Informational response {} for url '{}'\nFor more information check: {}",
                status_with_reason, url_str, mdn_url
            );
            return raise_error(msg);
        }
        if (300..400).contains(&self_.status_code) {
            let mut msg = format!(
                "Redirect response {} for url '{}'",
                status_with_reason, url_str
            );
            if let Some(loc) = self_.headers.bind(py).borrow().get_first_value("location") {
                msg.push_str(&format!("\nRedirect location: '{}'", loc));
            }
            msg.push_str(&format!("\nFor more information check: {}", mdn_url));
            return raise_error(msg);
        }
        if (400..500).contains(&self_.status_code) {
            let msg = format!(
                "Client error {} for url '{}'\nFor more information check: {}",
                status_with_reason, url_str, mdn_url
            );
            return raise_error(msg);
        }
        if (500..600).contains(&self_.status_code) {
            let msg = format!(
                "Server error {} for url '{}'\nFor more information check: {}",
                status_with_reason, url_str, mdn_url
            );
            return raise_error(msg);
        }
        drop(self_);
        Ok(slf)
    }

    fn __enter__(slf: PyRef<'_, Self>) -> PyRef<'_, Self> {
        slf
    }
    fn __exit__(
        &mut self,
        py: Python<'_>,
        _exc_type: Option<&Bound<'_, PyAny>>,
        _exc_value: Option<&Bound<'_, PyAny>>,
        _traceback: Option<&Bound<'_, PyAny>>,
    ) -> PyResult<()> {
        self.close(py)
    }

    #[getter]
    pub(crate) fn url(&self) -> PyResult<crate::urls::URL> {
        if let Some(ref req) = self.request {
            Ok(req.url.clone())
        } else if let Some(ref url_str) = self.lazy_request_url {
            Ok(crate::urls::URL::create_from_str_fast(url_str))
        } else {
            Ok(crate::urls::URL::create_from_str("")?)
        }
    }

    #[getter]
    fn encoding(&self, py: Python<'_>) -> Option<String> {
        if let Some(ref enc) = self.default_encoding_override {
            return Some(enc.clone());
        }
        if let Some(ct) = self
            .headers
            .bind(py)
            .borrow()
            .get_first_value("content-type")
        {
            if let Some(idx) = ct.to_lowercase().find("charset=") {
                let charset = &ct[idx + 8..];
                let charset = charset.split(';').next().unwrap_or(charset).trim();
                let codecs = py.import("codecs").ok();
                let is_valid = codecs
                    .as_ref()
                    .map(|c| c.call_method1("lookup", (charset,)).is_ok())
                    .unwrap_or(false);
                if is_valid {
                    return Some(charset.to_string());
                }
                return Some(self.get_encoding_str(py));
            }
        }
        if let Some(ref c) = self.content_bytes {
            if c.starts_with(&[0x00, 0x00, 0xFE, 0xFF]) {
                return Some("utf-32".to_string());
            }
            if c.starts_with(&[0xFF, 0xFE, 0x00, 0x00]) {
                return Some("utf-32".to_string());
            }
            if c.starts_with(&[0xFE, 0xFF]) {
                return Some("utf-16".to_string());
            }
            if c.starts_with(&[0xFF, 0xFE]) {
                return Some("utf-16".to_string());
            }
            if c.starts_with(&[0xEF, 0xBB, 0xBF]) {
                return Some("utf-8-sig".to_string());
            }

            if c.len() >= 4 {
                if c[0] == 0 && c[1] == 0 && c[2] == 0 {
                    return Some("utf-32-be".to_string());
                }
                if c[0] == 0 && c[2] == 0 {
                    return Some("utf-16-be".to_string());
                }
                if c[1] == 0 && c[2] == 0 && c[3] == 0 {
                    return Some("utf-32-le".to_string());
                }
                if c[1] == 0 && c[3] == 0 {
                    return Some("utf-16-le".to_string());
                }
            }
        }

        Some(self.get_encoding_str(py))
    }

    #[getter]
    fn charset_encoding(&self, py: Python<'_>) -> Option<String> {
        if let Some(ct) = self
            .headers
            .bind(py)
            .borrow()
            .get_first_value("content-type")
        {
            for part in ct.split(';') {
                let part = part.trim();
                if let Some(charset) = part.strip_prefix("charset=") {
                    return Some(charset.trim_matches('"').trim().to_lowercase());
                }
            }
        }
        None
    }

    #[setter]
    fn set_encoding(&mut self, _py: Python<'_>, value: &str) -> PyResult<()> {
        if self
            .text_accessed
            .load(std::sync::atomic::Ordering::Relaxed)
        {
            return Err(pyo3::exceptions::PyValueError::new_err(
                "The encoding of the response cannot be changed once `.text` has been accessed.",
            ));
        }
        self.default_encoding_override = Some(value.to_string());
        Ok(())
    }

    #[getter]
    fn links(&self, py: Python<'_>) -> PyResult<Py<PyAny>> {
        let dict = PyDict::new(py);
        if let Some(link_header) = self.headers.bind(py).borrow().get_first_value("link") {
            for part in link_header.split(',') {
                let part = part.trim();
                if let Some(url_end) = part.find('>') {
                    if part.starts_with('<') {
                        let url = &part[1..url_end];
                        let rest = &part[url_end + 1..];
                        let link_dict = PyDict::new(py);
                        link_dict.set_item("url", url)?;
                        for param in rest.split(';') {
                            let param = param.trim();
                            if param.is_empty() {
                                continue;
                            }
                            if let Some(eq_pos) = param.find('=') {
                                let key = param[..eq_pos].trim();
                                let val = param[eq_pos + 1..]
                                    .trim()
                                    .trim_matches(|c| c == '"' || c == '\'');
                                link_dict.set_item(key, val)?;
                            }
                        }
                        if let Some(rel) = link_dict.get_item("rel")? {
                            let rel_str: String = rel.extract()?;
                            dict.set_item(rel_str, link_dict)?;
                        } else {
                            dict.set_item(url, link_dict)?;
                        }
                    }
                }
            }
        }
        Ok(dict.into())
    }

    #[getter]
    fn elapsed(&self, py: Python<'_>) -> PyResult<Py<PyAny>> {
        if let Some(secs) = self.elapsed {
            let datetime = py.import("datetime")?;
            let td = datetime.call_method1("timedelta", (secs,))?;
            Ok(td.into())
        } else {
            Err(pyo3::exceptions::PyRuntimeError::new_err(
                "'.elapsed' may only be accessed after the response has been read or closed.",
            ))
        }
    }

    #[getter]
    fn default_encoding_prop(&self, py: Python<'_>) -> String {
        self.get_encoding_str(py)
    }

    /// Resolve default_encoding: could be a string or a callable
    fn get_encoding_str(&self, py: Python<'_>) -> String {
        let de = self.default_encoding.bind(py);
        if de.is_callable() {
            if let Some(ref c) = self.content_bytes {
                if let Ok(result) = de.call1((PyBytes::new(py, c),)) {
                    if let Ok(s) = result.extract::<String>() {
                        return s;
                    }
                }
            }
            "utf-8".to_string()
        } else if let Ok(s) = de.extract::<String>() {
            s
        } else {
            "utf-8".to_string()
        }
    }

    /// Resolve the actual encoding to use for text decoding.
    /// Uses the encoding property but validates the result.
    pub(crate) fn resolve_encoding(&self, py: Python<'_>) -> String {
        if let Some(enc) = self.encoding(py) {
            if let Ok(codecs) = py.import("codecs") {
                if codecs.call_method1("lookup", (enc.as_str(),)).is_ok() {
                    return enc;
                }
            }
            "utf-8".to_string()
        } else {
            "utf-8".to_string()
        }
    }
}
