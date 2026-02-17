use pyo3::prelude::*;
use pyo3::types::{PyBool, PyBytes, PyDict, PyFloat, PyList, PyString, PyTuple};

/// Convert a simd_json::OwnedValue directly to a Python object.
/// This is the fast path — no intermediate serde_json conversion.
pub(crate) fn simd_value_to_py(py: Python<'_>, val: &simd_json::OwnedValue) -> PyResult<Py<PyAny>> {
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

/// Convert a serde_json::Value directly to a Python object.
/// This is the primary fast path — no clone of content bytes needed.
pub(crate) fn serde_value_to_py(py: Python<'_>, val: &serde_json::Value) -> PyResult<Py<PyAny>> {
    use serde_json::Value;
    match val {
        Value::Null => Ok(py.None()),
        Value::Bool(b) => Ok(PyBool::new(py, *b).to_owned().into_any().unbind()),
        Value::Number(n) => {
            if let Some(i) = n.as_i64() {
                Ok(i.into_pyobject(py).unwrap().into_any().unbind())
            } else if let Some(u) = n.as_u64() {
                Ok(u.into_pyobject(py).unwrap().into_any().unbind())
            } else if let Some(f) = n.as_f64() {
                Ok(PyFloat::new(py, f).into_any().unbind())
            } else {
                Ok(py.None())
            }
        }
        Value::String(s) => Ok(PyString::new(py, s).into_any().unbind()),
        Value::Array(arr) => {
            let items: Vec<Py<PyAny>> = arr
                .iter()
                .map(|v| serde_value_to_py(py, v))
                .collect::<PyResult<_>>()?;
            Ok(PyList::new(py, &items)?.into_any().unbind())
        }
        Value::Object(map) => {
            let dict = PyDict::new(py);
            for (k, v) in map.iter() {
                dict.set_item(k, serde_value_to_py(py, v)?)?;
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

    /// Construct Headers directly from byte pairs — zero conversion, skip encoding detection.
    /// HTTP response headers are always ASCII, so we hardcode the encoding.
    pub fn from_raw_byte_pairs(raw: Vec<(Vec<u8>, Vec<u8>)>) -> Self {
        Headers {
            raw,
            encoding: "ascii".to_string(),
        }
    }

    /// Empty headers — fastest construction for minimal Request objects.
    pub fn empty() -> Self {
        Headers {
            raw: Vec::new(),
            encoding: "ascii".to_string(),
        }
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
            let mut keys_to_remove: Vec<String> = Vec::new();
            for (key, _) in &new_headers.raw {
                let key_lower = String::from_utf8_lossy(key).to_lowercase();
                if !keys_to_remove.contains(&key_lower) {
                    keys_to_remove.push(key_lower);
                }
            }
            self.raw.retain(|(k, _)| {
                let kl = String::from_utf8_lossy(k).to_lowercase();
                !keys_to_remove.contains(&kl)
            });
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
        if hint != "utf-8" {
            return hint.to_string();
        }
        let all_ascii = raw.iter().all(|(_, v)| v.iter().all(|b| b.is_ascii()));
        if all_ascii {
            return "ascii".to_string();
        }
        let all_utf8 = raw.iter().all(|(_, v)| std::str::from_utf8(v).is_ok());
        if all_utf8 {
            return "utf-8".to_string();
        }
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

        let keys: Vec<&str> = multi.iter().map(|(k, _)| k.as_str()).collect();
        let has_dups = {
            let mut seen = std::collections::HashSet::new();
            keys.iter().any(|k| !seen.insert(*k))
        };

        let apply_secure = |k: &str, v: &str| -> String {
            if sensitive.contains(&k) {
                "[secure]".to_string()
            } else {
                v.to_string()
            }
        };

        let encoding_suffix = if self.encoding != "ascii" {
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
            let parts: Vec<String> = multi
                .iter()
                .map(|(k, v)| format!("('{}', '{}')", k, apply_secure(k, v)))
                .collect();
            format!("Headers([{}]{})", parts.join(", "), encoding_suffix)
        } else {
            let parts: Vec<String> = multi
                .iter()
                .map(|(k, v)| format!("'{}': '{}'", k, apply_secure(k, v)))
                .collect();
            format!("Headers({{{}}}{})", parts.join(", "), encoding_suffix)
        }
    }
}
