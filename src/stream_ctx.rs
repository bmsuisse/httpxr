use pyo3::prelude::*;
use pyo3::types::{PyAnyMethods, PyDictMethods, PyDict};

use crate::client::AsyncClient;
use crate::models::Response;

#[pyclass]
pub struct StreamContextManager {
    pub(crate) client: Py<AsyncClient>,
    pub(crate) method: String,
    pub(crate) url: Py<PyAny>,
    pub(crate) kwargs: Option<Py<PyDict>>,
    pub(crate) response: Option<Py<Response>>,
}

#[pymethods]
impl StreamContextManager {
    fn __aenter__<'py>(slf: Bound<'py, Self>, py: Python<'py>) -> PyResult<Bound<'py, PyAny>> {
        let slf_borrow = slf.borrow();
        let client = slf_borrow.client.clone_ref(py);
        let method = slf_borrow.method.clone();
        let url = slf_borrow.url.clone_ref(py);
        let kwargs = slf_borrow.kwargs.as_ref().map(|k| k.clone_ref(py));
        
        let slf_py = slf.clone().unbind();

        pyo3_async_runtimes::tokio::future_into_py(py, async move {
            // First, call client.request(..., stream=true) to get the coroutine
            let coro_future = Python::attach(|py| -> PyResult<_> {
                let c_bound = client.bind(py);
                let u_bound = url.bind(py);
                let k_bound = kwargs.as_ref().map(|k| k.bind(py));
                
                let kwargs_dict: Py<PyDict> = k_bound.map(|k| k.to_owned().unbind()).unwrap_or_else(|| PyDict::new(py).unbind());
                kwargs_dict.bind(py).set_item("stream", true)?;
                
                let coro = c_bound.call_method("request", (&method, u_bound), Some(kwargs_dict.bind(py)))?;
                // Convert the coroutine to a Rust future
                pyo3_async_runtimes::tokio::into_future(coro)
            })?;
            
            // Await the coroutine to get the response
            let resp_py = coro_future.await?;
            
            // Extract the Response and store it
            Python::attach(|py| -> PyResult<Py<Response>> {
                let resp = resp_py.bind(py).extract::<Response>()?;
                let slf_bound = slf_py.bind(py);
                let mut slf_mut = slf_bound.borrow_mut();
                let resp_py = Py::new(py, resp.clone())?;
                slf_mut.response = Some(resp_py.clone_ref(py));
                Ok(resp_py)
            })
        })
    }

    fn __aexit__<'py>(
        &mut self,
        py: Python<'py>,
        _exc_type: Option<&Bound<'_, PyAny>>,
        _exc_value: Option<&Bound<'_, PyAny>>,
        _traceback: Option<&Bound<'_, PyAny>>,
    ) -> PyResult<Bound<'py, PyAny>> {
        if let Some(resp) = &self.response {
             let r = resp.clone_ref(py);
             pyo3_async_runtimes::tokio::future_into_py(py, async move {
                 let _ = Python::attach(|py| {
                     r.call_method0(py, "aclose")
                 }); // Ignore errors on close?
                 Ok(())
             })
        } else {
            pyo3_async_runtimes::tokio::future_into_py(py, async move { Ok(()) })
        }
    }
}
