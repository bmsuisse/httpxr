pub mod async_client;
pub mod common;
pub mod sync_client;

use pyo3::prelude::*;

pub use async_client::AsyncClient;
pub use common::UseClientDefault;
pub use sync_client::Client;

pub fn register(m: &Bound<'_, PyModule>) -> PyResult<()> {
    m.add_class::<UseClientDefault>()?;
    m.add_class::<Client>()?;
    m.add_class::<AsyncClient>()?;
    m.add_class::<crate::stream_ctx::StreamContextManager>()?;
    Ok(())
}
