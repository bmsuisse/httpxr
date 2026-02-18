use pyo3::prelude::*;
use pyo3::types::{PyBytes, PyDict, PyList, PyTuple};

/// Query parameters multidict.
#[pyclass(from_py_object)]
#[derive(Clone, Debug)]
pub struct QueryParams {
    pub(crate) items: Vec<(String, String)>,
}

impl QueryParams {
    pub fn from_query_string(query: &str) -> Self {
        let items: Vec<(String, String)> = query
            .split('&')
            .filter(|s| !s.is_empty())
            .map(|pair| {
                if let Some(pos) = pair.find('=') {
                    (
                        percent_decode(&pair[..pos]),
                        percent_decode(&pair[pos + 1..]),
                    )
                } else {
                    (percent_decode(pair), String::new())
                }
            })
            .collect();
        QueryParams { items }
    }

    pub fn encode(&self) -> String {
        let mut serializer = url::form_urlencoded::Serializer::new(String::new());
        for (k, v) in &self.items {
            serializer.append_pair(k, v);
        }
        serializer.finish()
    }

    fn value_to_string(v: &Bound<'_, PyAny>) -> PyResult<String> {
        if v.is_none() {
            Ok(String::new())
        } else if let Ok(b) = v.extract::<bool>() {
            Ok(if b {
                "true".to_string()
            } else {
                "false".to_string()
            })
        } else if let Ok(i) = v.extract::<i64>() {
            Ok(i.to_string())
        } else if let Ok(f) = v.extract::<f64>() {
            Ok(format!("{}", f))
        } else {
            Ok(v.str()?.extract::<String>()?)
        }
    }

    pub fn items_raw(&self) -> Vec<(String, String)> {
        self.items.clone()
    }

    pub fn from_items(items: Vec<(String, String)>) -> Self {
        QueryParams { items }
    }

    pub fn create(params: Option<&Bound<'_, PyAny>>) -> PyResult<Self> {
        let mut items = Vec::new();
        if let Some(p) = params {
            if p.is_none() {
            } else if let Ok(existing) = p.extract::<QueryParams>() {
                items = existing.items;
            } else if let Ok(s) = p.extract::<String>() {
                return Ok(Self::from_query_string(&s));
            } else if let Ok(b) = p.cast::<PyBytes>() {
                let s = std::str::from_utf8(b.as_bytes()).unwrap_or_default();
                return Ok(Self::from_query_string(s));
            } else if let Ok(d) = p.cast::<PyDict>() {
                for (k, v) in d.iter() {
                    let key: String = k.str()?.extract()?;
                    if let Ok(list) = v.cast::<PyList>() {
                        for item in list.iter() {
                            items.push((key.clone(), Self::value_to_string(&item)?));
                        }
                    } else if let Ok(tuple) = v.cast::<PyTuple>() {
                        for i in 0..tuple.len() {
                            let item = tuple.get_item(i)?;
                            items.push((key.clone(), Self::value_to_string(&item)?));
                        }
                    } else {
                        items.push((key, Self::value_to_string(&v)?));
                    }
                }
            } else if let Ok(l) = p.cast::<PyList>() {
                for item in l.iter() {
                    let tuple = item.cast::<PyTuple>()?;
                    let k = tuple.get_item(0)?.str()?.extract::<String>()?;
                    let v = Self::value_to_string(&tuple.get_item(1)?)?;
                    items.push((k, v));
                }
            } else if let Ok(t) = p.cast::<PyTuple>() {
                for i in 0..t.len() {
                    let item = t.get_item(i)?;
                    let inner = item.cast::<PyTuple>()?;
                    let k = inner.get_item(0)?.str()?.extract::<String>()?;
                    let v = Self::value_to_string(&inner.get_item(1)?)?;
                    items.push((k, v));
                }
            }
        }
        Ok(QueryParams { items })
    }
}

#[allow(dead_code)]
pub fn percent_encode(s: &str) -> String {
    percent_encoding::utf8_percent_encode(s, percent_encoding::NON_ALPHANUMERIC).to_string()
}

pub fn percent_decode(s: &str) -> String {
    percent_encoding::percent_decode_str(s)
        .decode_utf8_lossy()
        .to_string()
}

#[pymethods]
impl QueryParams {
    #[new]
    #[pyo3(signature = (params=None))]
    fn new(params: Option<&Bound<'_, PyAny>>) -> PyResult<Self> {
        Self::create(params)
    }

    fn keys(&self) -> Vec<String> {
        let mut seen = std::collections::HashSet::new();
        let mut result = Vec::new();
        for (k, _) in &self.items {
            if seen.insert(k.clone()) {
                result.push(k.clone());
            }
        }
        result
    }
    fn values(&self) -> Vec<String> {
        let mut seen = std::collections::HashSet::new();
        let mut result = Vec::new();
        for (k, v) in &self.items {
            if seen.insert(k.clone()) {
                result.push(v.clone());
            }
        }
        result
    }
    fn items(&self) -> Vec<(String, String)> {
        let mut seen = std::collections::HashSet::new();
        let mut result = Vec::new();
        for (k, v) in &self.items {
            if seen.insert(k.clone()) {
                result.push((k.clone(), v.clone()));
            }
        }
        result
    }
    fn multi_items(&self) -> Vec<(String, String)> {
        self.items.clone()
    }

    #[pyo3(signature = (key, default=None))]
    fn get(&self, key: &str, default: Option<&str>) -> Option<String> {
        self.items
            .iter()
            .find(|(k, _)| k == key)
            .map(|(_, v)| v.clone())
            .or_else(|| default.map(|s| s.to_string()))
    }

    fn get_list(&self, key: &str) -> Vec<String> {
        self.items
            .iter()
            .filter(|(k, _)| k == key)
            .map(|(_, v)| v.clone())
            .collect()
    }

    fn set(&self, key: &str, value: &str) -> Self {
        let mut items: Vec<(String, String)> = self
            .items
            .iter()
            .filter(|(k, _)| k != key)
            .cloned()
            .collect();
        items.push((key.to_string(), value.to_string()));
        QueryParams { items }
    }

    fn add(&self, key: &str, value: &str) -> Self {
        let mut items = self.items.clone();
        items.push((key.to_string(), value.to_string()));
        QueryParams { items }
    }

    fn remove(&self, key: &str) -> Self {
        let items: Vec<(String, String)> = self
            .items
            .iter()
            .filter(|(k, _)| k != key)
            .cloned()
            .collect();
        QueryParams { items }
    }

    fn merge(&self, other: &Bound<'_, PyAny>) -> PyResult<Self> {
        let other_qp = Self::create(Some(other))?;
        let mut items = Vec::new();
        let other_keys: std::collections::HashSet<String> =
            other_qp.items.iter().map(|(k, _)| k.clone()).collect();
        for (k, v) in &self.items {
            if !other_keys.contains(k) {
                items.push((k.clone(), v.clone()));
            }
        }
        items.extend(other_qp.items);
        Ok(QueryParams { items })
    }

    fn __str__(&self) -> String {
        self.encode()
    }
    fn __repr__(&self) -> String {
        format!("QueryParams('{}')", self.encode())
    }
    fn __getitem__(&self, key: &str) -> PyResult<String> {
        self.get(key, None)
            .ok_or_else(|| pyo3::exceptions::PyKeyError::new_err(key.to_string()))
    }
    fn __setitem__(&self, _key: &str, _value: &str) -> PyResult<()> {
        Err(pyo3::exceptions::PyRuntimeError::new_err(
            "QueryParams are immutable",
        ))
    }
    fn __contains__(&self, key: &str) -> bool {
        self.items.iter().any(|(k, _)| k == key)
    }
    fn __len__(&self) -> usize {
        let mut seen = std::collections::HashSet::new();
        for (k, _) in &self.items {
            seen.insert(k);
        }
        seen.len()
    }
    fn __bool__(&self) -> bool {
        !self.items.is_empty()
    }
    fn __eq__(&self, other: &Bound<'_, PyAny>) -> bool {
        if let Ok(other_qp) = other.extract::<QueryParams>() {
            let mut a = self.items.clone();
            let mut b = other_qp.items.clone();
            a.sort();
            b.sort();
            a == b
        } else {
            false
        }
    }
    fn __hash__(&self) -> u64 {
        use std::hash::{Hash, Hasher};
        let mut hasher = std::collections::hash_map::DefaultHasher::new();
        let mut sorted = self.items.clone();
        sorted.sort();
        sorted.hash(&mut hasher);
        hasher.finish()
    }
    fn __iter__(&self, py: Python<'_>) -> PyResult<Py<PyAny>> {
        let keys = self.keys();
        let list = PyList::new(py, &keys)?;
        Ok(list.call_method0("__iter__")?.into())
    }

    fn update(&self, _params: &Bound<'_, PyAny>) -> PyResult<Self> {
        Err(pyo3::exceptions::PyRuntimeError::new_err(
            "QueryParams are immutable",
        ))
    }
}

pub fn register(m: &Bound<'_, PyModule>) -> PyResult<()> {
    m.add_class::<QueryParams>()?;
    Ok(())
}
