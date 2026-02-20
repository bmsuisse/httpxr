use pyo3::prelude::*;
use pyo3::types::{PyDict, PyString};

use crate::models::Response;

/// Parse an HTTP method string into a `reqwest::Method`.
///
/// This eliminates the repeated 7-arm match block that was duplicated 6+ times
/// across sync/async transports and batch handlers.
pub fn parse_method(method_str: &str) -> Result<reqwest::Method, PyErr> {
    match method_str {
        "GET" => Ok(reqwest::Method::GET),
        "POST" => Ok(reqwest::Method::POST),
        "PUT" => Ok(reqwest::Method::PUT),
        "DELETE" => Ok(reqwest::Method::DELETE),
        "PATCH" => Ok(reqwest::Method::PATCH),
        "HEAD" => Ok(reqwest::Method::HEAD),
        "OPTIONS" => Ok(reqwest::Method::OPTIONS),
        _ => reqwest::Method::from_bytes(method_str.as_bytes())
            .map_err(|e| pyo3::exceptions::PyValueError::new_err(format!("Invalid method: {}", e))),
    }
}

/// Extract timeout values from a Request's extensions dict.
///
/// Returns `(connect, read, write, pool)` as `Option<f64>` quadruple.
/// This was duplicated in `send_request`, `send_batch_requests`,
/// `handle_async_request`, and `handle_async_requests_batch`.
pub fn extract_timeout_from_extensions(
    py: Python<'_>,
    extensions: &Py<PyAny>,
) -> (Option<f64>, Option<f64>, Option<f64>, Option<f64>) {
    if let Ok(ext) = extensions.bind(py).cast::<PyDict>() {
        if let Some(timeout_obj) = ext.get_item("timeout").ok().flatten() {
            if let Ok(t) = timeout_obj.extract::<crate::config::Timeout>() {
                return (t.connect, t.read, t.write, t.pool);
            }
        }
    }
    (None, None, None, None)
}

/// Compute the effective single timeout duration from individual timeout values.
///
/// Takes the minimum of all specified timeout values and converts to `Duration`.
pub fn compute_effective_timeout(
    connect: Option<f64>,
    read: Option<f64>,
    write: Option<f64>,
) -> Option<std::time::Duration> {
    [connect, read, write]
        .iter()
        .filter_map(|t| *t)
        .reduce(f64::min)
        .map(std::time::Duration::from_secs_f64)
}

/// Map a `reqwest::Error` to the appropriate Python exception.
///
/// This consolidates the error mapping logic that was duplicated in
/// `send_request`, `handle_async_request`, and `send_raw`.
pub fn map_reqwest_error(
    e: reqwest::Error,
    connect_timeout: Option<f64>,
    read_timeout: Option<f64>,
    write_timeout: Option<f64>,
    has_body: bool,
) -> PyErr {
    let msg = format!("{}", e);
    if e.is_timeout() {
        if e.is_connect()
            || (connect_timeout.is_some()
                && read_timeout.is_none()
                && write_timeout.is_none())
        {
            crate::exceptions::ConnectTimeout::new_err(msg)
        } else if write_timeout.is_some() && has_body && read_timeout.is_none() {
            crate::exceptions::WriteTimeout::new_err(msg)
        } else {
            crate::exceptions::ReadTimeout::new_err(msg)
        }
    } else if e.is_connect() {
        crate::exceptions::ConnectError::new_err(msg)
    } else if e.is_body() || e.is_decode() {
        if read_timeout.is_some() || write_timeout.is_some() {
            crate::exceptions::ReadTimeout::new_err(msg)
        } else {
            crate::exceptions::NetworkError::new_err(msg)
        }
    } else if msg.contains("Unknown Scheme") || msg.contains("URL scheme is not allowed") {
        crate::exceptions::UnsupportedProtocol::new_err(msg)
    } else if msg.contains("certificate")
        || msg.contains("Certificate")
        || msg.contains("(Connect)")
    {
        crate::exceptions::ConnectError::new_err(msg)
    } else {
        crate::exceptions::NetworkError::new_err(msg)
    }
}

/// Simplified error mapping for contexts without detailed timeout info (e.g. `send_raw`, batch).
pub fn map_reqwest_error_simple(e: reqwest::Error) -> PyErr {
    let msg = format!("{}", e);
    if e.is_timeout() {
        if e.is_connect() {
            crate::exceptions::ConnectTimeout::new_err(msg)
        } else {
            crate::exceptions::ReadTimeout::new_err(msg)
        }
    } else if e.is_connect() {
        crate::exceptions::ConnectError::new_err(msg)
    } else if msg.contains("Unknown Scheme") || msg.contains("URL scheme is not allowed") {
        crate::exceptions::UnsupportedProtocol::new_err(msg)
    } else {
        crate::exceptions::NetworkError::new_err(msg)
    }
}

/// Build a default `Response` from raw parts (status, headers, body bytes).
///
/// This consolidates the ~20-field struct literal that was copy-pasted 5+ times
/// across `send_request`, `send_batch_requests`, `handle_async_request`, and
/// `handle_async_requests_batch`.
pub fn build_default_response(
    py: Python<'_>,
    status_code: u16,
    resp_headers: Vec<(Vec<u8>, Vec<u8>)>,
    body_bytes: Vec<u8>,
) -> PyResult<Response> {
    let body_len = body_bytes.len();
    Ok(Response {
        status_code,
        headers: None,
        lazy_headers: Some(resp_headers),
        extensions: None,
        request: None,
        lazy_request_method: None,
        lazy_request_url: None,
        history: Vec::new(),
        content_bytes: Some(body_bytes),
        stream: None,
        default_encoding: PyString::intern(py, "utf-8").into_any().unbind(),
        default_encoding_override: None,
        elapsed: None,
        is_closed_flag: true,
        is_stream_consumed: true,
        was_streaming: false,
        text_accessed: std::sync::atomic::AtomicBool::new(false),
        num_bytes_downloaded_counter: std::sync::Arc::new(
            std::sync::atomic::AtomicUsize::new(body_len),
        ),
    })
}

/// Convert an HTTP version enum value to its string representation.
pub fn http_version_str(version: reqwest::Version) -> &'static str {
    match version {
        reqwest::Version::HTTP_09 => "HTTP/0.9",
        reqwest::Version::HTTP_10 => "HTTP/1.0",
        reqwest::Version::HTTP_11 => "HTTP/1.1",
        reqwest::Version::HTTP_2 => "HTTP/2",
        reqwest::Version::HTTP_3 => "HTTP/3",
        _ => "HTTP/1.1",
    }
}

/// Convert a vector of raw results into a Python list of `(status, headers_dict, body)` tuples.
///
/// Shared by `send_batch_raw`, sync `gather_raw`, and async `gather_raw`.
/// Each `Ok` result becomes a tuple; each `Err` becomes a `NetworkError` exception object.
pub fn raw_results_to_pylist(
    py: Python<'_>,
    results: Vec<Result<(u16, Vec<(String, Vec<u8>)>, Vec<u8>), String>>,
) -> PyResult<Py<pyo3::types::PyList>> {
    use pyo3::types::{PyBytes, PyTuple};
    use pyo3::IntoPyObject;

    let py_list = pyo3::types::PyList::empty(py);
    for result in results {
        match result {
            Ok((status, resp_headers, body_bytes)) => {
                let dict = PyDict::new(py);
                for (k, v) in &resp_headers {
                    let val_str = std::str::from_utf8(v).unwrap_or("");
                    let key_intern = crate::utils::intern_header_key(py, k.as_str());
                    dict.set_item(key_intern.as_any(), val_str)?;
                }
                let body_py = PyBytes::new(py, &body_bytes);
                let tuple = PyTuple::new(py, &[
                    status.into_pyobject(py)?.into_any().as_borrowed(),
                    dict.as_any().as_borrowed(),
                    body_py.as_any().as_borrowed(),
                ])?;
                py_list.append(tuple)?;
            }
            Err(msg) => {
                let err = crate::exceptions::NetworkError::new_err(msg);
                py_list.append(err.value(py))?;
            }
        }
    }
    Ok(py_list.unbind())
}
