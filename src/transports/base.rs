use pyo3::prelude::*;
use pyo3::types::{PyDict, PyTuple};

/// BaseTransport: sync transport base class.
#[pyclass(from_py_object, subclass)]
#[derive(Clone)]
pub struct BaseTransport;

#[pymethods]
impl BaseTransport {
    #[new]
    #[pyo3(signature = (*_args, **_kwargs))]
    fn new(_args: &Bound<'_, PyTuple>, _kwargs: Option<&Bound<'_, PyDict>>) -> Self {
        BaseTransport
    }

    fn __enter__(slf: PyRef<'_, Self>) -> PyRef<'_, Self> {
        slf
    }

    fn __exit__<'py>(
        _slf: Bound<'py, Self>,
        _exc_type: Option<&Bound<'py, PyAny>>,
        _exc_value: Option<&Bound<'py, PyAny>>,
        _traceback: Option<&Bound<'py, PyAny>>,
    ) -> PyResult<()> {
        Ok(())
    }

    fn handle_request(&self, _request: &crate::models::Request) -> PyResult<crate::models::Response> {
        Err(pyo3::exceptions::PyNotImplementedError::new_err(
            "The 'handle_request' method must be implemented.",
        ))
    }

    fn close(&self) -> PyResult<()> {
        Ok(())
    }
}

/// AsyncBaseTransport: async transport base class.
#[pyclass(from_py_object, subclass)]
#[derive(Clone)]
pub struct AsyncBaseTransport;

#[pymethods]
impl AsyncBaseTransport {
    #[new]
    #[pyo3(signature = (*_args, **_kwargs))]
    fn new(_args: &Bound<'_, PyTuple>, _kwargs: Option<&Bound<'_, PyDict>>) -> Self {
        AsyncBaseTransport
    }

    fn __aenter__<'py>(slf: Bound<'py, Self>, py: Python<'py>) -> PyResult<Bound<'py, PyAny>> {
        let slf_py = slf.unbind();
        pyo3_async_runtimes::tokio::future_into_py(py, async move {
            Ok(slf_py)
        })
    }

    fn __aexit__<'py>(
        slf: Bound<'py, Self>,
        py: Python<'py>,
        _exc_type: Option<Bound<'py, PyAny>>,
        _exc_value: Option<Bound<'py, PyAny>>,
        _traceback: Option<Bound<'py, PyAny>>,
    ) -> PyResult<Bound<'py, PyAny>> {
        let slf_py = slf.clone().unbind();
        pyo3_async_runtimes::tokio::future_into_py(py, async move {
             let fut = Python::attach(|py| {
                 let t = slf_py.bind(py);
                 if t.hasattr("aclose")? {
                     let result = t.call_method0("aclose")?;
                     let inspect = py.import("inspect")?;
                     let is_coro = inspect.call_method1("isawaitable", (&result,))?.extract::<bool>()?;
                     if is_coro {
                         return Ok::<_, PyErr>(Some(pyo3_async_runtimes::tokio::into_future(result)?));
                     }
                 }
                 Ok(None)
             })?;
             
             if let Some(f) = fut {
                 f.await?;
             }
             Ok(())
        })
    }

    fn handle_async_request(&self, _request: &crate::models::Request) -> PyResult<crate::models::Response> {
        Err(pyo3::exceptions::PyNotImplementedError::new_err(
            "The 'handle_async_request' method must be implemented.",
        ))
    }

    fn aclose(&self) -> PyResult<()> {
        Ok(())
    }
}

pub fn register(m: &Bound<'_, PyModule>) -> PyResult<()> {
    m.add_class::<BaseTransport>()?;
    m.add_class::<AsyncBaseTransport>()?;
    Ok(())
}
