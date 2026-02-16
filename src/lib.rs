use pyo3::prelude::*;

mod exceptions;
mod status_codes;
mod types;
mod urlparse;
mod urls;
mod utils;
mod models;
mod decoders;
mod content;
mod multipart;
mod config;
mod auth;
mod transports;
mod stream_ctx;
mod client;
mod api;

/// The native Rust extension module for httpr.
pub mod logger;
pub use logger::Python as PythonHelper;

#[pymodule]
fn _httpr(m: &Bound<'_, PyModule>) -> PyResult<()> {
    // Initialize logging
    let _ = logger::init();
    
    m.add("__version__", env!("CARGO_PKG_VERSION"))?;
    // Register submodules
    exceptions::register(m)?;
    status_codes::register(m)?;
    types::register(m)?;
    utils::register(m)?;
    urlparse::register(m)?;
    urls::register(m)?;
    models::register(m)?;
    decoders::register(m)?;
    content::register(m)?;
    multipart::register(m)?;
    config::register(m)?;
    auth::register(m)?;
    transports::register(m)?;
    client::register(m)?;
    api::register(m)?;
    Ok(())
}
