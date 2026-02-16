use pyo3::prelude::*;
use pyo3::types::{PyBytes, PyDict};

/// ByteStream: wraps raw bytes for request/response bodies.
#[pyclass]
#[derive(Clone)]
pub struct ByteStream {
    data: Vec<u8>,
}

#[pymethods]
impl ByteStream {
    #[new]
    #[pyo3(signature = (body=None))]
    fn new(body: Option<&[u8]>) -> Self {
        ByteStream {
            data: body.unwrap_or(b"").to_vec(),
        }
    }

    fn __iter__(slf: PyRef<'_, Self>) -> ByteStreamIter {
        ByteStreamIter {
            data: slf.data.clone(),
            consumed: false,
        }
    }

    fn close(&self) -> PyResult<()> {
        Ok(())
    }

    fn read(&self) -> Vec<u8> {
        self.data.clone()
    }
}

#[pyclass]
pub struct ByteStreamIter {
    data: Vec<u8>,
    consumed: bool,
}

#[pymethods]
impl ByteStreamIter {
    fn __iter__(slf: PyRef<'_, Self>) -> PyRef<'_, Self> {
        slf
    }

    fn __next__(&mut self, py: Python<'_>) -> Option<Py<PyAny>> {
        if self.consumed {
            None
        } else {
            self.consumed = true;
            if self.data.is_empty() {
                None
            } else {
                Some(PyBytes::new(py, &self.data).into())
            }
        }
    }
}

/// Encode content for a request based on the provided arguments.
/// Returns (headers_dict, stream_or_none)
#[pyfunction]
#[pyo3(signature = (content=None, data=None, files=None, json=None))]
pub fn encode_request(
    py: Python<'_>,
    content: Option<&Bound<'_, PyAny>>,
    data: Option<&Bound<'_, PyAny>>,
    files: Option<&Bound<'_, PyAny>>,
    json: Option<&Bound<'_, PyAny>>,
) -> PyResult<(Py<PyAny>, Py<PyAny>)> {
    let headers = PyDict::new(py);

    // JSON content
    if let Some(j) = json {
        let json_mod = py.import("json")?;
        let body_str: String = json_mod.call_method1("dumps", (j,))?.extract()?;
        let body_bytes = body_str.into_bytes();
        headers.set_item("content-type", "application/json")?;
        headers.set_item("content-length", body_bytes.len().to_string())?;
        let stream = ByteStream::new(Some(&body_bytes));
        return Ok((headers.into(), stream.into_pyobject(py)?.into_any().into()));
    }

    // Raw content (bytes or str)
    if let Some(c) = content {
        if let Ok(b) = c.downcast::<PyBytes>() {
            let body = b.as_bytes();
            headers.set_item("content-length", body.len().to_string())?;
            let stream = ByteStream::new(Some(body));
            return Ok((headers.into(), stream.into_pyobject(py)?.into_any().into()));
        }
        if let Ok(s) = c.extract::<String>() {
            let body = s.into_bytes();
            headers.set_item("content-length", body.len().to_string())?;
            headers.set_item("content-type", "text/plain; charset=utf-8")?;
            let stream = ByteStream::new(Some(&body));
            return Ok((headers.into(), stream.into_pyobject(py)?.into_any().into()));
        }
        // Iterable or async iterable — pass through as-is
        headers.set_item("transfer-encoding", "chunked")?;
        return Ok((headers.into(), c.clone().unbind()));
    }

    // URL-encoded form data
    if let Some(d) = data {
        if files.is_none() {
            // Simple form encoding
            let urllib = py.import("urllib.parse")?;
            let encoded: String = urllib.call_method1("urlencode", (d, true))?.extract()?;
            let body = encoded.into_bytes();
            headers.set_item("content-type", "application/x-www-form-urlencoded")?;
            headers.set_item("content-length", body.len().to_string())?;
            let stream = ByteStream::new(Some(&body));
            return Ok((headers.into(), stream.into_pyobject(py)?.into_any().into()));
        }
    }

    // Multipart (files + optional data)
    if let Some(_f) = files {
        // Delegate to multipart module
        let multipart = py.import("httpr._multipart")?;
        let data_arg = data.map(|d| d.clone().unbind()).unwrap_or(py.None());
        let stream = multipart.call_method1("MultipartStream", (data_arg, _f))?;
        let mp_headers = stream.call_method0("get_headers")?;
        let ct: String = mp_headers.call_method1("get", ("content-type",))?.extract()?;
        headers.set_item("content-type", ct)?;
        headers.set_item("transfer-encoding", "chunked")?;
        return Ok((headers.into(), stream.into()));
    }

    // No content
    let stream = ByteStream::new(None);
    Ok((headers.into(), stream.into_pyobject(py)?.into_any().into()))
}

pub fn register(m: &Bound<'_, PyModule>) -> PyResult<()> {
    m.add_class::<ByteStream>()?;
    m.add_class::<ByteStreamIter>()?;
    m.add_function(wrap_pyfunction!(encode_request, m)?)?;
    Ok(())
}
