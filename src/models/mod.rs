pub mod cookies;
pub mod headers;
pub mod request;
pub mod response;
mod response_streaming;

use pyo3::prelude::*;

pub use cookies::Cookies;
pub use headers::Headers;
pub use request::Request;
pub use response::Response;

pub fn register(m: &Bound<'_, PyModule>) -> PyResult<()> {
    m.add_class::<Headers>()?;
    m.add_class::<Request>()?;
    m.add_class::<Response>()?;
    m.add_class::<Cookies>()?;
    Ok(())
}
