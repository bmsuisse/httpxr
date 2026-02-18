pub mod base;
pub mod default;
pub mod helpers;
pub mod mock;

use pyo3::prelude::*;

pub fn register(m: &Bound<'_, PyModule>) -> PyResult<()> {
    base::register(m)?;
    default::register(m)?;
    mock::register(m)?;
    Ok(())
}
