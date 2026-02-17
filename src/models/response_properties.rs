use pyo3::prelude::*;
use pyo3::types::{PyBytes, PyDict};

use super::cookies::Cookies;
use super::headers::Headers;
use super::request::Request;
use super::response::Response;

#[pymethods]
impl Response {
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
        let reason = self_.reason_phrase_impl();
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
}
