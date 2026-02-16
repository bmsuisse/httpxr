use pyo3::prelude::*;
use pyo3::types::PyBytes;

/// AsyncByteStream base class
#[pyclass(subclass)]
pub struct AsyncByteStream;

#[pymethods]
impl AsyncByteStream {
    #[new]
    fn new() -> Self {
        AsyncByteStream
    }

    fn aclose<'py>(&self, py: Python<'py>) -> PyResult<Bound<'py, PyAny>> {
        let asyncio = py.import("asyncio")?;
        let future =
            asyncio.call_method1("ensure_future", (asyncio.call_method1("sleep", (0,))?,))?;
        Ok(future)
    }

    fn __aiter__(slf: PyRef<'_, Self>) -> PyRef<'_, Self> {
        slf
    }

    fn __anext__<'py>(&self, _py: Python<'py>) -> PyResult<Option<Bound<'py, PyAny>>> {
        Ok(None)
    }
}

/// SyncByteStream base class
#[pyclass(extends=AsyncByteStream, subclass)]
pub struct SyncByteStream;

#[pymethods]
impl SyncByteStream {
    #[new]
    fn new() -> PyClassInitializer<Self> {
        PyClassInitializer::from(AsyncByteStream).add_subclass(SyncByteStream)
    }

    fn __iter__(slf: PyRef<'_, Self>) -> PyRef<'_, Self> {
        slf
    }

    fn __next__(&self) -> Option<Py<PyAny>> {
        None
    }

    fn close(&self) -> PyResult<()> {
        Ok(())
    }
}

/// A stream of bytes.
#[pyclass(from_py_object, extends=SyncByteStream)]
#[derive(Clone)]
pub struct ByteStream {
    pub data: Vec<u8>,
    #[allow(dead_code)]
    pub pos: usize,
}

#[pymethods]
impl ByteStream {
    #[new]
    #[pyo3(signature = (data=None))]
    fn new(data: Option<Vec<u8>>) -> PyClassInitializer<Self> {
        PyClassInitializer::from(AsyncByteStream)
            .add_subclass(SyncByteStream)
            .add_subclass(ByteStream {
                data: data.unwrap_or_default(),
                pos: 0,
            })
    }

    fn __iter__(slf: PyRef<'_, Self>) -> ByteStreamIter {
        ByteStreamIter {
            data: slf.data.clone(),
            done: false,
        }
    }

    fn __aiter__(slf: PyRef<'_, Self>) -> ByteStreamAsyncIter {
        ByteStreamAsyncIter {
            data: slf.data.clone(),
            done: false,
        }
    }

    fn __repr__(&self) -> String {
        format!("<ByteStream [{} bytes]>", self.data.len())
    }

    fn close(&self) -> PyResult<()> {
        Ok(())
    }
    fn read(&self) -> Vec<u8> {
        self.data.clone()
    }
}

#[pyclass]
pub struct ByteStreamIter {
    data: Vec<u8>,
    done: bool,
}

#[pymethods]
impl ByteStreamIter {
    fn __iter__(slf: PyRef<'_, Self>) -> PyRef<'_, Self> {
        slf
    }
    fn __next__(&mut self, py: Python<'_>) -> Option<Py<PyAny>> {
        if self.done {
            return None;
        }
        self.done = true;
        Some(PyBytes::new(py, &self.data).into())
    }
}

#[pyclass]
pub struct ByteStreamAsyncIter {
    data: Vec<u8>,
    done: bool,
}

#[pymethods]
impl ByteStreamAsyncIter {
    fn __aiter__(slf: PyRef<'_, Self>) -> PyRef<'_, Self> {
        slf
    }
    fn __anext__<'py>(&mut self, py: Python<'py>) -> PyResult<Option<Bound<'py, PyAny>>> {
        if self.done {
            return Ok(None);
        }
        self.done = true;
        let data = self.data.clone();
        let result = pyo3_async_runtimes::tokio::future_into_py(py, async move { Ok(data) })?;
        Ok(Some(result))
    }
}

/// Iterator for streaming bytes from a blocking reader (e.g. ureq)
#[pyclass(unsendable)]
pub struct BlockingBytesIterator {
    reader: Option<Box<dyn std::io::Read + Send>>,
    buffer_size: usize,
}

impl BlockingBytesIterator {
    pub fn new(reader: Box<dyn std::io::Read + Send>) -> Self {
        BlockingBytesIterator {
            reader: Some(reader),
            buffer_size: 8192, // Default chunk size
        }
    }

    #[allow(dead_code)]
    pub fn take_reader(&mut self) -> Option<Box<dyn std::io::Read + Send>> {
        self.reader.take()
    }
}

#[pymethods]
impl BlockingBytesIterator {
    fn __iter__(slf: PyRef<'_, Self>) -> PyRef<'_, Self> {
        slf
    }

    fn __next__(&mut self, py: Python<'_>) -> PyResult<Option<Py<PyAny>>> {
        if let Some(reader) = &mut self.reader {
            let mut buf = vec![0u8; self.buffer_size];
            // Release GIL during blocking read
            // TODO: Restore allow_threads when pyo3 version compatibility is resolved
            let n = reader.read(&mut buf)?;

            if n == 0 {
                self.reader = None; // Close/Drop reader
                Ok(None)
            } else {
                buf.truncate(n);
                Ok(Some(PyBytes::new(py, &buf).into()))
            }
        } else {
            Ok(None)
        }
    }
}

#[pyclass]
pub struct AsyncResponseStream {
    stream: std::sync::Arc<
        tokio::sync::Mutex<
            Box<dyn futures::Stream<Item = reqwest::Result<bytes::Bytes>> + Send + Unpin>,
        >,
    >,
    #[allow(dead_code)]
    pool_permit: Option<tokio::sync::OwnedSemaphorePermit>,
}

impl AsyncResponseStream {
    pub fn new(
        stream: Box<dyn futures::Stream<Item = reqwest::Result<bytes::Bytes>> + Send + Unpin>,
    ) -> Self {
        AsyncResponseStream {
            stream: std::sync::Arc::new(tokio::sync::Mutex::new(stream)),
            pool_permit: None,
        }
    }

    pub fn new_with_permit(
        stream: Box<dyn futures::Stream<Item = reqwest::Result<bytes::Bytes>> + Send + Unpin>,
        permit: tokio::sync::OwnedSemaphorePermit,
    ) -> Self {
        AsyncResponseStream {
            stream: std::sync::Arc::new(tokio::sync::Mutex::new(stream)),
            pool_permit: Some(permit),
        }
    }
}

#[pymethods]
impl AsyncResponseStream {
    fn __aiter__(slf: PyRef<'_, Self>) -> PyRef<'_, Self> {
        slf
    }

    fn __anext__<'py>(&mut self, py: Python<'py>) -> PyResult<Bound<'py, PyAny>> {
        let stream = self.stream.clone();
        pyo3_async_runtimes::tokio::future_into_py(py, async move {
            let mut s = stream.lock().await;
            match futures::StreamExt::next(&mut *s).await {
                Some(Ok(chunk)) => {
                    pyo3::Python::attach(|py| Ok(PyBytes::new(py, &chunk).into_any().unbind()))
                }
                Some(Err(e)) => Err(pyo3::exceptions::PyIOError::new_err(format!(
                    "Stream error: {}",
                    e
                ))),
                None => Err(pyo3::exceptions::PyStopAsyncIteration::new_err("")),
            }
        })
    }
}

/// Chunked iterator that supports both sync and async iteration over pre-split chunks.
/// Optionally tracks byte counts for `num_bytes_downloaded` on `Response`.
#[pyclass]
pub struct ChunkedIter {
    pub chunks: Vec<Py<PyAny>>,
    pub pos: usize,
    /// Optional: shared counter to increment as chunks are yielded
    pub byte_counter: Option<std::sync::Arc<std::sync::atomic::AtomicUsize>>,
    /// Optional: byte size of each chunk (must be same length as chunks)
    pub chunk_byte_sizes: Option<Vec<usize>>,
}

#[pymethods]
impl ChunkedIter {
    fn __iter__(slf: PyRef<'_, Self>) -> PyRef<'_, Self> {
        slf
    }
    fn __next__(&mut self, py: Python<'_>) -> Option<Py<PyAny>> {
        if self.pos >= self.chunks.len() {
            return None;
        }
        let chunk = self.chunks[self.pos].clone_ref(py);
        if let (Some(counter), Some(sizes)) = (&self.byte_counter, &self.chunk_byte_sizes) {
            counter.fetch_add(sizes[self.pos], std::sync::atomic::Ordering::Relaxed);
        }
        self.pos += 1;
        Some(chunk)
    }
    fn __aiter__(slf: PyRef<'_, Self>) -> PyRef<'_, Self> {
        slf
    }
    fn __anext__<'py>(&mut self, py: Python<'py>) -> PyResult<Option<Bound<'py, PyAny>>> {
        if self.pos >= self.chunks.len() {
            return Ok(None);
        }
        let chunk = self.chunks[self.pos].clone_ref(py);
        if let (Some(counter), Some(sizes)) = (&self.byte_counter, &self.chunk_byte_sizes) {
            counter.fetch_add(sizes[self.pos], std::sync::atomic::Ordering::Relaxed);
        }
        self.pos += 1;
        let result = pyo3_async_runtimes::tokio::future_into_py(py, async move { Ok(chunk) })?;
        Ok(Some(result))
    }
}

/// Wrapper for Python iterators to look like SyncByteStream
#[pyclass(extends=SyncByteStream)]
pub struct IteratorByteStream {
    iterator: Py<PyAny>,
}

#[pymethods]
impl IteratorByteStream {
    #[new]
    pub fn new(iterator: Py<PyAny>) -> PyClassInitializer<Self> {
        PyClassInitializer::from(AsyncByteStream)
            .add_subclass(SyncByteStream)
            .add_subclass(IteratorByteStream { iterator })
    }

    fn __iter__(slf: PyRef<'_, Self>) -> PyRef<'_, Self> {
        slf
    }

    fn __next__(&self, py: Python<'_>) -> PyResult<Option<Py<PyAny>>> {
        // Call __next__ on iterator
        match self.iterator.call_method0(py, "__next__") {
            Ok(val) => Ok(Some(val)),
            Err(e) => {
                if e.is_instance_of::<pyo3::exceptions::PyStopIteration>(py) {
                    Ok(None)
                } else {
                    Err(e)
                }
            }
        }
    }

    fn close(&self) -> PyResult<()> {
        Ok(())
    }
}

/// Wrapper for Python sync iterators used in Response that tracks consumption.
/// Raises StreamConsumed on re-iteration after the stream has been fully consumed.
#[pyclass]
pub struct ResponseIteratorStream {
    iterator: Py<PyAny>,
    actual_iter: std::sync::Mutex<Option<Py<PyAny>>>,
    consumed: std::sync::atomic::AtomicBool,
    started: std::sync::atomic::AtomicBool,
}

impl ResponseIteratorStream {
    pub fn new(iterator: Py<PyAny>) -> Self {
        ResponseIteratorStream {
            iterator,
            actual_iter: std::sync::Mutex::new(None),
            consumed: std::sync::atomic::AtomicBool::new(false),
            started: std::sync::atomic::AtomicBool::new(false),
        }
    }
}

#[pymethods]
impl ResponseIteratorStream {
    fn __iter__<'a>(slf: PyRef<'a, Self>, py: Python<'_>) -> PyResult<PyRef<'a, Self>> {
        if slf.consumed.load(std::sync::atomic::Ordering::Relaxed) {
            return Err(crate::exceptions::StreamConsumed::new_err(
                "Attempted to read or stream content, but the stream has already been consumed.",
            ));
        }
        slf.started
            .store(true, std::sync::atomic::Ordering::Relaxed);
        // Get the actual iterator by calling __iter__ on the original iterable
        {
            let mut guard = slf.actual_iter.lock().unwrap();
            if guard.is_none() {
                let iter = slf.iterator.call_method0(py, "__iter__")?;
                *guard = Some(iter);
            }
        }
        Ok(slf)
    }

    fn __next__(&self, py: Python<'_>) -> PyResult<Option<Py<PyAny>>> {
        if self.consumed.load(std::sync::atomic::Ordering::Relaxed) {
            return Err(crate::exceptions::StreamConsumed::new_err(
                "Attempted to read or stream content, but the stream has already been consumed.",
            ));
        }
        let guard = self.actual_iter.lock().unwrap();
        let iter = guard.as_ref().ok_or_else(|| {
            pyo3::exceptions::PyRuntimeError::new_err("__iter__ must be called before __next__")
        })?;
        match iter.call_method0(py, "__next__") {
            Ok(val) => Ok(Some(val)),
            Err(e) => {
                if e.is_instance_of::<pyo3::exceptions::PyStopIteration>(py) {
                    self.consumed
                        .store(true, std::sync::atomic::Ordering::Relaxed);
                    Ok(None)
                } else {
                    Err(e)
                }
            }
        }
    }
}

/// Wrapper for Python async iterators used in Response that tracks consumption.
/// Raises StreamConsumed on re-iteration after the stream has been fully consumed.
#[pyclass(extends=AsyncByteStream)]
pub struct ResponseAsyncIteratorStream {
    original: Py<PyAny>,
    actual_iterator: std::sync::Mutex<Option<Py<PyAny>>>,
    started: std::sync::atomic::AtomicBool,
}

impl ResponseAsyncIteratorStream {
    pub fn new(original: Py<PyAny>) -> PyClassInitializer<Self> {
        PyClassInitializer::from(AsyncByteStream).add_subclass(ResponseAsyncIteratorStream {
            original,
            actual_iterator: std::sync::Mutex::new(None),
            started: std::sync::atomic::AtomicBool::new(false),
        })
    }
}

#[pymethods]
impl ResponseAsyncIteratorStream {
    fn __aiter__(slf: PyRef<'_, Self>, py: Python<'_>) -> PyResult<Py<Self>> {
        // If __aiter__ was already called (stream was started), raise StreamConsumed
        if slf.started.load(std::sync::atomic::Ordering::Relaxed) {
            return Err(crate::exceptions::StreamConsumed::new_err(
                "Attempted to read or stream content, but the stream has already been consumed.",
            ));
        }
        slf.started
            .store(true, std::sync::atomic::Ordering::Relaxed);
        // Call __aiter__ on the original to get the actual async iterator
        {
            let mut guard = slf.actual_iterator.lock().unwrap();
            if guard.is_none() {
                let iter = slf.original.call_method0(py, "__aiter__")?;
                *guard = Some(iter);
            }
        } // drop guard before converting slf
          // Return self as the object to iterate (we implement __anext__)
        Ok(slf.into())
    }

    fn __anext__<'py>(&self, py: Python<'py>) -> PyResult<Option<Bound<'py, PyAny>>> {
        let guard = self.actual_iterator.lock().unwrap();
        let iter = guard.as_ref().ok_or_else(|| {
            pyo3::exceptions::PyRuntimeError::new_err("__aiter__ must be called before __anext__")
        })?;
        match iter.call_method0(py, "__anext__") {
            Ok(val) => {
                let bound = val.bind(py);
                Ok(Some(bound.clone()))
            }
            Err(e) => {
                if e.is_instance_of::<pyo3::exceptions::PyStopAsyncIteration>(py) {
                    Ok(None)
                } else {
                    Err(e)
                }
            }
        }
    }
}

pub fn register(m: &Bound<'_, PyModule>) -> PyResult<()> {
    m.add_class::<AsyncByteStream>()?;
    m.add_class::<SyncByteStream>()?;
    m.add_class::<ByteStream>()?;
    m.add_class::<BlockingBytesIterator>()?;
    m.add_class::<AsyncResponseStream>()?;
    m.add_class::<ChunkedIter>()?;
    m.add_class::<IteratorByteStream>()?;
    m.add_class::<ResponseIteratorStream>()?;
    m.add_class::<ResponseAsyncIteratorStream>()?;
    Ok(())
}
