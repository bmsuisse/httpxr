#[global_allocator]
static GLOBAL: mimalloc::MiMalloc = mimalloc::MiMalloc;

use pyo3::prelude::*;

mod api;
mod auth;
mod client;
mod config;
mod content;
mod decoders;
mod exceptions;
mod models;
mod multipart;
mod status_codes;
mod stream_ctx;
mod transports;
mod types;
mod urlparse;
mod urls;
mod utils;

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
