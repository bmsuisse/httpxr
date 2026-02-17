use pyo3::prelude::*;
use pyo3::types::PyDict;

use crate::models::Response;
use crate::models::{Headers, Request};
use crate::transports::default::HTTPTransport;
use crate::urls::URL;

/// Top-level convenience functions that create a temporary Client.

fn extract_from_kwargs<'py>(
    kwargs: Option<&Bound<'py, PyDict>>,
    key: &str,
) -> Option<Bound<'py, PyAny>> {
    kwargs.and_then(|d| d.get_item(key).ok().flatten())
}

#[pyfunction]
#[pyo3(signature = (method, url, *, params=None, content=None, data=None, files=None, json=None, headers=None, cookies=None, auth=None, proxy=None, follow_redirects=true, verify=None, timeout=None))]
pub fn request(
    py: Python<'_>,
    method: &str,
    url: &Bound<'_, PyAny>,
    params: Option<&Bound<'_, PyAny>>,
    content: Option<&Bound<'_, PyAny>>,
    data: Option<&Bound<'_, PyAny>>,
    files: Option<&Bound<'_, PyAny>>,
    json: Option<&Bound<'_, PyAny>>,
    headers: Option<&Bound<'_, PyAny>>,
    cookies: Option<&Bound<'_, PyAny>>,
    auth: Option<&Bound<'_, PyAny>>,
    proxy: Option<&crate::config::Proxy>,
    follow_redirects: bool,
    verify: Option<&Bound<'_, PyAny>>,
    timeout: Option<&Bound<'_, PyAny>>,
) -> PyResult<Response> {
    let _ = params;
    let _ = files;
    let _ = cookies;
    let _ = auth;
    let _ = follow_redirects;
    // Direct implementation using transport
    let (verify_bool, verify_path) = if let Some(v_bound) = verify {
        if let Ok(b) = v_bound.extract::<bool>() {
            (b, None)
        } else if let Ok(s) = v_bound.extract::<String>() {
            (true, Some(s))
        } else {
            // Check for _cafile attribute (added by our create_ssl_context patch)
            let cafile = v_bound
                .getattr("_cafile")
                .ok()
                .and_then(|attr| attr.extract::<String>().ok());
            (true, cafile)
        }
    } else {
        (true, None)
    };

    let transport =
        HTTPTransport::create(verify_bool, verify_path.as_deref(), false, None, proxy, 0)?;
    let url_str = url.str()?.extract::<String>()?;
    let target_url = URL::create_from_str(&url_str)?;

    // Validate scheme early – tests expect UnsupportedProtocol, not NetworkError
    if let Some(pos) = url_str.find("://") {
        let scheme = &url_str[..pos].to_lowercase();
        if scheme != "http" && scheme != "https" {
            return Err(crate::exceptions::UnsupportedProtocol::new_err(format!(
                "Request URL has an unsupported protocol '{}://'.",
                scheme
            )));
        }
    }

    let mut hdrs = Headers::create(headers, "utf-8")?;

    // Build body
    let body = if let Some(c) = content {
        if let Ok(b) = c.cast::<pyo3::types::PyBytes>() {
            Some(b.as_bytes().to_vec())
        } else if let Ok(s) = c.extract::<String>() {
            Some(s.into_bytes())
        } else {
            None
        }
    } else if let Some(j) = json {
        let json_mod = py.import("json")?;
        let s: String = json_mod.call_method1("dumps", (j,))?.extract()?;
        hdrs.set_header("content-type", "application/json");
        Some(s.into_bytes())
    } else if let Some(d) = data {
        let urllib = py.import("urllib.parse")?;
        let s: String = urllib.call_method1("urlencode", (d, true))?.extract()?;
        hdrs.set_header("content-type", "application/x-www-form-urlencoded");
        Some(s.into_bytes())
    } else {
        None
    };

    let req = Request {
        method: method.to_uppercase(),
        url: target_url,
        headers: Py::new(py, hdrs)?,
        extensions: PyDict::new(py).into(),
        content_body: body,
        stream: None,
        stream_response: false,
    };

    if let Some(t) = timeout {
        req.extensions.bind(py).set_item("timeout", t)?;
    }

    let start = std::time::Instant::now();
    let mut response = transport.send_request(py, &req)?;
    response.elapsed = Some(start.elapsed().as_secs_f64());
    response.request = Some(req);

    Ok(response)
}

#[pyfunction]
#[pyo3(signature = (url, **kwargs))]
pub fn get(
    py: Python<'_>,
    url: &Bound<'_, PyAny>,
    kwargs: Option<&Bound<'_, PyDict>>,
) -> PyResult<Response> {
    let verify = extract_from_kwargs(kwargs, "verify");
    let timeout = extract_from_kwargs(kwargs, "timeout");
    let follow = extract_from_kwargs(kwargs, "follow_redirects")
        .and_then(|v| v.extract::<bool>().ok())
        .unwrap_or(true);

    request(
        py,
        "GET",
        url,
        None,
        None,
        None,
        None,
        None,
        None,
        None,
        None,
        None,
        follow,
        verify.as_ref(),
        timeout.as_ref(),
    )
}

#[pyfunction]
#[pyo3(signature = (url, **kwargs))]
pub fn head(
    py: Python<'_>,
    url: &Bound<'_, PyAny>,
    kwargs: Option<&Bound<'_, PyDict>>,
) -> PyResult<Response> {
    let verify = extract_from_kwargs(kwargs, "verify");
    let timeout = extract_from_kwargs(kwargs, "timeout");
    let follow = extract_from_kwargs(kwargs, "follow_redirects")
        .and_then(|v| v.extract::<bool>().ok())
        .unwrap_or(true);
    request(
        py,
        "HEAD",
        url,
        None,
        None,
        None,
        None,
        None,
        None,
        None,
        None,
        None,
        follow,
        verify.as_ref(),
        timeout.as_ref(),
    )
}

#[pyfunction]
#[pyo3(signature = (url, **kwargs))]
pub fn options(
    py: Python<'_>,
    url: &Bound<'_, PyAny>,
    kwargs: Option<&Bound<'_, PyDict>>,
) -> PyResult<Response> {
    let verify = extract_from_kwargs(kwargs, "verify");
    let timeout = extract_from_kwargs(kwargs, "timeout");
    request(
        py,
        "OPTIONS",
        url,
        None,
        None,
        None,
        None,
        None,
        None,
        None,
        None,
        None,
        true,
        verify.as_ref(),
        timeout.as_ref(),
    )
}

#[pyfunction]
#[pyo3(signature = (url, *, content=None, data=None, files=None, json=None, **kwargs))]
pub fn post(
    py: Python<'_>,
    url: &Bound<'_, PyAny>,
    content: Option<&Bound<'_, PyAny>>,
    data: Option<&Bound<'_, PyAny>>,
    files: Option<&Bound<'_, PyAny>>,
    json: Option<&Bound<'_, PyAny>>,
    kwargs: Option<&Bound<'_, PyDict>>,
) -> PyResult<Response> {
    let verify = extract_from_kwargs(kwargs, "verify");
    let timeout = extract_from_kwargs(kwargs, "timeout");
    request(
        py,
        "POST",
        url,
        None,
        content,
        data,
        files,
        json,
        None,
        None,
        None,
        None,
        true,
        verify.as_ref(),
        timeout.as_ref(),
    )
}

#[pyfunction]
#[pyo3(signature = (url, *, content=None, data=None, files=None, json=None, **kwargs))]
pub fn put(
    py: Python<'_>,
    url: &Bound<'_, PyAny>,
    content: Option<&Bound<'_, PyAny>>,
    data: Option<&Bound<'_, PyAny>>,
    files: Option<&Bound<'_, PyAny>>,
    json: Option<&Bound<'_, PyAny>>,
    kwargs: Option<&Bound<'_, PyDict>>,
) -> PyResult<Response> {
    let verify = extract_from_kwargs(kwargs, "verify");
    let timeout = extract_from_kwargs(kwargs, "timeout");
    request(
        py,
        "PUT",
        url,
        None,
        content,
        data,
        files,
        json,
        None,
        None,
        None,
        None,
        true,
        verify.as_ref(),
        timeout.as_ref(),
    )
}

#[pyfunction]
#[pyo3(signature = (url, *, content=None, data=None, files=None, json=None, **kwargs))]
pub fn patch(
    py: Python<'_>,
    url: &Bound<'_, PyAny>,
    content: Option<&Bound<'_, PyAny>>,
    data: Option<&Bound<'_, PyAny>>,
    files: Option<&Bound<'_, PyAny>>,
    json: Option<&Bound<'_, PyAny>>,
    kwargs: Option<&Bound<'_, PyDict>>,
) -> PyResult<Response> {
    let verify = extract_from_kwargs(kwargs, "verify");
    let timeout = extract_from_kwargs(kwargs, "timeout");
    request(
        py,
        "PATCH",
        url,
        None,
        content,
        data,
        files,
        json,
        None,
        None,
        None,
        None,
        true,
        verify.as_ref(),
        timeout.as_ref(),
    )
}

#[pyfunction]
#[pyo3(signature = (url, **kwargs))]
pub fn delete(
    py: Python<'_>,
    url: &Bound<'_, PyAny>,
    kwargs: Option<&Bound<'_, PyDict>>,
) -> PyResult<Response> {
    let verify = extract_from_kwargs(kwargs, "verify");
    let timeout = extract_from_kwargs(kwargs, "timeout");
    request(
        py,
        "DELETE",
        url,
        None,
        None,
        None,
        None,
        None,
        None,
        None,
        None,
        None,
        true,
        verify.as_ref(),
        timeout.as_ref(),
    )
}

#[pyfunction]
#[pyo3(signature = (method, url, *, params=None, content=None, data=None, files=None, json=None, headers=None, cookies=None, auth=None, proxy=None, follow_redirects=true, verify=None, timeout=None))]
pub fn stream(
    py: Python<'_>,
    method: &str,
    url: &Bound<'_, PyAny>,
    params: Option<&Bound<'_, PyAny>>,
    content: Option<&Bound<'_, PyAny>>,
    data: Option<&Bound<'_, PyAny>>,
    files: Option<&Bound<'_, PyAny>>,
    json: Option<&Bound<'_, PyAny>>,
    headers: Option<&Bound<'_, PyAny>>,
    cookies: Option<&Bound<'_, PyAny>>,
    auth: Option<&Bound<'_, PyAny>>,
    proxy: Option<&crate::config::Proxy>,
    follow_redirects: bool,
    verify: Option<&Bound<'_, PyAny>>,
    timeout: Option<&Bound<'_, PyAny>>,
) -> PyResult<Response> {
    request(
        py,
        method,
        url,
        params,
        content,
        data,
        files,
        json,
        headers,
        cookies,
        auth,
        proxy,
        follow_redirects,
        verify,
        timeout,
    )
}

pub fn register(m: &Bound<'_, PyModule>) -> PyResult<()> {
    m.add_function(wrap_pyfunction!(request, m)?)?;
    m.add_function(wrap_pyfunction!(get, m)?)?;
    m.add_function(wrap_pyfunction!(head, m)?)?;
    m.add_function(wrap_pyfunction!(options, m)?)?;
    m.add_function(wrap_pyfunction!(post, m)?)?;
    m.add_function(wrap_pyfunction!(put, m)?)?;
    m.add_function(wrap_pyfunction!(patch, m)?)?;
    m.add_function(wrap_pyfunction!(delete, m)?)?;
    m.add_function(wrap_pyfunction!(stream, m)?)?;
    m.add_function(wrap_pyfunction!(fetch_all, m)?)?;
    Ok(())
}

/// Fetch multiple URLs concurrently and return a list of Responses.
///
/// Uses a thread pool with configurable concurrency.
#[pyfunction]
#[pyo3(signature = (urls, *, concurrency=50, headers=None, verify=None, timeout=None))]
pub fn fetch_all(
    py: Python<'_>,
    urls: Vec<String>,
    concurrency: usize,
    headers: Option<&Bound<'_, PyAny>>,
    verify: Option<&Bound<'_, PyAny>>,
    timeout: Option<&Bound<'_, PyAny>>,
) -> PyResult<Py<PyAny>> {
    use std::sync::Arc;

    let _ = timeout; // TODO: wire timeout through

    if urls.is_empty() {
        return Ok(pyo3::types::PyList::empty(py).into());
    }

    let (verify_bool, verify_path) = if let Some(v_bound) = verify {
        if let Ok(b) = v_bound.extract::<bool>() {
            (b, None)
        } else if let Ok(s) = v_bound.extract::<String>() {
            (true, Some(s))
        } else {
            let cafile = v_bound
                .getattr("_cafile")
                .ok()
                .and_then(|attr| attr.extract::<String>().ok());
            (true, cafile)
        }
    } else {
        (true, None)
    };

    let hdrs = Headers::create(headers, "utf-8")?;
    let header_pairs: Vec<(String, String)> = {
        let items = hdrs.get_multi_items();
        items.into_iter().collect()
    };

    let concurrency = concurrency.max(1).min(urls.len());

    // Collect response data (status + body bytes) outside Python
    let url_count = urls.len();
    let vp = verify_path.clone();
    let hp = header_pairs.clone();

    // Execute all requests with GIL released for max throughput
    let responses: Vec<Result<Response, String>> = py.detach(|| {
        use std::sync::mpsc;
        use std::thread;

        // Channel-based semaphore for concurrency control
        let (sem_tx, sem_rx) = mpsc::sync_channel::<()>(concurrency);
        for _ in 0..concurrency {
            let _ = sem_tx.send(());
        }
        let sem_rx = Arc::new(std::sync::Mutex::new(sem_rx));

        let mut handles = Vec::with_capacity(url_count);
        let results: Arc<std::sync::Mutex<Vec<(usize, Result<Response, String>)>>> =
            Arc::new(std::sync::Mutex::new(Vec::with_capacity(url_count)));

        for (idx, url_str) in urls.into_iter().enumerate() {
            let sem_rx = sem_rx.clone();
            let sem_tx = sem_tx.clone();
            let results = results.clone();
            let vb = verify_bool;
            let vp = vp.clone();
            let hp = hp.clone();

            let handle = thread::spawn(move || {
                // Acquire semaphore slot
                let _ = sem_rx.lock().unwrap().recv();

                let result = Python::attach(|py| {
                    let transport = HTTPTransport::create(vb, vp.as_deref(), false, None, None, 0)
                        .map_err(|e| format!("Transport error: {}", e))?;

                    let target_url =
                        URL::create_from_str(&url_str).map_err(|e| format!("URL error: {}", e))?;

                    let mut merged_headers = Headers::create(None, "utf-8")
                        .map_err(|e| format!("Header error: {}", e))?;
                    for (k, v) in &hp {
                        merged_headers.set_header(k, v);
                    }
                    if !merged_headers.contains_header("host") {
                        merged_headers.set_header("host", target_url.get_raw_host());
                    }
                    if !merged_headers.contains_header("accept") {
                        merged_headers.set_header("accept", "*/*");
                    }
                    if !merged_headers.contains_header("accept-encoding") {
                        merged_headers.set_header("accept-encoding", "gzip, deflate, br, zstd");
                    }
                    if !merged_headers.contains_header("user-agent") {
                        merged_headers.set_header(
                            "user-agent",
                            &format!("python-httpxr/{}", env!("CARGO_PKG_VERSION")),
                        );
                    }

                    let req = Request {
                        method: "GET".to_string(),
                        url: target_url,
                        headers: Py::new(py, merged_headers).map_err(|e| format!("{}", e))?,
                        extensions: pyo3::types::PyDict::new(py).into(),
                        content_body: None,
                        stream: None,
                        stream_response: false,
                    };

                    let start = std::time::Instant::now();
                    let mut resp = transport
                        .send_request(py, &req)
                        .map_err(|e| format!("Request error: {}", e))?;
                    resp.elapsed = Some(start.elapsed().as_secs_f64());
                    resp.request = Some(req);
                    Ok(resp)
                });

                results.lock().unwrap().push((idx, result));
                // Release semaphore slot
                let _ = sem_tx.send(());
            });
            handles.push(handle);
        }

        for handle in handles {
            let _ = handle.join();
        }

        let mut results_vec = results.lock().unwrap().drain(..).collect::<Vec<_>>();
        results_vec.sort_by_key(|(idx, _)| *idx);
        results_vec.into_iter().map(|(_, r)| r).collect()
    });

    // Return list of Responses
    let py_list = pyo3::types::PyList::empty(py);
    for result in responses {
        match result {
            Ok(resp) => {
                py_list.append(Py::new(py, resp)?)?;
            }
            Err(msg) => {
                return Err(pyo3::exceptions::PyRuntimeError::new_err(msg));
            }
        }
    }
    Ok(py_list.into())
}
