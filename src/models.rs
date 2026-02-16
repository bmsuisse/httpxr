use pyo3::prelude::*;
use pyo3::types::{PyBool, PyBytes, PyDict, PyFloat, PyList, PyString, PyTuple};

/// Convert a simd_json::OwnedValue directly to a Python object.
/// This is the fast path — no intermediate serde_json conversion.
fn simd_value_to_py(py: Python<'_>, val: &simd_json::OwnedValue) -> PyResult<Py<PyAny>> {
    use simd_json::OwnedValue;
    match val {
        OwnedValue::Static(s) => match s {
            simd_json::StaticNode::Null => Ok(py.None()),
            simd_json::StaticNode::Bool(b) => {
                Ok(PyBool::new(py, *b).to_owned().into_any().unbind())
            }
            simd_json::StaticNode::I64(i) => {
                Ok((*i).into_pyobject(py).unwrap().into_any().unbind())
            }
            simd_json::StaticNode::U64(u) => {
                Ok((*u).into_pyobject(py).unwrap().into_any().unbind())
            }
            simd_json::StaticNode::F64(f) => {
                Ok(PyFloat::new(py, *f).into_any().unbind())
            }
            #[allow(unreachable_patterns)]
            _ => Ok(py.None()),
        },
        OwnedValue::String(s) => Ok(PyString::new(py, s).into_any().unbind()),
        OwnedValue::Array(arr) => {
            let items: Vec<Py<PyAny>> = arr
                .iter()
                .map(|v| simd_value_to_py(py, v))
                .collect::<PyResult<_>>()?;
            Ok(PyList::new(py, &items)?.into_any().unbind())
        }
        OwnedValue::Object(map) => {
            let dict = PyDict::new(py);
            for (k, v) in map.iter() {
                let key: &str = k.as_ref();
                dict.set_item(key, simd_value_to_py(py, v)?)?;
            }
            Ok(dict.into_any().unbind())
        }
    }
}


/// Case-insensitive HTTP headers multidict.
#[pyclass(from_py_object)]
#[derive(Clone, Debug)]
pub struct Headers {
    pub raw: Vec<(Vec<u8>, Vec<u8>)>,
    pub encoding: String,
}

impl Headers {
    pub fn to_header_bytes_from(value: &Bound<'_, PyAny>) -> PyResult<Vec<u8>> {
        if let Ok(b) = value.cast::<PyBytes>() {
            Ok(b.as_bytes().to_vec())
        } else if let Ok(s) = value.cast::<PyString>() {
            Ok(s.to_str()?.as_bytes().to_vec())
        } else {
            Ok(value.str()?.to_str()?.as_bytes().to_vec())
        }
    }

    fn decode_value(&self, bytes: &[u8]) -> String {
        match self.encoding.as_str() {
            "ascii" => String::from_utf8_lossy(bytes).to_string(),
            "utf-8" | "utf8" => String::from_utf8_lossy(bytes).to_string(),
            "iso-8859-1" | "latin-1" | "latin1" => bytes.iter().map(|&b| b as char).collect(),
            _ => String::from_utf8_lossy(bytes).to_string(),
        }
    }

    pub fn get_first_value(&self, key: &str) -> Option<String> {
        let key_lower = key.to_lowercase();
        let values: Vec<String> = self
            .raw
            .iter()
            .filter(|(k, _)| String::from_utf8_lossy(k).to_lowercase() == key_lower)
            .map(|(_, v)| self.decode_value(v))
            .collect();
        if values.is_empty() {
            None
        } else if values.len() == 1 {
            Some(values[0].clone())
        } else {
            Some(values.join(", "))
        }
    }

    pub fn get_multi_items(&self) -> Vec<(String, String)> {
        self.raw
            .iter()
            .map(|(k, v)| {
                (
                    String::from_utf8_lossy(k).to_lowercase(),
                    self.decode_value(v),
                )
            })
            .collect()
    }

    /// Return raw header byte pairs (owned) — no String conversion, no lowercasing.
    /// Cheaper than `get_multi_items()` since it skips UTF-8 decode and `.to_lowercase()`.
    pub fn get_raw_items_owned(&self) -> Vec<(Vec<u8>, Vec<u8>)> {
        self.raw.clone()
    }

    pub fn set_header(&mut self, key: &str, value: &str) {
        let key_lower = key.to_lowercase();
        self.raw
            .retain(|(k, _)| String::from_utf8_lossy(k).to_lowercase() != key_lower);
        self.raw
            .push((key.as_bytes().to_vec(), value.as_bytes().to_vec()));
    }

    /// Construct Headers directly from string pairs — O(n) with no deduplication.
    /// Use this when building response headers from scratch (e.g. from reqwest).
    pub fn from_raw_pairs(pairs: Vec<(String, String)>) -> Self {
        let raw: Vec<(Vec<u8>, Vec<u8>)> = pairs
            .into_iter()
            .map(|(k, v)| (k.into_bytes(), v.into_bytes()))
            .collect();
        let encoding = Self::detect_encoding(&raw, "utf-8");
        Headers { raw, encoding }
    }

    /// Construct Headers directly from byte pairs — zero conversion.
    /// Use when response headers are already in byte form.
    pub fn from_raw_byte_pairs(raw: Vec<(Vec<u8>, Vec<u8>)>) -> Self {
        let encoding = Self::detect_encoding(&raw, "utf-8");
        Headers { raw, encoding }
    }

    pub fn contains_header(&self, key: &str) -> bool {
        let key_lower = key.to_lowercase();
        self.raw
            .iter()
            .any(|(k, _)| String::from_utf8_lossy(k).to_lowercase() == key_lower)
    }

    pub fn update_from(&mut self, headers: Option<&Bound<'_, PyAny>>) -> PyResult<()> {
        if let Some(h) = headers {
            let new_headers = Headers::create(Some(h), &self.encoding)?;
            // Collect unique keys to remove from existing headers
            let mut keys_to_remove: Vec<String> = Vec::new();
            for (key, _) in &new_headers.raw {
                let key_lower = String::from_utf8_lossy(key).to_lowercase();
                if !keys_to_remove.contains(&key_lower) {
                    keys_to_remove.push(key_lower);
                }
            }
            // Remove all existing entries for those keys
            self.raw.retain(|(k, _)| {
                let kl = String::from_utf8_lossy(k).to_lowercase();
                !keys_to_remove.contains(&kl)
            });
            // Append all new entries (preserving duplicates)
            for (key, value) in &new_headers.raw {
                self.raw.push((key.clone(), value.clone()));
            }
        }
        Ok(())
    }

    pub fn remove_header(&mut self, key: &str) {
        let key_lower = key.to_lowercase();
        self.raw
            .retain(|(k, _)| String::from_utf8_lossy(k).to_lowercase() != key_lower);
    }

    pub fn new_empty() -> Self {
        Headers {
            raw: Vec::new(),
            encoding: "utf-8".to_string(),
        }
    }

    pub fn iter_all(&self) -> Vec<(String, String)> {
        self.raw
            .iter()
            .map(|(k, v)| (String::from_utf8_lossy(k).to_string(), self.decode_value(v)))
            .collect()
    }

    pub fn append_header(&mut self, key: &str, value: &str) {
        self.raw
            .push((key.as_bytes().to_vec(), value.as_bytes().to_vec()));
    }

    fn validate_header_value(v: &Bound<'_, PyAny>) -> PyResult<Vec<u8>> {
        if v.is_none() {
            return Err(pyo3::exceptions::PyTypeError::new_err(format!(
                "Header value must be str or bytes, not {}",
                v.get_type()
            )));
        }
        Self::to_header_bytes_from(v)
    }

    pub fn create(headers: Option<&Bound<'_, PyAny>>, encoding: &str) -> PyResult<Self> {
        let mut raw = Vec::new();
        if let Some(h) = headers {
            if h.is_none() {
                // None passed
            } else if let Ok(existing) = h.extract::<Headers>() {
                raw = existing.raw;
            } else if let Ok(d) = h.cast::<PyDict>() {
                for (k, v) in d.iter() {
                    let key_bytes = Self::to_header_bytes_from(&k)?;
                    let val_bytes = Self::validate_header_value(&v)?;
                    raw.push((key_bytes, val_bytes));
                }
            } else if let Ok(l) = h.cast::<PyList>() {
                for item in l.iter() {
                    let tuple = item.cast::<PyTuple>()?;
                    let key_bytes = Self::to_header_bytes_from(&tuple.get_item(0)?)?;
                    let val_bytes = Self::validate_header_value(&tuple.get_item(1)?)?;
                    raw.push((key_bytes, val_bytes));
                }
            } else if let Ok(items) = h.call_method0("items") {
                for item in items.try_iter()? {
                    let item: Bound<'_, PyAny> = item?;
                    let tuple = item.cast::<PyTuple>()?;
                    let key_bytes = Self::to_header_bytes_from(&tuple.get_item(0)?)?;
                    let val_bytes = Self::validate_header_value(&tuple.get_item(1)?)?;
                    raw.push((key_bytes, val_bytes));
                }
            }
        }
        let detected_encoding = Self::detect_encoding(&raw, encoding);
        Ok(Headers {
            raw,
            encoding: detected_encoding,
        })
    }

    fn detect_encoding(raw: &[(Vec<u8>, Vec<u8>)], hint: &str) -> String {
        // If user explicitly specified something other than default, use it
        if hint != "utf-8" {
            return hint.to_string();
        }
        // Check all values: if all are pure ASCII, use ascii
        let all_ascii = raw.iter().all(|(_, v)| v.iter().all(|b| b.is_ascii()));
        if all_ascii {
            return "ascii".to_string();
        }
        // Check if valid UTF-8
        let all_utf8 = raw.iter().all(|(_, v)| std::str::from_utf8(v).is_ok());
        if all_utf8 {
            return "utf-8".to_string();
        }
        // Fall back to iso-8859-1
        "iso-8859-1".to_string()
    }
}

#[pymethods]
impl Headers {
    #[new]
    #[pyo3(signature = (headers=None, encoding="utf-8"))]
    fn new(headers: Option<&Bound<'_, PyAny>>, encoding: &str) -> PyResult<Self> {
        Self::create(headers, encoding)
    }

    fn keys(&self) -> Vec<String> {
        let mut seen = Vec::new();
        for (k, _) in &self.raw {
            let key = String::from_utf8_lossy(k).to_lowercase();
            if !seen.contains(&key) {
                seen.push(key);
            }
        }
        seen
    }

    fn values(&self) -> Vec<String> {
        self.keys()
            .iter()
            .filter_map(|k| self.get_first_value(k))
            .collect()
    }

    #[pyo3(name = "items")]
    fn py_items(&self) -> Vec<(String, String)> {
        self.keys()
            .iter()
            .filter_map(|k| self.get_first_value(k).map(|v| (k.clone(), v)))
            .collect()
    }

    fn multi_items(&self) -> Vec<(String, String)> {
        self.get_multi_items()
    }

    #[pyo3(signature = (key, default=None))]
    pub fn get(&self, key: &str, default: Option<&str>) -> Option<String> {
        self.get_first_value(key)
            .or_else(|| default.map(|s| s.to_string()))
    }

    #[pyo3(signature = (key, split_commas=false))]
    fn get_list(&self, key: &str, split_commas: bool) -> Vec<String> {
        let key_lower = key.to_lowercase();
        let values: Vec<String> = self
            .raw
            .iter()
            .filter(|(k, _)| String::from_utf8_lossy(k).to_lowercase() == key_lower)
            .map(|(_, v)| String::from_utf8_lossy(v).to_string())
            .collect();
        if split_commas {
            values
                .iter()
                .flat_map(|v| v.split(',').map(|s| s.trim().to_string()))
                .collect()
        } else {
            values
        }
    }

    fn update(&mut self, headers: Option<&Bound<'_, PyAny>>) -> PyResult<()> {
        self.update_from(headers)
    }

    fn copy(&self) -> Self {
        self.clone()
    }

    #[getter]
    fn raw(&self, py: Python<'_>) -> PyResult<Py<PyAny>> {
        let list = PyList::empty(py);
        for (k, v) in &self.raw {
            let kb = PyBytes::new(py, k);
            let vb = PyBytes::new(py, v);
            let tuple = (kb, vb).into_pyobject(py)?;
            list.append(tuple)?;
        }
        Ok(list.into())
    }

    #[getter]
    fn encoding(&self) -> &str {
        &self.encoding
    }

    #[setter]
    fn set_encoding(&mut self, value: &str) {
        self.encoding = value.to_string();
    }

    fn __getitem__(&self, key: &str) -> PyResult<String> {
        self.get_first_value(key)
            .ok_or_else(|| pyo3::exceptions::PyKeyError::new_err(key.to_string()))
    }

    fn __setitem__(&mut self, key: &str, value: &str) {
        let key_lower = key.to_lowercase();
        let mut first_idx = None;
        for (i, (k, _)) in self.raw.iter().enumerate() {
            if String::from_utf8_lossy(k).to_lowercase() == key_lower {
                if first_idx.is_none() {
                    first_idx = Some(i);
                }
            }
        }
        self.raw
            .retain(|(k, _)| String::from_utf8_lossy(k).to_lowercase() != key_lower);
        if let Some(idx) = first_idx {
            let insert_at = idx.min(self.raw.len());
            self.raw.insert(
                insert_at,
                (key_lower.into_bytes(), value.as_bytes().to_vec()),
            );
        } else {
            self.raw
                .push((key_lower.into_bytes(), value.as_bytes().to_vec()));
        }
    }
    fn __delitem__(&mut self, key: &str) -> PyResult<()> {
        let key_lower = key.to_lowercase();
        let before_len = self.raw.len();
        self.raw
            .retain(|(k, _)| String::from_utf8_lossy(k).to_lowercase() != key_lower);
        if self.raw.len() == before_len {
            return Err(pyo3::exceptions::PyKeyError::new_err(key.to_string()));
        }
        Ok(())
    }
    fn __contains__(&self, key: &str) -> bool {
        self.contains_header(key)
    }
    fn __iter__(&self, py: Python<'_>) -> PyResult<Py<PyAny>> {
        let keys = self.keys();
        let list = PyList::new(py, &keys)?;
        Ok(list.call_method0("__iter__")?.into())
    }
    fn __len__(&self) -> usize {
        self.keys().len()
    }
    fn __bool__(&self) -> bool {
        !self.raw.is_empty()
    }

    #[pyo3(signature = (key, default=""))]
    fn setdefault(&mut self, key: &str, default: &str) -> String {
        if let Some(val) = self.get_first_value(key) {
            val
        } else {
            self.raw
                .push((key.to_lowercase().into_bytes(), default.as_bytes().to_vec()));
            default.to_string()
        }
    }

    fn __eq__(&self, _py: Python<'_>, other: &Bound<'_, PyAny>) -> bool {
        if let Ok(other_headers) = other.extract::<Headers>() {
            let mut s: Vec<(String, String)> = self
                .get_multi_items()
                .into_iter()
                .map(|(k, v)| (k.to_lowercase(), v))
                .collect();
            let mut o: Vec<(String, String)> = other_headers
                .get_multi_items()
                .into_iter()
                .map(|(k, v)| (k.to_lowercase(), v))
                .collect();
            s.sort();
            o.sort();
            s == o
        } else if let Ok(d) = other.cast::<pyo3::types::PyDict>() {
            // Dict comparison: treat each key-value pair as a header entry, case-insensitive sorted
            let mut s: Vec<(String, String)> = self
                .get_multi_items()
                .into_iter()
                .map(|(k, v)| (k.to_lowercase(), v))
                .collect();
            s.sort();
            let mut o: Vec<(String, String)> = Vec::new();
            for (k, v) in d.iter() {
                let key = k
                    .str()
                    .ok()
                    .map(|s| s.to_string().to_lowercase())
                    .unwrap_or_default();
                let val = v.str().ok().map(|s| s.to_string()).unwrap_or_default();
                o.push((key, val));
            }
            o.sort();
            s == o
        } else if let Ok(list) = other.cast::<pyo3::types::PyList>() {
            // Compare with list of tuples - case-insensitive sorted comparison
            let mut s: Vec<(String, String)> = self
                .get_multi_items()
                .into_iter()
                .map(|(k, v)| (k.to_lowercase(), v))
                .collect();
            s.sort();
            let mut o: Vec<(String, String)> = Vec::new();
            for item in list.iter() {
                if let Ok(tuple) = item.cast::<pyo3::types::PyTuple>() {
                    if tuple.len() == 2 {
                        let k: String = tuple
                            .get_item(0)
                            .ok()
                            .and_then(|v| v.extract().ok())
                            .unwrap_or_default();
                        let v: String = tuple
                            .get_item(1)
                            .ok()
                            .and_then(|v| v.extract().ok())
                            .unwrap_or_default();
                        o.push((k.to_lowercase(), v));
                    }
                }
            }
            o.sort();
            s == o
        } else {
            false
        }
    }

    fn __repr__(&self) -> String {
        let sensitive = ["authorization", "proxy-authorization"];
        let multi = self.get_multi_items();
        if multi.is_empty() {
            return "Headers({})".to_string();
        }

        // Check if there are duplicate keys
        let keys: Vec<&str> = multi.iter().map(|(k, _)| k.as_str()).collect();
        let has_dups = {
            let mut seen = std::collections::HashSet::new();
            keys.iter().any(|k| !seen.insert(*k))
        };

        // Obfuscate sensitive values
        let apply_secure = |k: &str, v: &str| -> String {
            if sensitive.contains(&k) {
                "[secure]".to_string()
            } else {
                v.to_string()
            }
        };

        let encoding_suffix = if self.encoding != "ascii" {
            // Check if any value has non-ASCII bytes
            let has_non_ascii = self
                .raw
                .iter()
                .any(|(_, v)| v.iter().any(|b| !b.is_ascii()));
            if has_non_ascii {
                format!(", encoding='{}'", self.encoding)
            } else {
                String::new()
            }
        } else {
            String::new()
        };

        if has_dups {
            // List format: Headers([('k', 'v'), ...])
            let parts: Vec<String> = multi
                .iter()
                .map(|(k, v)| format!("('{}', '{}')", k, apply_secure(k, v)))
                .collect();
            format!("Headers([{}]{})", parts.join(", "), encoding_suffix)
        } else {
            // Dict format: Headers({'k': 'v', ...})
            let parts: Vec<String> = multi
                .iter()
                .map(|(k, v)| format!("'{}': '{}'", k, apply_secure(k, v)))
                .collect();
            format!("Headers({{{}}}{})", parts.join(", "), encoding_suffix)
        }
    }
}

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
            // Check if value is a list - expand each element
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
#[pyclass(from_py_object, module = "httpr._httpr")]
pub struct Request {
    #[pyo3(get, set)]
    pub method: String,
    #[pyo3(get, set)]
    pub url: crate::urls::URL,
    #[pyo3(get, set)]
    pub headers: Py<Headers>,
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
                let qp = crate::urls::QueryParams::create(Some(p))?;
                let qs = qp.encode();
                // Rebuild URL: if qs is empty, strip query; else set new query
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

        // Handle data= as deprecated alias for content= when it's not a dict
        let (actual_content, used_data_as_content) = if content.is_some() {
            (content, false)
        } else if let Some(d) = data {
            if !d.is_none() && !d.is_instance_of::<pyo3::types::PyDict>() {
                // data= with non-dict type is deprecated, use content= instead
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
                // Dicts are not valid content - they should be passed as data= or json=
                return Err(pyo3::exceptions::PyTypeError::new_err(format!(
                    "Unexpected type for 'content': {}",
                    c.get_type().name()?
                )));
            } else if c.hasattr("read").unwrap_or(false) && !c.hasattr("__aiter__").unwrap_or(false)
            {
                // File-like object (e.g. BytesIO) - read eagerly and wrap in sync-only stream
                let data_bytes: Vec<u8> = c.call_method0("read")?.extract()?;
                // Create a sync-only iterator wrapper (not AsyncIterable)
                let py_bytes = PyBytes::new(py, &data_bytes);
                let list = pyo3::types::PyList::new(py, &[py_bytes.into_any()])?;
                let iter = list.call_method0("__iter__")?;
                let wrapper = crate::types::ResponseIteratorStream::new(iter.unbind());
                let wrapper_py = Py::new(py, wrapper)?;
                stream_obj = Some(wrapper_py.into_any());
                // Set Content-Length for the known size
                if !hdrs.contains_header("content-length")
                    && !hdrs.contains_header("transfer-encoding")
                {
                    hdrs.set_header("content-length", &data_bytes.len().to_string());
                }
                None
            } else {
                // Check if it's an async iterator (has __aiter__)
                let has_aiter = c.hasattr("__aiter__").unwrap_or(false);
                let has_iter = c.hasattr("__iter__").unwrap_or(false);
                // Also check if it is a generator or iterator type but not bytes/str
                let is_iterator = has_iter
                    && !c.is_instance_of::<pyo3::types::PyBytes>()
                    && !c.is_instance_of::<pyo3::types::PyString>();

                if has_aiter && !is_iterator {
                    // Pure async iterator - wrap in consumption-tracking wrapper
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
                    // Sync iterator - wrap in consumption-tracking wrapper
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
                    // Not bytes, string, or iterable - raise TypeError
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
            // If files are provided, use multipart/form-data encoding
            // Check if files dict/list is empty
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
                // Fallback to data handling if files is explicitly None/empty
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

        // Set content-type for form data if we have encoded data
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
            // Stream exists but hasn't been read
            Err(crate::exceptions::RequestNotRead::new_err(
                "Attempted to access streaming request content without having called `.read()`.",
            ))
        } else {
            Ok(PyBytes::new(py, b"").into())
        }
    }
    #[getter]
    fn stream(&self, py: Python<'_>) -> PyResult<Py<PyAny>> {
        // If we have an original stream object, return it directly
        if let Some(ref s) = self.stream {
            return Ok(s.clone_ref(py));
        }
        // Otherwise wrap content_body in a ByteStream
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
        // Try to consume the stream
        if let Some(ref stream) = self.stream {
            let stream_bound = stream.bind(py);
            let mut buf = Vec::new();
            // Call __iter__ and propagate any errors (e.g. StreamClosed)
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
        // If content already read, return it
        if let Some(ref c) = self_mut.content_body {
            let content = c.clone();
            return pyo3_async_runtimes::tokio::future_into_py(py, async move { Ok(content) });
        }
        // Try to consume the stream (sync or async)
        if let Some(ref stream) = self_mut.stream {
            let stream_bound = stream.bind(py);
            // Try sync iteration first
            let has_iter = stream_bound.hasattr("__iter__").unwrap_or(false);
            let has_aiter = stream_bound.hasattr("__aiter__").unwrap_or(false);
            if has_iter && !has_aiter {
                // Pure sync iterator
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
            // For async streams, create a Python coroutine that stores result back on this request
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
        let cls = py.import("httpr")?.getattr("Request")?;
        // Build args: (method, url_string)
        let args = pyo3::types::PyTuple::new(
            py,
            &[
                self.method.clone().into_pyobject(py)?.into_any(),
                self.url.to_string().into_pyobject(py)?.into_any(),
            ],
        )?;
        // Build state dict with headers and content
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
        // Track whether the request had an unread stream
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
        // If the original request had an unread stream, set a closed stream marker
        // so that .content raises RequestNotRead and .read()/.aread() raises StreamClosed
        if let Some(had_stream) = dict.get_item("had_stream")? {
            if had_stream.extract::<bool>().unwrap_or(false) && self.content_body.is_none() {
                // Create a ClosedStream marker that raises StreamClosed on iteration
                let ns = pyo3::types::PyDict::new(py);
                py.run(c"class _ClosedStream:\n    def __iter__(self): raise __import__('httpr').StreamClosed(self)\n    def __aiter__(self): return self\n    async def __anext__(self): raise __import__('httpr').StreamClosed(self)\n_result = _ClosedStream()\n", None, Some(&ns))?;
                let closed_stream = ns.get_item("_result")?.unwrap();
                self.stream = Some(closed_stream.unbind());
            }
        }
        Ok(())
    }
}

/// HTTP Response object.
#[pyclass(from_py_object, module = "httpr._httpr")]
pub struct Response {
    #[pyo3(get, set)]
    pub status_code: u16,
    #[pyo3(get, set)]
    pub headers: Py<Headers>,
    pub extensions: Py<PyAny>,
    pub request: Option<Request>,
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

        // Parse chained encodings (applied in reverse order for decoding)
        let mut encodings: Vec<&str> = encoding.split(',').map(|s| s.trim()).collect();
        // Encodings are listed in the order they were applied, so we decode in reverse
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
                    // Try zlib-wrapped first
                    let mut decoder = flate2::read::ZlibDecoder::new(&result[..]);
                    let mut decoded = Vec::new();
                    if decoder.read_to_end(&mut decoded).is_ok() {
                        decoded
                    } else {
                        // Try raw deflate
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
                    // Handle multiframe zstd: decompress frame by frame
                    let mut output = Vec::new();
                    let mut cursor = std::io::Cursor::new(&result);
                    loop {
                        let pos = cursor.position() as usize;
                        if pos >= result.len() {
                            break;
                        }
                        let remaining = &result[pos..];
                        // Check for skippable frame (magic 0x184D2A50..0x184D2A5F)
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
                                // Try frame-by-frame with a limited reader
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
                    // Unknown encoding, return as-is
                    result
                }
            };
        }

        Ok(result)
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

        // Check if content is a sync/async iterator (streaming body)
        let (content_bytes, stream_obj) = if let Some(c) = content {
            if let Ok(b) = c.cast::<PyBytes>() {
                (Some(b.as_bytes().to_vec()), None)
            } else if let Ok(s) = c.extract::<String>() {
                (Some(s.into_bytes()), None)
            } else {
                // Check if it's an iterator or async iterator
                let has_aiter = c.hasattr("__aiter__").unwrap_or(false);
                let has_iter = c.hasattr("__iter__").unwrap_or(false);
                if has_aiter && !has_iter {
                    // Pure async iterator - wrap in ResponseAsyncIteratorStream
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
                    // Sync iterator - wrap in ResponseIteratorStream
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

        // Merge explicit stream kwarg with content-derived stream
        let final_stream = stream.or(stream_obj);
        let _is_streaming = final_stream.is_some();

        // If no content and no stream, default content_bytes to empty and mark closed
        let (final_content, final_closed) = if content_bytes.is_none() && final_stream.is_none() {
            (Some(Vec::new()), true)
        } else if content_bytes.is_some() {
            (content_bytes, true) // Non-streaming responses are immediately closed
        } else {
            (None, false) // Streaming responses start open
        };

        // Decompress content if Content-Encoding header is present
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
        // Non-streaming responses with content are already "consumed"
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
        // If we have an original stream object, return it directly
        if let Some(ref s) = self.stream {
            if self.is_stream_consumed {
                return Err(crate::exceptions::StreamConsumed::new_err(
                    "Attempted to read or stream content, but the stream has already been consumed."
                ));
            }
            return Ok(s.clone_ref(py));
        }
        // Return a ByteStream wrapping the content (or empty)
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

    fn __eq__(&self, other: &Response) -> bool {
        self.status_code == other.status_code
            // Check request equality (method/url)
            && self.request.as_ref().map(|r| r.method.clone()) == other.request.as_ref().map(|r| r.method.clone())
            && self.request.as_ref().map(|r| r.url.to_string()) == other.request.as_ref().map(|r| r.url.to_string())
            // Ideally check headers/content too, but for history tracking, this might be enough
            // Check content length to distinguish different responses?
            && self.content_bytes.as_ref().map(|c| c.len()) == other.content_bytes.as_ref().map(|c| c.len())
    }

    #[pyo3(signature = (**kwargs))]
    fn json(&self, py: Python<'_>, kwargs: Option<&Bound<'_, PyDict>>) -> PyResult<Py<PyAny>> {
        let content = self.content_bytes.as_ref().ok_or_else(|| {
            crate::exceptions::ResponseNotRead::new_err(
                "Attempted to access json, without having called `read()`.",
            )
        })?;

        // If kwargs were provided, fall back to Python json.loads
        if kwargs.is_some() {
            let text = self.text(py)?;
            let json_mod = py.import("json")?;
            let loads = json_mod.getattr("loads")?;
            let py_text = text.into_pyobject(py).unwrap().into_any();
            let args = (py_text,);
            return Ok(loads.call(args, kwargs)?.into());
        }

        // Use simd-json for SIMD-accelerated parsing directly on raw bytes
        // We clone because simd_json modifies the buffer in-place
        let mut buf = content.clone();
        match simd_json::to_owned_value(&mut buf) {
            Ok(val) => simd_value_to_py(py, &val),
            Err(_) => {
                // If simd-json fails (e.g. not UTF-8 or other issues), try Python's json.loads
                // ensuring we decode text first
                let text = self.text(py)?;
                let json_mod = py.import("json")?;
                Ok(json_mod.call_method1("loads", (text,))?.into())
            }
        }
    }

    #[getter]
    fn get_request(&self) -> PyResult<Request> {
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
        // If the stream was consumed via streaming iteration (iter_bytes, iter_raw),
        // raise StreamConsumed even though we have cached content.
        if self.was_streaming && self.is_stream_consumed && self.stream.is_none() {
            return Err(crate::exceptions::StreamConsumed::new_err(
                "Attempted to read or stream content, but the stream has already been consumed.",
            ));
        }
        // Already have content bytes (non-streaming or read()), return them
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
        // Consume sync iterator stream
        if let Some(stream_py) = self.stream.take() {
            // Take ownership of stream
            let mut buf = Vec::new();
            let s = stream_py.bind(py);
            // Check if it's an async iterator - if so, raise RuntimeError
            if s.hasattr("__aiter__")? && !s.hasattr("__iter__")? {
                return Err(pyo3::exceptions::PyRuntimeError::new_err(
                    "Attempted to read an asynchronous response using 'response.read()'. Use 'await response.aread()' instead."));
            }
            // Call __iter__ to get the iterator (needed for ResponseIteratorStream)
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
            // Apply decompression if Content-Encoding header is present
            let hdrs_ref = self.headers.borrow(py);
            let buf = Response::decompress_content(py, &buf, &hdrs_ref)?;
            self.content_bytes = Some(buf.clone());
            self.is_stream_consumed = true;
            self.is_closed_flag = true;
            self.num_bytes_downloaded_counter
                .store(buf.len(), std::sync::atomic::Ordering::Relaxed);
            // self.stream is already None due to .take()
            return Ok(buf);
        }
        // No stream, no content - return empty
        self.content_bytes = Some(Vec::new());
        self.is_closed_flag = true;
        Ok(Vec::new())
    }

    fn close(&mut self, py: Python<'_>) -> PyResult<()> {
        if let Some(ref stream) = self.stream {
            let s = stream.bind(py);
            // Async generators have __aiter__ but not __iter__
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
        (200..300).contains(&self.status_code)
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
            // Calculate new URL
            let new_url = req.url.join_relative(&location)?;

            // Determine method and body
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

            // Clone headers if preserved, otherwise create new
            let new_headers = if let Some(h) = headers {
                h
            } else {
                req.headers.clone_ref(py)
            };

            if method == "GET" && req.method != "GET" {
                // Strip content headers
                let mut h = new_headers.bind(py).borrow_mut();
                h.remove_header("content-length");
                h.remove_header("content-type");
                h.remove_header("transfer-encoding");
                drop(h);
            }

            // We don't handle cookie persistence here (requires client cookie jar).
            // httpx next_request usually just returns the request object.

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
        crate::status_codes::status_code_map()
            .get(&self.status_code)
            .unwrap_or(&"")
            .to_string()
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
    fn extensions(&self, py: Python<'_>) -> Py<PyAny> {
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

        // If content was already read (e.g. by a previous aread()), return it immediately
        if let Some(bytes) = content_bytes {
            return pyo3_async_runtimes::tokio::future_into_py(py, async move { Ok(bytes) });
        }

        // If stream was consumed via async iteration, raise StreamConsumed
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

        // Try sync iteration first (works for sync generators used in tests)
        if let Some(stream_py) = stream_ref {
            // Take ownership of stream_ref
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
                // Apply decompression if Content-Encoding header is present
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

        // For async streams, delegate entirely to Python to properly handle
        // BaseException types (KeyboardInterrupt, SystemExit, asyncio.CancelledError)
        // that cannot be reliably caught through pyo3_async_runtimes.
        let slf_bound = slf.bind(py);
        {
            let mut self_mut = slf_bound.borrow_mut();
            if self_mut.stream.is_none() {
                // No stream — return empty content
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
        // Define a Python coroutine that does the async iteration.
        // For direct callers (user code), re-raising exceptions is fine since
        // they're not running inside pyo3's async bridge.
        // The eager-read path in AsyncClient.send does its own iteration
        // with special no-re-raise handling for KeyboardInterrupt.
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
        let self_ = slf.borrow();
        let py = slf.py();
        if let None = &self_.request {
            return Err(pyo3::exceptions::PyRuntimeError::new_err(
                "Cannot call `raise_for_status` as the request instance has not been set on this response."
            ));
        }
        // Helper to raise error with attrs
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
        } else {
            Ok(crate::urls::URL::create_from_str("")?)
        }
    }

    #[getter]
    fn encoding(&self, py: Python<'_>) -> Option<String> {
        // If encoding was explicitly set, return that
        if let Some(ref enc) = self.default_encoding_override {
            return Some(enc.clone());
        }
        // Validate charset is a valid Python codec, fall back to default encoding if not
        if let Some(ct) = self
            .headers
            .bind(py)
            .borrow()
            .get_first_value("content-type")
        {
            if let Some(idx) = ct.to_lowercase().find("charset=") {
                let charset = &ct[idx + 8..];
                let charset = charset.split(';').next().unwrap_or(charset).trim();
                // Validate with codecs.lookup
                let codecs = py.import("codecs").ok();
                let is_valid = codecs
                    .as_ref()
                    .map(|c| c.call_method1("lookup", (charset,)).is_ok())
                    .unwrap_or(false);
                if is_valid {
                    return Some(charset.to_string());
                }
                // Invalid codec, fall back to default encoding
                return Some(self.get_encoding_str(py));
            }
        }
        // Default to utf-8 if content has been read
        // But first check for BOMs to support autodetect
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

            // JSON encoding sniffing (RFC 4627)
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
        // Return charset from Content-Type header, if present
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
        // In httpx, setting encoding after text has been accessed always raises ValueError
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
            // Parse Link header: <url>; rel="name", <url2>; rel="name2"
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
                        // Use rel as key if available, otherwise use url
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
            // Call the callable with the content bytes
            if let Some(ref c) = self.content_bytes {
                if let Ok(result) = de.call1((PyBytes::new(py, c),)) {
                    if let Ok(s) = result.extract::<String>() {
                        return s;
                    }
                }
            }
            // Fallback if callable fails
            "utf-8".to_string()
        } else if let Ok(s) = de.extract::<String>() {
            s
        } else {
            "utf-8".to_string()
        }
    }

    /// Resolve the actual encoding to use for text decoding.
    /// Uses the encoding property but validates the result.
    fn resolve_encoding(&self, py: Python<'_>) -> String {
        if let Some(enc) = self.encoding(py) {
            // Validate the encoding is a real Python codec
            if let Ok(codecs) = py.import("codecs") {
                if codecs.call_method1("lookup", (enc.as_str(),)).is_ok() {
                    return enc;
                }
            }
            // Invalid encoding, fall back to utf-8
            "utf-8".to_string()
        } else {
            "utf-8".to_string()
        }
    }

    // Streaming method stubs for compatibility
    #[pyo3(signature = (chunk_size=None))]
    fn iter_bytes(&mut self, py: Python<'_>, chunk_size: Option<usize>) -> PyResult<Py<PyAny>> {
        // Check lifecycle
        if self.is_stream_consumed && self.content_bytes.is_none() {
            return Err(crate::exceptions::StreamConsumed::new_err(
                "Attempted to read or stream content, but the stream has already been consumed.",
            ));
        }
        if self.is_closed_flag && self.content_bytes.is_none() {
            return Err(crate::exceptions::StreamClosed::new_err(
                "Attempted to read or stream content, but the stream has been closed.",
            ));
        }

        // If we have a stream (streaming body), consume it lazily
        if let Some(stream_py) = self.stream.take() {
            // Take ownership of stream
            let s = stream_py.bind(py);
            // Check for async - should raise RuntimeError
            if s.hasattr("__aiter__")? && !s.hasattr("__iter__")? {
                return Err(pyo3::exceptions::PyRuntimeError::new_err(
                    "Attempted to read an asynchronous response using a synchronous method.",
                ));
            }
            let mut all_chunks: Vec<Py<PyAny>> = Vec::new();
            let mut chunk_byte_sizes: Vec<usize> = Vec::new();
            let mut total_bytes = Vec::new();
            if let Ok(iter) = s.try_iter() {
                for item in iter {
                    let item = item?;
                    if let Ok(b) = item.cast::<PyBytes>() {
                        let bytes = b.as_bytes();
                        if !bytes.is_empty() {
                            total_bytes.extend_from_slice(bytes);
                            if chunk_size.is_none() {
                                all_chunks.push(PyBytes::new(py, bytes).into_any().unbind());
                                chunk_byte_sizes.push(bytes.len());
                            }
                        }
                    } else if let Ok(v) = item.extract::<Vec<u8>>() {
                        if !v.is_empty() {
                            total_bytes.extend_from_slice(&v);
                            if chunk_size.is_none() {
                                all_chunks.push(PyBytes::new(py, &v).into_any().unbind());
                                chunk_byte_sizes.push(v.len());
                            }
                        }
                    }
                }
            }
            self.content_bytes = Some(total_bytes.clone());
            self.is_stream_consumed = true;
            self.is_closed_flag = true;
            // self.stream is already None due to .take()

            if let Some(cs) = chunk_size {
                // Re-chunk from total_bytes
                all_chunks.clear();
                chunk_byte_sizes.clear();
                let mut offset = 0;
                while offset < total_bytes.len() {
                    let end = std::cmp::min(offset + cs, total_bytes.len());
                    let chunk_len = end - offset;
                    all_chunks.push(
                        PyBytes::new(py, &total_bytes[offset..end])
                            .into_any()
                            .unbind(),
                    );
                    chunk_byte_sizes.push(chunk_len);
                    offset = end;
                }
            }

            let counter = self.num_bytes_downloaded_counter.clone();
            let iter = crate::types::ChunkedIter {
                chunks: all_chunks,
                pos: 0,
                byte_counter: Some(counter),
                chunk_byte_sizes: Some(chunk_byte_sizes),
            };
            return Ok(Py::new(py, iter)?.into_any());
        }

        // Non-streaming: use content_bytes
        let content = self.content_bytes.clone().unwrap_or_default();
        if content.is_empty() {
            let list = PyList::empty(py);
            return Ok(list.call_method0("__iter__")?.into());
        }
        if let Some(cs) = chunk_size {
            let mut chunks = Vec::new();
            let mut offset = 0;
            while offset < content.len() {
                let end = std::cmp::min(offset + cs, content.len());
                chunks.push(PyBytes::new(py, &content[offset..end]).into_any().unbind());
                offset = end;
            }
            let list = PyList::new(py, &chunks)?;
            Ok(list.call_method0("__iter__")?.into())
        } else {
            let list = PyList::new(py, &[PyBytes::new(py, &content)])?;
            Ok(list.call_method0("__iter__")?.into())
        }
    }

    #[pyo3(signature = (chunk_size=None))]
    fn iter_raw(&mut self, py: Python<'_>, chunk_size: Option<usize>) -> PyResult<Py<PyAny>> {
        self.iter_bytes(py, chunk_size)
    }

    #[pyo3(signature = (chunk_size=None))]
    fn iter_text(&mut self, py: Python<'_>, chunk_size: Option<usize>) -> PyResult<Py<PyAny>> {
        // If we have a stream, iterate it chunk-by-chunk and decode text per chunk
        if self.content_bytes.is_none() && self.stream.is_some() {
            // Consume the stream, collecting raw chunks
            let stream = self.stream.take();
            let mut raw_chunks: Vec<Vec<u8>> = Vec::new();
            if let Some(ref s) = stream {
                let s = s.bind(py);
                if let Ok(iter) = s.try_iter() {
                    for item in iter {
                        let item = item?;
                        if let Ok(b) = item.cast::<PyBytes>() {
                            raw_chunks.push(b.as_bytes().to_vec());
                        } else if let Ok(v) = item.extract::<Vec<u8>>() {
                            raw_chunks.push(v);
                        }
                    }
                }
            }
            // Store full content
            let full: Vec<u8> = raw_chunks.iter().flatten().copied().collect();
            self.content_bytes = Some(full);
            self.is_stream_consumed = true;
            self.is_closed_flag = true;

            // Resolve encoding
            let encoding_name = self.resolve_encoding(py);

            if let Some(cs) = chunk_size {
                // With chunk_size, decode all text and split
                let text = self.text(py)?;
                if text.is_empty() {
                    let list = PyList::empty(py);
                    return Ok(list.call_method0("__iter__")?.into());
                }
                let mut chunks = Vec::new();
                let chars: Vec<char> = text.chars().collect();
                let mut offset = 0;
                while offset < chars.len() {
                    let end = std::cmp::min(offset + cs, chars.len());
                    let chunk: String = chars[offset..end].iter().collect();
                    chunks.push(chunk);
                    offset = end;
                }
                let list = PyList::new(py, &chunks)?;
                Ok(list.call_method0("__iter__")?.into())
            } else {
                // Without chunk_size, yield each stream chunk as separate text
                let mut text_chunks: Vec<String> = Vec::new();
                for chunk in &raw_chunks {
                    if !chunk.is_empty() {
                        let py_bytes = PyBytes::new(py, chunk);
                        let str_obj =
                            py_bytes.call_method1("decode", (&encoding_name, "replace"))?;
                        let s: String = str_obj.extract()?;
                        if !s.is_empty() {
                            text_chunks.push(s);
                        }
                    }
                }
                let list = PyList::new(py, &text_chunks)?;
                Ok(list.call_method0("__iter__")?.into())
            }
        } else {
            let text = self.text(py)?;
            if text.is_empty() {
                let list = PyList::empty(py);
                return Ok(list.call_method0("__iter__")?.into());
            }
            if let Some(cs) = chunk_size {
                let mut chunks = Vec::new();
                let chars: Vec<char> = text.chars().collect();
                let mut offset = 0;
                while offset < chars.len() {
                    let end = std::cmp::min(offset + cs, chars.len());
                    let chunk: String = chars[offset..end].iter().collect();
                    chunks.push(chunk);
                    offset = end;
                }
                let list = PyList::new(py, &chunks)?;
                Ok(list.call_method0("__iter__")?.into())
            } else {
                let list = PyList::new(py, &[text])?;
                Ok(list.call_method0("__iter__")?.into())
            }
        }
    }

    fn iter_lines(&mut self, py: Python<'_>) -> PyResult<Py<PyAny>> {
        // Ensure content is read if we have a stream
        if self.content_bytes.is_none() && self.stream.is_some() {
            self.read(py)?;
        }
        let text = self.text(py)?;
        // Split by \r\n, \r, or \n (matching Python's universal newline behavior)
        let mut lines: Vec<String> = Vec::new();
        let mut current = String::new();
        let chars: Vec<char> = text.chars().collect();
        let mut i = 0;
        while i < chars.len() {
            if chars[i] == '\r' {
                lines.push(std::mem::take(&mut current));
                if i + 1 < chars.len() && chars[i + 1] == '\n' {
                    i += 1; // skip \n after \r
                }
            } else if chars[i] == '\n' {
                lines.push(std::mem::take(&mut current));
            } else {
                current.push(chars[i]);
            }
            i += 1;
        }
        if !current.is_empty() {
            lines.push(current);
        }
        let list = PyList::new(py, &lines)?;
        Ok(list.call_method0("__iter__")?.into())
    }

    #[pyo3(signature = (chunk_size=None))]
    fn aiter_bytes(slf: Bound<'_, Self>, chunk_size: Option<usize>) -> PyResult<Py<PyAny>> {
        let py = slf.py();
        {
            let self_ = slf.borrow();
            // Check lifecycle
            if self_.is_stream_consumed && self_.content_bytes.is_none() {
                return Err(crate::exceptions::StreamConsumed::new_err(
                    "Attempted to read or stream content, but the stream has already been consumed."));
            }
            if self_.is_closed_flag && self_.content_bytes.is_none() {
                return Err(crate::exceptions::StreamClosed::new_err(
                    "Attempted to read or stream content, but the stream has been closed.",
                ));
            }

            // If we have a stream, check if it's sync-only - should raise RuntimeError
            if let Some(ref stream) = self_.stream {
                let s = stream.bind(py);
                let has_aiter = s.hasattr("__aiter__").unwrap_or(false);
                let has_iter = s.hasattr("__iter__").unwrap_or(false);
                if has_iter && !has_aiter {
                    return Err(pyo3::exceptions::PyRuntimeError::new_err(
                        "Attempted to read an synchronous response using an asynchronous method.",
                    ));
                }
            }
        }

        // For async streams, take the stream and use Python async for to drain it
        let stream_py = {
            let mut self_mut = slf.borrow_mut();
            self_mut.stream.take()
        };
        if let Some(stream_py) = stream_py {
            let ns = pyo3::types::PyDict::new(py);
            ns.set_item("_stream", stream_py)?;
            ns.set_item("_chunk_size", chunk_size)?;
            ns.set_item("_response", &slf)?;

            py.run(
                c"
async def _aiter_bytes_impl():
    chunks = []
    async for chunk in _stream:
        b = bytes(chunk)
        if b:
            chunks.append(b)
    return chunks

class _AsyncChunkIter:
    def __init__(self, coro, chunk_size, response):
        self._coro = coro
        self._chunk_size = chunk_size
        self._response = response
        self._chunks = None
        self._pos = 0
    
    def __aiter__(self):
        return self
    
    async def __anext__(self):
        if self._chunks is None:
            # First call: drain the stream
            raw_chunks = await self._coro
            # Re-chunk if needed
            if self._chunk_size is not None:
                total = b''.join(raw_chunks)
                cs = self._chunk_size
                self._chunks = [total[i:i+cs] for i in range(0, len(total), cs)]
            else:
                self._chunks = raw_chunks
        
        if self._pos >= len(self._chunks):
            raise StopAsyncIteration
        
        chunk = self._chunks[self._pos]
        self._pos += 1
        self._response._add_bytes_downloaded(len(chunk))
        return chunk

_result = _AsyncChunkIter(_aiter_bytes_impl(), _chunk_size, _response)
",
                Some(&ns),
                Some(&ns),
            )?;

            let result = ns.get_item("_result")?.unwrap();

            // Mark stream state
            {
                let mut self_mut = slf.borrow_mut();
                self_mut.is_stream_consumed = true;
                self_mut.is_closed_flag = true;
            }

            return Ok(result.unbind());
        }

        let self_ = slf.borrow();
        // Non-streaming: use content_bytes
        let content = self_.content_bytes.clone().unwrap_or_default();
        if content.is_empty() {
            let iter = crate::types::ChunkedIter {
                chunks: Vec::new(),
                pos: 0,
                byte_counter: None,
                chunk_byte_sizes: None,
            };
            return Ok(Py::new(py, iter)?.into_any());
        }
        if let Some(cs) = chunk_size {
            let mut chunks = Vec::new();
            let mut offset = 0;
            while offset < content.len() {
                let end = std::cmp::min(offset + cs, content.len());
                chunks.push(PyBytes::new(py, &content[offset..end]).into_any().unbind());
                offset = end;
            }
            let iter = crate::types::ChunkedIter {
                chunks,
                pos: 0,
                byte_counter: None,
                chunk_byte_sizes: None,
            };
            Ok(Py::new(py, iter)?.into_any())
        } else {
            let bs = crate::types::ByteStream {
                data: content,
                pos: 0,
            };
            let init = pyo3::PyClassInitializer::from(crate::types::AsyncByteStream)
                .add_subclass(crate::types::SyncByteStream)
                .add_subclass(bs);
            Ok(Py::new(py, init)?.into_any())
        }
    }

    #[pyo3(signature = (chunk_size=None))]
    fn aiter_raw(slf: Bound<'_, Self>, chunk_size: Option<usize>) -> PyResult<Py<PyAny>> {
        Self::aiter_bytes(slf, chunk_size)
    }

    #[pyo3(signature = (chunk_size=None))]
    fn aiter_text(&self, py: Python<'_>, chunk_size: Option<usize>) -> PyResult<Py<PyAny>> {
        let text = self.text(py)?;
        if text.is_empty() {
            let iter = crate::types::ChunkedIter {
                chunks: Vec::new(),
                pos: 0,
                byte_counter: None,
                chunk_byte_sizes: None,
            };
            return Ok(Py::new(py, iter)?.into_any());
        }
        if let Some(cs) = chunk_size {
            let mut chunks: Vec<Py<PyAny>> = Vec::new();
            let chars: Vec<char> = text.chars().collect();
            let mut offset = 0;
            while offset < chars.len() {
                let end = std::cmp::min(offset + cs, chars.len());
                let chunk: String = chars[offset..end].iter().collect();
                chunks.push(chunk.into_pyobject(py)?.into_any().unbind());
                offset = end;
            }
            let iter = crate::types::ChunkedIter {
                chunks,
                pos: 0,
                byte_counter: None,
                chunk_byte_sizes: None,
            };
            Ok(Py::new(py, iter)?.into_any())
        } else {
            let chunks = vec![text.into_pyobject(py)?.into_any().unbind()];
            let iter = crate::types::ChunkedIter {
                chunks,
                pos: 0,
                byte_counter: None,
                chunk_byte_sizes: None,
            };
            Ok(Py::new(py, iter)?.into_any())
        }
    }

    fn aiter_lines(&self, py: Python<'_>) -> PyResult<Py<PyAny>> {
        let text = self.text(py)?;
        let chunks: Vec<Py<PyAny>> = text
            .lines()
            .map(|l| l.to_string().into_pyobject(py).unwrap().into_any().unbind())
            .collect();
        let iter = crate::types::ChunkedIter {
            chunks,
            pos: 0,
            byte_counter: None,
            chunk_byte_sizes: None,
        };
        Ok(Py::new(py, iter)?.into_any())
    }

    fn aclose<'py>(&mut self, py: Python<'py>) -> PyResult<Bound<'py, PyAny>> {
        // Check if it's a sync stream - should raise RuntimeError
        if let Some(ref stream) = self.stream {
            let s = stream.bind(py);
            let has_aiter = s.hasattr("__aiter__").unwrap_or(false);
            let has_iter = s.hasattr("__iter__").unwrap_or(false);
            if has_iter && !has_aiter {
                return Err(pyo3::exceptions::PyRuntimeError::new_err(
                    "Attempted to close a synchronous response using 'await response.aclose()'. Use 'response.close()' instead."));
            }
        }
        self.is_closed_flag = true;
        pyo3_async_runtimes::tokio::future_into_py(py, async move { Ok(()) })
    }

    #[getter]
    fn is_stream_consumed(&self) -> bool {
        self.is_stream_consumed
    }
    fn __repr__(&self) -> String {
        let reason = self.reason_phrase();
        if reason.is_empty() {
            format!("<Response [{}]>", self.status_code)
        } else {
            format!("<Response [{} {}]>", self.status_code, reason)
        }
    }
    fn __bool__(&self) -> bool {
        self.is_success()
    }

    fn __reduce__(&self, py: Python<'_>) -> PyResult<Py<PyAny>> {
        let cls = py.import("httpr")?.getattr("Response")?;
        let args =
            pyo3::types::PyTuple::new(py, &[self.status_code.into_pyobject(py)?.into_any()])?;
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
        // Only set content if we have it (streaming responses that weren't read should not have content)
        if let Some(ref c) = self.content_bytes {
            state.set_item("content", PyBytes::new(py, c))?;
            state.set_item("has_content", true)?;
        } else {
            state.set_item("has_content", false)?;
        }
        state.set_item("is_stream_consumed", self.is_stream_consumed)?;
        state.set_item("was_streaming", self.was_streaming)?;
        state.set_item(
            "num_bytes_downloaded",
            self.num_bytes_downloaded_counter
                .load(std::sync::atomic::Ordering::Relaxed),
        )?;
        // Save request if present
        if let Some(ref req) = self.request {
            state.set_item("request", req.clone().into_pyobject(py)?)?;
        }
        let result =
            pyo3::types::PyTuple::new(py, &[cls.into_any(), args.into_any(), state.into_any()])?;
        Ok(result.into())
    }

    fn __setstate__(&mut self, py: Python<'_>, state: &Bound<'_, PyAny>) -> PyResult<()> {
        let dict = state.cast::<pyo3::types::PyDict>()?;
        if let Some(has_content) = dict.get_item("has_content")? {
            if has_content.extract::<bool>()? {
                if let Some(content) = dict.get_item("content")? {
                    self.content_bytes = Some(content.extract::<Vec<u8>>()?);
                }
            } else {
                // Streaming response that wasn't read - reset content to None
                self.content_bytes = None;
            }
        } else if let Some(content) = dict.get_item("content")? {
            // Legacy fallback
            self.content_bytes = Some(content.extract::<Vec<u8>>()?);
        }
        if let Some(headers) = dict.get_item("headers")? {
            let h_list: Vec<(Vec<u8>, Vec<u8>)> = headers.extract()?;
            let hdrs = Headers {
                raw: h_list,
                encoding: "utf-8".to_string(),
            };
            self.headers = Py::new(py, hdrs)?;
        }
        if let Some(consumed) = dict.get_item("is_stream_consumed")? {
            self.is_stream_consumed = consumed.extract::<bool>()?;
        } else {
            // Default: mark as consumed if we have content
            self.is_stream_consumed = self.content_bytes.is_some();
        }
        if let Some(was_str) = dict.get_item("was_streaming")? {
            self.was_streaming = was_str.extract::<bool>()?;
        }
        // Restore num_bytes_downloaded
        if let Some(nbd) = dict.get_item("num_bytes_downloaded")? {
            self.num_bytes_downloaded_counter.store(
                nbd.extract::<usize>()?,
                std::sync::atomic::Ordering::Relaxed,
            );
        } else if let Some(ref c) = self.content_bytes {
            self.num_bytes_downloaded_counter
                .store(c.len(), std::sync::atomic::Ordering::Relaxed);
        }
        // Always mark as closed after unpickling
        self.is_closed_flag = true;
        self.stream = None;
        // Restore request if present
        if let Some(req) = dict.get_item("request")? {
            if !req.is_none() {
                self.request = Some(req.extract::<Request>()?);
            }
        }
        Ok(())
    }
}

#[pyclass(from_py_object)]
pub struct Cookies {
    pub jar: Py<PyAny>,
}

impl Clone for Cookies {
    fn clone(&self) -> Self {
        Python::attach(|py| Cookies {
            jar: self.jar.clone_ref(py),
        })
    }
}

impl Cookies {
    pub fn create(py: Python<'_>, cookies: Option<&Bound<'_, PyAny>>) -> PyResult<Self> {
        let cookiejar = py.import("http.cookiejar")?;
        let jar = cookiejar.call_method0("CookieJar")?;
        if let Some(c) = cookies {
            if !c.is_none() {
                if let Ok(existing) = c.extract::<Cookies>() {
                    return Ok(existing);
                }
                // Check if it's a CookieJar (has set_cookie method and is iterable)
                let cookie_jar_class = cookiejar.getattr("CookieJar")?;
                if c.is_instance(&cookie_jar_class)? {
                    // Copy cookies from existing jar
                    for cookie in c.try_iter()? {
                        let cookie = cookie?;
                        jar.call_method1("set_cookie", (cookie,))?;
                    }
                } else if let Ok(d) = c.cast::<PyDict>() {
                    for (k, v) in d.iter() {
                        let name: String = k.extract()?;
                        let value: String = v.extract()?;
                        Self::set_cookie_on_jar_inner(py, &jar, &name, &value, "", "/")?;
                    }
                } else if let Ok(l) = c.cast::<PyList>() {
                    for item in l.iter() {
                        let tuple = item.cast::<PyTuple>()?;
                        let name: String = tuple.get_item(0)?.extract()?;
                        let value: String = tuple.get_item(1)?.extract()?;
                        Self::set_cookie_on_jar_inner(py, &jar, &name, &value, "", "/")?;
                    }
                }
            }
        }
        Ok(Cookies { jar: jar.into() })
    }

    pub(crate) fn set_cookie_on_jar_inner(
        py: Python<'_>,
        jar: &Bound<'_, PyAny>,
        name: &str,
        value: &str,
        domain: &str,
        path: &str,
    ) -> PyResult<()> {
        let cookiejar = py.import("http.cookiejar")?;
        let kwargs = PyDict::new(py);
        kwargs.set_item("version", 0i32)?;
        kwargs.set_item("name", name)?;
        kwargs.set_item("value", value)?;
        kwargs.set_item("port", py.None())?;
        kwargs.set_item("port_specified", false)?;
        kwargs.set_item("domain", domain)?;
        kwargs.set_item("domain_specified", false)?;
        kwargs.set_item("domain_initial_dot", domain.starts_with('.'))?;
        kwargs.set_item("path", path)?;
        kwargs.set_item("path_specified", path != "/")?;
        kwargs.set_item("secure", false)?;
        kwargs.set_item("expires", py.None())?;
        kwargs.set_item("discard", false)?;
        kwargs.set_item("comment", py.None())?;
        kwargs.set_item("comment_url", py.None())?;
        kwargs.set_item("rest", PyDict::new(py))?;
        let cookie = cookiejar.getattr("Cookie")?.call((), Some(&kwargs))?;
        jar.call_method1("set_cookie", (cookie,))?;
        Ok(())
    }
}

#[pymethods]
impl Cookies {
    #[new]
    #[pyo3(signature = (cookies=None))]
    fn new(py: Python<'_>, cookies: Option<&Bound<'_, PyAny>>) -> PyResult<Self> {
        Self::create(py, cookies)
    }

    #[pyo3(signature = (name, value, domain=None, path=None))]
    fn set(
        &self,
        py: Python<'_>,
        name: &str,
        value: &str,
        domain: Option<&str>,
        path: Option<&str>,
    ) -> PyResult<()> {
        Self::set_cookie_on_jar_inner(
            py,
            self.jar.bind(py),
            name,
            value,
            domain.unwrap_or(""),
            path.unwrap_or("/"),
        )
    }

    fn delete(
        &self,
        py: Python<'_>,
        name: &str,
        domain: Option<&str>,
        path: Option<&str>,
    ) -> PyResult<()> {
        // Remove cookies matching name/domain/path
        let jar_bound = self.jar.bind(py);
        let mut to_remove = Vec::new();
        for cookie in jar_bound.try_iter()? {
            let cookie = cookie?;
            let cookie_name: String = cookie.getattr("name")?.extract()?;
            if cookie_name != name {
                continue;
            }
            if let Some(d) = domain {
                let cookie_domain: String = cookie.getattr("domain")?.extract()?;
                if cookie_domain != d {
                    continue;
                }
            }
            if let Some(p) = path {
                let cookie_path: String = cookie.getattr("path")?.extract()?;
                if cookie_path != p {
                    continue;
                }
            }
            to_remove.push(cookie.clone().unbind());
        }
        for c in to_remove {
            jar_bound.call_method1(
                "clear",
                (
                    c.bind(py).getattr("domain")?,
                    c.bind(py).getattr("path")?,
                    c.bind(py).getattr("name")?,
                ),
            )?;
        }
        Ok(())
    }

    #[pyo3(signature = (domain=None, path=None))]
    fn clear(&self, py: Python<'_>, domain: Option<&str>, path: Option<&str>) -> PyResult<()> {
        let jar_bound = self.jar.bind(py);
        if domain.is_none() && path.is_none() {
            jar_bound.call_method0("clear")?;
        } else {
            let mut to_remove = Vec::new();
            for cookie in jar_bound.try_iter()? {
                let cookie = cookie?;
                let mut matches = true;
                if let Some(d) = domain {
                    let cookie_domain: String = cookie.getattr("domain")?.extract()?;
                    if cookie_domain != d {
                        matches = false;
                    }
                }
                if let Some(p) = path {
                    let cookie_path: String = cookie.getattr("path")?.extract()?;
                    if cookie_path != p {
                        matches = false;
                    }
                }
                if matches {
                    to_remove.push(cookie.clone().unbind());
                }
            }
            for c in to_remove {
                jar_bound.call_method1(
                    "clear",
                    (
                        c.bind(py).getattr("domain")?,
                        c.bind(py).getattr("path")?,
                        c.bind(py).getattr("name")?,
                    ),
                )?;
            }
        }
        Ok(())
    }

    fn extract_cookies(&self, py: Python<'_>, response: &Response) -> PyResult<()> {
        // Extract Set-Cookie headers from response and add to jar
        let headers = response.headers.bind(py).borrow();
        for (k, v) in headers.get_multi_items() {
            if k == "set-cookie" {
                if let Some(pos) = v.find('=') {
                    let name = &v[..pos];
                    let rest = &v[pos + 1..];
                    let value = rest.split(';').next().unwrap_or(rest);
                    Self::set_cookie_on_jar_inner(py, self.jar.bind(py), name, value, "", "/")?;
                }
            }
        }
        Ok(())
    }

    fn update(&self, py: Python<'_>, other: Option<&Bound<'_, PyAny>>) -> PyResult<()> {
        if let Some(o) = other {
            if let Ok(cookies) = o.extract::<Cookies>() {
                for cookie in cookies.jar.bind(py).try_iter()? {
                    let cookie = cookie?;
                    self.jar.bind(py).call_method1("set_cookie", (cookie,))?;
                }
            } else if let Ok(d) = o.cast::<PyDict>() {
                for (k, v) in d.iter() {
                    let name: String = k.extract()?;
                    let value: String = v.extract()?;
                    Self::set_cookie_on_jar_inner(py, self.jar.bind(py), &name, &value, "", "/")?;
                }
            }
        }
        Ok(())
    }

    fn keys(&self, py: Python<'_>) -> PyResult<Vec<String>> {
        let mut keys = Vec::new();
        for cookie in self.jar.bind(py).try_iter()? {
            let cookie = cookie?;
            keys.push(cookie.getattr("name")?.extract()?);
        }
        Ok(keys)
    }

    fn values(&self, py: Python<'_>) -> PyResult<Vec<String>> {
        let mut values = Vec::new();
        for cookie in self.jar.bind(py).try_iter()? {
            let cookie = cookie?;
            values.push(cookie.getattr("value")?.extract()?);
        }
        Ok(values)
    }

    #[pyo3(signature = (name, default=None, domain=None, path=None))]
    fn get(
        &self,
        py: Python<'_>,
        name: &str,
        default: Option<&str>,
        domain: Option<&str>,
        path: Option<&str>,
    ) -> PyResult<Option<String>> {
        let jar_bound = self.jar.bind(py);
        for cookie in jar_bound.try_iter()? {
            let cookie: Bound<'_, PyAny> = cookie?;
            let cookie_name: String = cookie.getattr("name")?.extract()?;
            if cookie_name == name {
                if let Some(d) = domain {
                    let cookie_domain: String = cookie.getattr("domain")?.extract()?;
                    if cookie_domain != d {
                        continue;
                    }
                }
                if let Some(p) = path {
                    let cookie_path: String = cookie.getattr("path")?.extract()?;
                    if cookie_path != p {
                        continue;
                    }
                }
                return Ok(Some(cookie.getattr("value")?.extract()?));
            }
        }
        Ok(default.map(|s| s.to_string()))
    }

    #[getter]
    fn jar(&self, py: Python<'_>) -> Py<PyAny> {
        self.jar.clone_ref(py)
    }

    fn __len__(&self, py: Python<'_>) -> PyResult<usize> {
        let mut count = 0;
        for _ in self.jar.bind(py).try_iter()? {
            count += 1;
        }
        Ok(count)
    }

    fn items(&self, py: Python<'_>) -> PyResult<Vec<(String, String)>> {
        let mut items = Vec::new();
        for cookie in self.jar.bind(py).try_iter()? {
            let cookie = cookie?;
            let name: String = cookie.getattr("name")?.extract()?;
            let value: String = cookie.getattr("value")?.extract()?;
            items.push((name, value));
        }
        Ok(items)
    }

    fn __repr__(&self, py: Python<'_>) -> PyResult<String> {
        let mut parts = Vec::new();
        for cookie in self.jar.bind(py).try_iter()? {
            let cookie = cookie?;
            let name: String = cookie.getattr("name")?.extract()?;
            let value: String = cookie.getattr("value")?.extract()?;
            let domain: String = cookie.getattr("domain")?.extract()?;
            let path: String = cookie.getattr("path")?.extract()?;
            if domain.is_empty() {
                parts.push(format!("<Cookie {}={} for {}>", name, value, path));
            } else {
                parts.push(format!(
                    "<Cookie {}={} for {} {}>",
                    name, value, domain, path
                ));
            }
        }
        Ok(format!("<Cookies[{}]>", parts.join(", ")))
    }

    fn __iter__(&self, py: Python<'_>) -> PyResult<Py<PyAny>> {
        let keys = self.keys(py)?;
        let list = PyList::new(py, &keys)?;
        Ok(list.call_method0("__iter__")?.into())
    }

    fn __getitem__(&self, py: Python<'_>, key: &str) -> PyResult<String> {
        let jar_bound = self.jar.bind(py);
        let mut values = Vec::new();
        for cookie in jar_bound.try_iter()? {
            let cookie = cookie?;
            let cookie_name: String = cookie.getattr("name")?.extract()?;
            if cookie_name == key {
                values.push(cookie.getattr("value")?.extract::<String>()?);
            }
        }
        if values.len() > 1 {
            return Err(crate::exceptions::CookieConflict::new_err(
                format!("Attempted to access the cookie '{}', but multiple cookies with that name exist. Use 'cookies.get(\"{}\")', passing the appropriate 'domain' or 'path' arguments.", key, key)
            ));
        }
        values
            .into_iter()
            .next()
            .ok_or_else(|| pyo3::exceptions::PyKeyError::new_err(key.to_string()))
    }

    fn __setitem__(&self, py: Python<'_>, key: &str, value: &str) -> PyResult<()> {
        Self::set_cookie_on_jar_inner(py, self.jar.bind(py), key, value, "", "/")
    }

    fn __delitem__(&self, py: Python<'_>, key: &str) -> PyResult<()> {
        self.delete(py, key, None, None)
    }

    fn __contains__(&self, py: Python<'_>, key: &str) -> PyResult<bool> {
        let jar_bound = self.jar.bind(py);
        for cookie in jar_bound.try_iter()? {
            let cookie = cookie?;
            let cookie_name: String = cookie.getattr("name")?.extract()?;
            if cookie_name == key {
                return Ok(true);
            }
        }
        Ok(false)
    }

    fn __bool__(&self, py: Python<'_>) -> PyResult<bool> {
        Ok(self.__len__(py)? > 0)
    }
}

pub fn register(m: &Bound<'_, PyModule>) -> PyResult<()> {
    m.add_class::<Headers>()?;
    m.add_class::<Request>()?;
    m.add_class::<Response>()?;
    m.add_class::<Cookies>()?;
    Ok(())
}
