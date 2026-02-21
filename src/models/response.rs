use pyo3::prelude::*;
use pyo3::types::{PyBytes, PyDict};

use super::headers::{serde_value_to_py, simd_value_to_py, Headers};
use super::request::Request;

/// HTTP Response object.
#[pyclass(from_py_object, module = "httpxr._httpxr")]
pub struct Response {
    #[pyo3(get, set)]
    pub status_code: u16,
    pub headers: Option<Py<Headers>>,
    pub lazy_headers: Option<Vec<(Vec<u8>, Vec<u8>)>>,
    pub extensions: Option<Py<PyAny>>,
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
            headers: self.headers.as_ref().map(|h| h.clone_ref(py)),
            lazy_headers: self.lazy_headers.clone(),
            extensions: self.extensions.as_ref().map(|e| e.clone_ref(py)),
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
    /// Lazily initialize extensions dict if not already present.
    pub fn ensure_extensions(&mut self, py: Python<'_>) -> &Py<PyAny> {
        if self.extensions.is_none() {
            let ext = pyo3::types::PyDict::new(py);
            self.extensions = Some(ext.into_any().unbind());
        }
        self.extensions.as_ref().unwrap()
    }

    pub fn has_redirect_check(&mut self, py: Python<'_>) -> bool {
        (300..400).contains(&self.status_code)
            && self
                .headers(py)
                .unwrap()
                .bind(py)
                .borrow()
                .contains_header("location")
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
        headers: Option<Py<Headers>>,
        extensions: Option<Py<PyAny>>,
        request: Option<Request>,
        content_bytes: Option<Vec<u8>>,
    ) -> Self {
        Python::attach(|py| Response {
            status_code,
            headers,
            lazy_headers: None,
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
                _ => result,
            };
        }

        Ok(result)
    }

    pub(crate) fn text_impl(&mut self, py: Python<'_>) -> PyResult<String> {
        self.text_accessed
            .store(true, std::sync::atomic::Ordering::Relaxed);
        let encoding_name = self.resolve_encoding(py);
        if let Some(ref c) = self.content_bytes {
            if c.is_empty() {
                return Ok(String::new());
            }
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
            let hdrs_py = self.headers(py)?;
            let hdrs_ref = hdrs_py.borrow(py);
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
        crate::status_codes::reason_phrase_for_code(self.status_code).to_string()
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
            headers: Some(hdrs_py),
            lazy_headers: None,
            extensions: Some(ext),
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
    fn text(&mut self, py: Python<'_>) -> PyResult<String> {
        self.text_impl(py)
    }

    fn __eq__(&self, other: &Response) -> bool {
        self.status_code == other.status_code
            && self.get_method_str() == other.get_method_str()
            && self.get_url_str() == other.get_url_str()
            && self.content_bytes.as_ref().map(|c| c.len())
                == other.content_bytes.as_ref().map(|c| c.len())
    }

    #[pyo3(signature = (**kwargs))]
    fn json(&mut self, py: Python<'_>, kwargs: Option<&Bound<'_, PyDict>>) -> PyResult<Py<PyAny>> {
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

    /// Return the raw JSON content bytes without Python string decoding.
    /// This allows combining with orjson.loads() for maximum throughput.
    /// This is an httpxr extension — not available in httpx.
    fn json_bytes<'py>(&self, py: Python<'py>) -> PyResult<Bound<'py, PyBytes>> {
        let content = self.content_bytes.as_ref().ok_or_else(|| {
            crate::exceptions::ResponseNotRead::new_err(
                "Attempted to access json_bytes, without having called `read()`.",
            )
        })?;
        Ok(PyBytes::new(py, content))
    }

    /// Parse each line of the response body as JSON (NDJSON / JSON Lines).
    /// Returns a list of parsed Python objects.
    /// This is an httpxr extension — not available in httpx.
    fn iter_json(&self, py: Python<'_>) -> PyResult<Py<pyo3::types::PyList>> {
        let content = self.content_bytes.as_ref().ok_or_else(|| {
            crate::exceptions::ResponseNotRead::new_err(
                "Attempted to access iter_json, without having called `read()`.",
            )
        })?;
        let text = String::from_utf8_lossy(content);
        let result = pyo3::types::PyList::empty(py);
        for line in text.lines() {
            let trimmed = line.trim();
            if trimmed.is_empty() {
                continue;
            }
            // Skip SSE event prefixes
            if trimmed.starts_with("event:")
                || trimmed.starts_with("id:")
                || trimmed.starts_with("retry:")
            {
                continue;
            }
            let json_str = if let Some(stripped) = trimmed.strip_prefix("data:") {
                stripped.trim()
            } else {
                trimmed
            };
            if json_str.is_empty() || json_str == "[DONE]" {
                continue;
            }
            match serde_json::from_str::<serde_json::Value>(json_str) {
                Ok(val) => {
                    result.append(serde_value_to_py(py, &val)?)?;
                }
                Err(_) => {
                    let json_mod = py.import("json")?;
                    result.append(json_mod.call_method1("loads", (json_str,))?)?;
                }
            }
        }
        Ok(result.unbind())
    }

    #[getter]
    fn get_request(&mut self, py: Python<'_>) -> PyResult<Request> {
        if self.request.is_none() {
            if let (Some(method), Some(url_str)) = (
                self.lazy_request_method.take(),
                self.lazy_request_url.take(),
            ) {
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
}
