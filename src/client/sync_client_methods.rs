//! Macro-generated convenience HTTP methods for the synchronous `Client`.
//!
//! Each macro generates a separate `#[pymethods] impl Client` block because
//! PyO3's proc macro does not support macro invocations inside `#[pymethods]`.

use pyo3::prelude::*;
use pyo3::types::PyDict;

use crate::models::Response;

use super::common::extract_from_kwargs;
use super::sync_client::Client;

// ---------------------------------------------------------------------------
// High-level convenience methods (get, post, put, patch, delete, head, options)
// ---------------------------------------------------------------------------

macro_rules! sync_no_body_method {
    ($name:ident, $method:expr) => {
        #[pymethods]
        impl Client {
            #[pyo3(signature = (url, *, params=None, headers=None, cookies=None, follow_redirects=None, timeout=None, extensions=None, **kwargs))]
            fn $name(
                &self,
                py: Python<'_>,
                url: &Bound<'_, PyAny>,
                params: Option<&Bound<'_, PyAny>>,
                headers: Option<&Bound<'_, PyAny>>,
                cookies: Option<&Bound<'_, PyAny>>,
                follow_redirects: Option<bool>,
                timeout: Option<&Bound<'_, PyAny>>,
                extensions: Option<&Bound<'_, PyAny>>,
                kwargs: Option<&Bound<'_, PyDict>>,
            ) -> PyResult<Response> {
                let timeout_kw = extract_from_kwargs(kwargs, "timeout");
                let t = timeout.or_else(|| timeout_kw.as_ref());
                let f = follow_redirects.or_else(|| {
                    extract_from_kwargs(kwargs, "follow_redirects")
                        .and_then(|v| v.extract::<bool>().ok())
                });
                self.request(
                    py, $method, url, None, None, None, None, params, headers,
                    cookies, f, t, extensions, kwargs,
                )
            }
        }
    };
}

macro_rules! sync_body_method {
    ($name:ident, $method:expr) => {
        #[pymethods]
        impl Client {
            #[pyo3(signature = (url, *, content=None, data=None, files=None, json=None, params=None, headers=None, cookies=None, follow_redirects=None, timeout=None, extensions=None, **kwargs))]
            fn $name(
                &self,
                py: Python<'_>,
                url: &Bound<'_, PyAny>,
                content: Option<&Bound<'_, PyAny>>,
                data: Option<&Bound<'_, PyAny>>,
                files: Option<&Bound<'_, PyAny>>,
                json: Option<&Bound<'_, PyAny>>,
                params: Option<&Bound<'_, PyAny>>,
                headers: Option<&Bound<'_, PyAny>>,
                cookies: Option<&Bound<'_, PyAny>>,
                follow_redirects: Option<bool>,
                timeout: Option<&Bound<'_, PyAny>>,
                extensions: Option<&Bound<'_, PyAny>>,
                kwargs: Option<&Bound<'_, PyDict>>,
            ) -> PyResult<Response> {
                let timeout_kw = extract_from_kwargs(kwargs, "timeout");
                let t = timeout.or_else(|| timeout_kw.as_ref());
                let f = follow_redirects.or_else(|| {
                    extract_from_kwargs(kwargs, "follow_redirects")
                        .and_then(|v| v.extract::<bool>().ok())
                });
                self.request(
                    py, $method, url, content, data, files, json, params, headers,
                    cookies, f, t, extensions, kwargs,
                )
            }
        }
    };
}

// ---------------------------------------------------------------------------
// Raw convenience methods (get_raw, post_raw, etc.)
// ---------------------------------------------------------------------------

macro_rules! sync_raw_no_body {
    ($name:ident, $method:expr) => {
        #[pymethods]
        impl Client {
            #[pyo3(signature = (url, *, headers=None, timeout=None))]
            fn $name(&self, py: Python<'_>, url: &str, headers: Option<&Bound<'_, PyDict>>, timeout: Option<f64>) -> PyResult<Py<PyAny>> {
                self._send_raw(py, $method, url, headers, None, timeout)
            }
        }
    };
}

macro_rules! sync_raw_with_body {
    ($name:ident, $method:expr) => {
        #[pymethods]
        impl Client {
            #[pyo3(signature = (url, *, headers=None, body=None, timeout=None))]
            fn $name(&self, py: Python<'_>, url: &str, headers: Option<&Bound<'_, PyDict>>, body: Option<Vec<u8>>, timeout: Option<f64>) -> PyResult<Py<PyAny>> {
                self._send_raw(py, $method, url, headers, body, timeout)
            }
        }
    };
}

// ---------------------------------------------------------------------------
// Invocations
// ---------------------------------------------------------------------------

sync_no_body_method!(get, "GET");
sync_no_body_method!(head, "HEAD");
sync_no_body_method!(options, "OPTIONS");
sync_no_body_method!(delete, "DELETE");
sync_body_method!(post, "POST");
sync_body_method!(put, "PUT");
sync_body_method!(patch, "PATCH");
sync_raw_no_body!(get_raw, reqwest::Method::GET);
sync_raw_with_body!(post_raw, reqwest::Method::POST);
sync_raw_with_body!(put_raw, reqwest::Method::PUT);
sync_raw_with_body!(patch_raw, reqwest::Method::PATCH);
sync_raw_no_body!(delete_raw, reqwest::Method::DELETE);
sync_raw_no_body!(head_raw, reqwest::Method::HEAD);
