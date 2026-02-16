use pyo3::prelude::*;
use pyo3::types::{PyBool, PyBytes, PyFloat, PyInt, PyString};

/// Coerce a primitive data type into a string value.
#[pyfunction]
pub fn primitive_value_to_str(py: Python<'_>, value: &Bound<'_, PyAny>) -> PyResult<String> {
    if value.is_none() {
        return Ok(String::new());
    }
    if let Ok(b) = value.downcast::<PyBool>() {
        return Ok(if b.is_true() { "true" } else { "false" }.to_string());
    }
    if let Ok(s) = value.downcast::<PyString>() {
        return Ok(s.to_str()?.to_string());
    }
    if let Ok(_i) = value.downcast::<PyInt>() {
        return Ok(value.str()?.to_str()?.to_string());
    }
    if let Ok(_f) = value.downcast::<PyFloat>() {
        return Ok(value.str()?.to_str()?.to_string());
    }
    Ok(value.str()?.to_str()?.to_string())
}

#[pyfunction]
pub fn to_bytes(value: &Bound<'_, PyAny>, encoding: Option<&str>) -> PyResult<Py<PyAny>> {
    let enc = encoding.unwrap_or("utf-8");
    let py = value.py();
    if let Ok(s) = value.downcast::<PyString>() {
        let bytes_val = s.to_str()?.as_bytes().to_vec();
        Ok(PyBytes::new(py, &bytes_val).into())
    } else if let Ok(b) = value.downcast::<PyBytes>() {
        Ok(b.clone().into())
    } else {
        // Try calling encode on the value
        let result = value.call_method1("encode", (enc,))?;
        Ok(result.into())
    }
}

#[pyfunction]
pub fn to_str(value: &Bound<'_, PyAny>, encoding: Option<&str>) -> PyResult<String> {
    let _enc = encoding.unwrap_or("utf-8");
    if let Ok(s) = value.downcast::<PyString>() {
        Ok(s.to_str()?.to_string())
    } else if let Ok(b) = value.downcast::<PyBytes>() {
        Ok(String::from_utf8_lossy(b.as_bytes()).to_string())
    } else {
        Ok(value.str()?.to_str()?.to_string())
    }
}

#[pyfunction]
pub fn to_bytes_or_str(py: Python<'_>, value: &str, match_type_of: &Bound<'_, PyAny>) -> PyResult<Py<PyAny>> {
    if match_type_of.downcast::<PyString>().is_ok() {
        Ok(PyString::new(py, value).into())
    } else {
        Ok(PyBytes::new(py, value.as_bytes()).into())
    }
}

#[pyfunction]
pub fn unquote(value: &str) -> String {
    if value.len() >= 2 && value.starts_with('"') && value.ends_with('"') {
        value[1..value.len()-1].to_string()
    } else {
        value.to_string()
    }
}

#[pyfunction]
pub fn peek_filelike_length(py: Python<'_>, stream: &Bound<'_, PyAny>) -> PyResult<Py<PyAny>> {
    // Try fileno() first
    if let Ok(fd) = stream.call_method0("fileno") {
        let os = py.import("os")?;
        let stat = os.call_method1("fstat", (fd,))?;
        let size = stat.getattr("st_size")?;
        return Ok(size.into());
    }

    // Try tell/seek
    if let (Ok(offset), Ok(_)) = (stream.call_method0("tell"), stream.call_method0("tell")) {
        let offset = stream.call_method0("tell")?;
        let length = stream.call_method1("seek", (0i64, 2i64))?;
        stream.call_method1("seek", (offset,))?;
        return Ok(length.into());
    }

    Ok(py.None())
}

#[pyfunction]
pub fn is_ipv4_hostname(hostname: &str) -> bool {
    let parts: Vec<&str> = hostname.split('/').collect();
    let host = parts[0];
    host.parse::<std::net::Ipv4Addr>().is_ok()
}

#[pyfunction]
pub fn is_ipv6_hostname(hostname: &str) -> bool {
    let parts: Vec<&str> = hostname.split('/').collect();
    let host = parts[0];
    host.parse::<std::net::Ipv6Addr>().is_ok()
}

#[pyfunction]
pub fn get_environment_proxies(py: Python<'_>) -> PyResult<Py<PyAny>> {
    // Delegate to Python's urllib.request.getproxies and process
    let urllib = py.import("urllib.request")?;
    let proxy_info = urllib.call_method0("getproxies")?;
    let dict = pyo3::types::PyDict::new(py);

    for scheme in ["http", "https", "all"] {
        let val = proxy_info.call_method1("get", (scheme,))?;
        if !val.is_none() {
            let hostname: String = val.extract()?;
            if !hostname.is_empty() {
                let mount = format!("{scheme}://");
                let url = if hostname.contains("://") {
                    hostname
                } else {
                    format!("http://{hostname}")
                };
                dict.set_item(mount, url)?;
            }
        }
    }

    let no_proxy = proxy_info.call_method1("get", ("no", ""))?;
    let no_proxy_str: String = no_proxy.extract()?;
    let no_proxy_hosts: Vec<&str> = no_proxy_str.split(',').map(|s| s.trim()).collect();

    for hostname in no_proxy_hosts {
        if hostname == "*" {
            return Ok(pyo3::types::PyDict::new(py).into());
        } else if !hostname.is_empty() {
            if hostname.contains("://") {
                dict.set_item(hostname, py.None())?;
            } else if is_ipv4_hostname(hostname) {
                dict.set_item(format!("all://{hostname}"), py.None())?;
            } else if is_ipv6_hostname(hostname) {
                dict.set_item(format!("all://[{hostname}]"), py.None())?;
            } else if hostname.to_lowercase() == "localhost" {
                dict.set_item(format!("all://{hostname}"), py.None())?;
            } else {
                dict.set_item(format!("all://*{hostname}"), py.None())?;
            }
        }
    }

    Ok(dict.into())
}

pub fn register(m: &Bound<'_, PyModule>) -> PyResult<()> {
    m.add_function(wrap_pyfunction!(primitive_value_to_str, m)?)?;
    m.add_function(wrap_pyfunction!(to_bytes, m)?)?;
    m.add_function(wrap_pyfunction!(to_str, m)?)?;
    m.add_function(wrap_pyfunction!(to_bytes_or_str, m)?)?;
    m.add_function(wrap_pyfunction!(unquote, m)?)?;
    m.add_function(wrap_pyfunction!(peek_filelike_length, m)?)?;
    m.add_function(wrap_pyfunction!(is_ipv4_hostname, m)?)?;
    m.add_function(wrap_pyfunction!(is_ipv6_hostname, m)?)?;
    m.add_function(wrap_pyfunction!(get_environment_proxies, m)?)?;
    Ok(())
}
