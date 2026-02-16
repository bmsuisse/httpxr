use pyo3::prelude::*;

/// MockTransport: transport for testing that calls a user-provided handler.
#[pyclass(subclass, extends=super::base::BaseTransport)]
#[pyo3(dict)]
pub struct MockTransport {
    #[pyo3(get, set)]
    handler: Py<PyAny>,
}

#[pymethods]
impl MockTransport {
    #[new]
    fn new(handler: Py<PyAny>) -> (Self, super::base::BaseTransport) {
        (MockTransport { handler }, super::base::BaseTransport)
    }

    fn handle_request(&self, py: Python<'_>, request: crate::models::Request) -> PyResult<crate::models::Response> {
        let result = self.handler.call1(py, (request,))?;
        let bound = result.bind(py);
        let response: crate::models::Response = bound.downcast::<crate::models::Response>()?.borrow().clone();
        Ok(response)
    }

    fn handle_async_request<'py>(
        &self,
        py: Python<'py>,
        request: crate::models::Request,
    ) -> PyResult<Bound<'py, PyAny>> {
        // Call the handler, which might be sync or async
        let result = self.handler.call1(py, (request,))?;
        // If result is awaitable, return it; otherwise wrap in a completed future
        if result.bind(py).hasattr("__await__")? {
            Ok(result.into_bound(py))
        } else {
            // Wrap sync result in an async-compatible form
            let _handler = self.handler.clone_ref(py);
            pyo3_async_runtimes::tokio::future_into_py(py, async move {
                Python::attach(|py| {
                    let bound = result.bind(py);
                    let response: crate::models::Response = bound.downcast::<crate::models::Response>()?.borrow().clone();
                    Ok(response)
                })
            })
        }
    }

    #[getter]
    fn get_handler(&self, py: Python<'_>) -> Py<PyAny> {
        self.handler.clone_ref(py)
    }

    fn close(&self) -> PyResult<()> { Ok(()) }
    fn aclose(&self) -> PyResult<()> { Ok(()) }

    fn __enter__(slf: PyRef<'_, Self>) -> PyRef<'_, Self> { slf }
    fn __exit__(&self, _e1: Option<&Bound<'_, PyAny>>, _e2: Option<&Bound<'_, PyAny>>, _e3: Option<&Bound<'_, PyAny>>) -> PyResult<()> {
        self.close()
    }

    fn __aenter__<'py>(slf: Bound<'py, Self>, py: Python<'py>) -> PyResult<Bound<'py, PyAny>> {
        let slf_py = slf.unbind();
        pyo3_async_runtimes::tokio::future_into_py(py, async move {
            Ok(slf_py)
        })
    }

    fn __aexit__<'py>(
        _slf: Bound<'py, Self>,
        py: Python<'py>,
        _exc_type: Option<Bound<'py, PyAny>>,
        _exc_value: Option<Bound<'py, PyAny>>,
        _traceback: Option<Bound<'py, PyAny>>,
    ) -> PyResult<Bound<'py, PyAny>> {
        pyo3_async_runtimes::tokio::future_into_py(py, async move {
             Ok(())
        })
    }
}

/// AsyncMockTransport: async mock transport.
#[pyclass(extends=super::base::AsyncBaseTransport)]
#[pyo3(dict)]
pub struct AsyncMockTransport {
    #[pyo3(get, set)]
    handler: Py<PyAny>,
}

#[pymethods]
impl AsyncMockTransport {
    #[new]
    fn new(handler: Py<PyAny>) -> (Self, super::base::AsyncBaseTransport) {
        (AsyncMockTransport { handler }, super::base::AsyncBaseTransport)
    }

    fn handle_async_request<'py>(
        &self,
        py: Python<'py>,
        request: crate::models::Request,
    ) -> PyResult<Bound<'py, PyAny>> {
        // Call the handler, which might be sync or async
        let result = self.handler.call1(py, (request,))?;
        // If result is awaitable, return it; otherwise wrap
        if result.bind(py).hasattr("__await__")? {
            Ok(result.into_bound(py))
        } else {
            // Wrap sync result in a coroutine
            let asyncio = py.import("asyncio")?;
            let coro = asyncio.call_method1("coroutine", (result,))?;
            Ok(coro)
        }
    }
}

pub fn register(m: &Bound<'_, PyModule>) -> PyResult<()> {
    m.add_class::<MockTransport>()?;
    m.add_class::<AsyncMockTransport>()?;
    Ok(())
}
