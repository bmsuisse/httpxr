use log::{Metadata, Record};
use pyo3::prelude::*;

/// A simple wrapper around pyo3::Python::with_gil
pub struct Python;

impl Python {
    pub fn attach<F, T>(f: F) -> T
    where
        F: FnOnce(pyo3::Python<'_>) -> T,
    {
        pyo3::Python::attach(f)
    }
}

struct PyLogger;

impl log::Log for PyLogger {
    fn enabled(&self, _metadata: &Metadata) -> bool {
        true
    }

    fn log(&self, record: &Record) {
        if self.enabled(record.metadata()) {
            Python::attach(|py| {
                let logging = match py.import("logging") {
                    Ok(l) => l,
                    Err(_) => return,
                };

                let level = match record.level() {
                    log::Level::Error => 40,
                    log::Level::Warn => 30,
                    log::Level::Info => 20,
                    log::Level::Debug => 10,
                    log::Level::Trace => 0,
                };

                let logger_name = record.target();
                let logger_name = if logger_name.starts_with("httpr") {
                    "httpr"
                } else {
                    logger_name
                };

                let logger = match logging.call_method1("getLogger", (logger_name,)) {
                    Ok(l) => l,
                    Err(_) => return,
                };

                let msg = format!("{}", record.args());
                let _ = logger.call_method1("log", (level, msg));
            });
        }
    }

    fn flush(&self) {}
}

pub fn init() -> Result<(), log::SetLoggerError> {
    log::set_logger(&PyLogger).map(|()| log::set_max_level(log::LevelFilter::Trace))
}
