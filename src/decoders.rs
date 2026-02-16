use flate2::read::{DeflateDecoder as FlateDeflateDecoder, GzDecoder};
use pyo3::prelude::*;
use pyo3::types::PyBytes;
use std::io::Read;

/// Content decoder trait — implemented by each compression decoder.
trait ContentDecoder {
    fn decode(&mut self, data: &[u8]) -> PyResult<Vec<u8>>;
    fn flush(&mut self) -> PyResult<Vec<u8>>;
}

struct IdentityDecoder;

impl ContentDecoder for IdentityDecoder {
    fn decode(&mut self, data: &[u8]) -> PyResult<Vec<u8>> {
        Ok(data.to_vec())
    }
    fn flush(&mut self) -> PyResult<Vec<u8>> {
        Ok(Vec::new())
    }
}

struct GZipDecoder {
    buffer: Vec<u8>,
}

impl GZipDecoder {
    fn new() -> Self {
        GZipDecoder { buffer: Vec::new() }
    }
}

impl ContentDecoder for GZipDecoder {
    fn decode(&mut self, data: &[u8]) -> PyResult<Vec<u8>> {
        self.buffer.extend_from_slice(data);
        let mut decoder = GzDecoder::new(&self.buffer[..]);
        let mut output = Vec::new();
        match decoder.read_to_end(&mut output) {
            Ok(_) => {
                self.buffer.clear();
                Ok(output)
            }
            Err(_) => {
                // Not enough data yet, wait for more
                Ok(Vec::new())
            }
        }
    }
    fn flush(&mut self) -> PyResult<Vec<u8>> {
        if self.buffer.is_empty() {
            return Ok(Vec::new());
        }
        let mut decoder = GzDecoder::new(&self.buffer[..]);
        let mut output = Vec::new();
        decoder.read_to_end(&mut output).map_err(|e| {
            crate::exceptions::DecodingError::new_err(format!("GZip decoding error: {}", e))
        })?;
        self.buffer.clear();
        Ok(output)
    }
}

struct DeflateDecoder {
    buffer: Vec<u8>,
}

impl DeflateDecoder {
    fn new() -> Self {
        DeflateDecoder { buffer: Vec::new() }
    }
}

impl ContentDecoder for DeflateDecoder {
    fn decode(&mut self, data: &[u8]) -> PyResult<Vec<u8>> {
        self.buffer.extend_from_slice(data);
        let mut decoder = FlateDeflateDecoder::new(&self.buffer[..]);
        let mut output = Vec::new();
        match decoder.read_to_end(&mut output) {
            Ok(_) => {
                self.buffer.clear();
                Ok(output)
            }
            Err(_) => Ok(Vec::new()),
        }
    }
    fn flush(&mut self) -> PyResult<Vec<u8>> {
        if self.buffer.is_empty() {
            return Ok(Vec::new());
        }
        let mut decoder = FlateDeflateDecoder::new(&self.buffer[..]);
        let mut output = Vec::new();
        decoder.read_to_end(&mut output).map_err(|e| {
            crate::exceptions::DecodingError::new_err(format!("Deflate decoding error: {}", e))
        })?;
        self.buffer.clear();
        Ok(output)
    }
}

/// Python-exposed decoder wrapper
#[pyclass]
pub struct PyDecoder {
    inner: Box<dyn ContentDecoder + Send + Sync>,
    #[allow(dead_code)]
    encoding: String,
}

#[pymethods]
impl PyDecoder {
    #[new]
    fn new(encoding: &str) -> PyResult<Self> {
        let inner: Box<dyn ContentDecoder + Send + Sync> = match encoding {
            "gzip" => Box::new(GZipDecoder::new()),
            "deflate" => Box::new(DeflateDecoder::new()),
            "identity" | "" => Box::new(IdentityDecoder),
            "br" => {
                // Try to use brotli via Python fallback
                return Err(pyo3::exceptions::PyValueError::new_err(
                    "Brotli support requires the 'brotli' Python package. Use: pip install brotli",
                ));
            }
            "zstd" => {
                return Err(pyo3::exceptions::PyValueError::new_err(
                    "Zstandard support requires the 'zstandard' Python package. Use: pip install zstandard",
                ));
            }
            _ => {
                return Err(pyo3::exceptions::PyValueError::new_err(format!(
                    "Unknown encoding: {}",
                    encoding
                )));
            }
        };
        Ok(PyDecoder {
            inner,
            encoding: encoding.to_string(),
        })
    }

    fn decode(&mut self, py: Python<'_>, data: &[u8]) -> PyResult<Py<PyAny>> {
        let result = self.inner.decode(data)?;
        Ok(PyBytes::new(py, &result).into())
    }

    fn flush(&mut self, py: Python<'_>) -> PyResult<Py<PyAny>> {
        let result = self.inner.flush()?;
        Ok(PyBytes::new(py, &result).into())
    }
}

/// ByteChunker: yield fixed-size chunks of bytes
#[pyclass]
pub struct ByteChunker {
    chunk_size: usize,
    buffer: Vec<u8>,
}

#[pymethods]
impl ByteChunker {
    #[new]
    #[pyo3(signature = (chunk_size=None))]
    fn new(chunk_size: Option<usize>) -> Self {
        ByteChunker {
            chunk_size: chunk_size.unwrap_or(65536),
            buffer: Vec::new(),
        }
    }

    fn decode(&mut self, py: Python<'_>, data: &[u8]) -> PyResult<Py<PyAny>> {
        self.buffer.extend_from_slice(data);
        let list = pyo3::types::PyList::empty(py);
        while self.buffer.len() >= self.chunk_size {
            let chunk: Vec<u8> = self.buffer.drain(..self.chunk_size).collect();
            list.append(PyBytes::new(py, &chunk))?;
        }
        Ok(list.into())
    }

    fn flush(&mut self, py: Python<'_>) -> PyResult<Py<PyAny>> {
        if self.buffer.is_empty() {
            Ok(PyBytes::new(py, b"").into())
        } else {
            let data = std::mem::take(&mut self.buffer);
            Ok(PyBytes::new(py, &data).into())
        }
    }
}

/// LineDecoder: split bytes into lines
#[pyclass]
pub struct LineDecoder {
    buffer: String,
}

#[pymethods]
impl LineDecoder {
    #[new]
    fn new() -> Self {
        LineDecoder {
            buffer: String::new(),
        }
    }

    fn decode(&mut self, data: &str) -> Vec<String> {
        self.buffer.push_str(data);
        let mut lines = Vec::new();
        loop {
            // Find the earliest line ending: \r\n, \r, or \n
            let crlf_pos = self.buffer.find("\r\n");
            let cr_pos = self.buffer.find('\r');
            let lf_pos = self.buffer.find('\n');

            // Determine which line ending comes first
            let (end_pos, skip_len) = match (crlf_pos, cr_pos, lf_pos) {
                (Some(crlf), _, _) if lf_pos.is_none() || crlf + 1 == lf_pos.unwrap() => {
                    // \r\n found and it's the \r\n pair (not a standalone \r before a later \n)
                    (crlf + 2, crlf + 2)
                }
                (_, Some(cr), Some(lf)) if cr < lf => {
                    // Check if this \r is part of \r\n
                    if cr + 1 == lf {
                        (cr + 2, cr + 2) // \r\n
                    } else {
                        (cr + 1, cr + 1) // standalone \r
                    }
                }
                (_, Some(cr), None) => (cr + 1, cr + 1), // standalone \r
                (_, _, Some(lf)) => (lf + 1, lf + 1),    // standalone \n
                _ => break,                              // no line ending found
            };

            let line = self.buffer[..end_pos].to_string();
            self.buffer = self.buffer[skip_len..].to_string();
            lines.push(line);
        }
        lines
    }

    fn flush(&mut self) -> Vec<String> {
        if self.buffer.is_empty() {
            Vec::new()
        } else {
            vec![std::mem::take(&mut self.buffer)]
        }
    }
}

pub fn register(m: &Bound<'_, PyModule>) -> PyResult<()> {
    m.add_class::<PyDecoder>()?;
    m.add_class::<ByteChunker>()?;
    m.add_class::<LineDecoder>()?;
    Ok(())
}
