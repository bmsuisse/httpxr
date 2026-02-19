use pyo3::prelude::*;
use pyo3::types::{PyBytes, PyList};

use super::headers::Headers;
use super::request::Request;
use super::response::Response;

#[pymethods]
impl Response {
    #[pyo3(signature = (chunk_size=None))]
    fn iter_bytes(&mut self, py: Python<'_>, chunk_size: Option<usize>) -> PyResult<Py<PyAny>> {
        if self.is_stream_consumed && self.content_bytes.is_none() {
            return Err(crate::exceptions::StreamConsumed::new_err(
                "Attempted to read or stream content, but the stream has already been consumed.",
            ));
        }
        if self.is_closed_flag && self.content_bytes.is_none() {
            return Err(crate::exceptions::StreamClosed::new_err(
                "Attempted to read or stream content, but the stream has been closed.",
            ));
        }

        if let Some(stream_py) = self.stream.take() {
            let s = stream_py.bind(py);
            if s.hasattr("__aiter__")? && !s.hasattr("__iter__")? {
                return Err(pyo3::exceptions::PyRuntimeError::new_err(
                    "Attempted to read an asynchronous response using a synchronous method.",
                ));
            }
            let mut all_chunks: Vec<Py<PyAny>> = Vec::new();
            let mut chunk_byte_sizes: Vec<usize> = Vec::new();
            let mut total_bytes = Vec::new();
            if let Ok(iter) = s.try_iter() {
                for item in iter {
                    let item = item?;
                    if let Ok(b) = item.cast::<PyBytes>() {
                        let bytes = b.as_bytes();
                        if !bytes.is_empty() {
                            total_bytes.extend_from_slice(bytes);
                            if chunk_size.is_none() {
                                all_chunks.push(PyBytes::new(py, bytes).into_any().unbind());
                                chunk_byte_sizes.push(bytes.len());
                            }
                        }
                    } else if let Ok(v) = item.extract::<Vec<u8>>() {
                        if !v.is_empty() {
                            total_bytes.extend_from_slice(&v);
                            if chunk_size.is_none() {
                                all_chunks.push(PyBytes::new(py, &v).into_any().unbind());
                                chunk_byte_sizes.push(v.len());
                            }
                        }
                    }
                }
            }
            self.content_bytes = Some(total_bytes.clone());
            self.is_stream_consumed = true;
            self.is_closed_flag = true;

            if let Some(cs) = chunk_size {
                all_chunks.clear();
                chunk_byte_sizes.clear();
                let mut offset = 0;
                while offset < total_bytes.len() {
                    let end = std::cmp::min(offset + cs, total_bytes.len());
                    let chunk_len = end - offset;
                    all_chunks.push(
                        PyBytes::new(py, &total_bytes[offset..end])
                            .into_any()
                            .unbind(),
                    );
                    chunk_byte_sizes.push(chunk_len);
                    offset = end;
                }
            }

            let counter = self.num_bytes_downloaded_counter.clone();
            let iter = crate::types::ChunkedIter {
                chunks: all_chunks,
                pos: 0,
                byte_counter: Some(counter),
                chunk_byte_sizes: Some(chunk_byte_sizes),
            };
            return Ok(Py::new(py, iter)?.into_any());
        }

        let content = self.content_bytes.clone().unwrap_or_default();
        if content.is_empty() {
            let list = PyList::empty(py);
            return Ok(list.call_method0("__iter__")?.into());
        }
        if let Some(cs) = chunk_size {
            let mut chunks = Vec::new();
            let mut offset = 0;
            while offset < content.len() {
                let end = std::cmp::min(offset + cs, content.len());
                chunks.push(PyBytes::new(py, &content[offset..end]).into_any().unbind());
                offset = end;
            }
            let list = PyList::new(py, &chunks)?;
            Ok(list.call_method0("__iter__")?.into())
        } else {
            let list = PyList::new(py, &[PyBytes::new(py, &content)])?;
            Ok(list.call_method0("__iter__")?.into())
        }
    }

    #[pyo3(signature = (chunk_size=None))]
    fn iter_raw(&mut self, py: Python<'_>, chunk_size: Option<usize>) -> PyResult<Py<PyAny>> {
        self.iter_bytes(py, chunk_size)
    }

    #[pyo3(signature = (chunk_size=None))]
    fn iter_text(&mut self, py: Python<'_>, chunk_size: Option<usize>) -> PyResult<Py<PyAny>> {
        if self.content_bytes.is_none() && self.stream.is_some() {
            let stream = self.stream.take();
            let mut raw_chunks: Vec<Vec<u8>> = Vec::new();
            if let Some(ref s) = stream {
                let s = s.bind(py);
                if let Ok(iter) = s.try_iter() {
                    for item in iter {
                        let item = item?;
                        if let Ok(b) = item.cast::<PyBytes>() {
                            raw_chunks.push(b.as_bytes().to_vec());
                        } else if let Ok(v) = item.extract::<Vec<u8>>() {
                            raw_chunks.push(v);
                        }
                    }
                }
            }
            let full: Vec<u8> = raw_chunks.iter().flatten().copied().collect();
            self.content_bytes = Some(full);
            self.is_stream_consumed = true;
            self.is_closed_flag = true;

            let encoding_name = self.resolve_encoding(py);

            if let Some(cs) = chunk_size {
                let text = self.text_impl(py)?;
                if text.is_empty() {
                    let list = PyList::empty(py);
                    return Ok(list.call_method0("__iter__")?.into());
                }
                let mut chunks = Vec::new();
                let chars: Vec<char> = text.chars().collect();
                let mut offset = 0;
                while offset < chars.len() {
                    let end = std::cmp::min(offset + cs, chars.len());
                    let chunk: String = chars[offset..end].iter().collect();
                    chunks.push(chunk);
                    offset = end;
                }
                let list = PyList::new(py, &chunks)?;
                Ok(list.call_method0("__iter__")?.into())
            } else {
                let mut text_chunks: Vec<String> = Vec::new();
                for chunk in &raw_chunks {
                    if !chunk.is_empty() {
                        let py_bytes = PyBytes::new(py, chunk);
                        let str_obj =
                            py_bytes.call_method1("decode", (&encoding_name, "replace"))?;
                        let s: String = str_obj.extract()?;
                        if !s.is_empty() {
                            text_chunks.push(s);
                        }
                    }
                }
                let list = PyList::new(py, &text_chunks)?;
                Ok(list.call_method0("__iter__")?.into())
            }
        } else {
            let text = self.text_impl(py)?;
            if text.is_empty() {
                let list = PyList::empty(py);
                return Ok(list.call_method0("__iter__")?.into());
            }
            if let Some(cs) = chunk_size {
                let mut chunks = Vec::new();
                let chars: Vec<char> = text.chars().collect();
                let mut offset = 0;
                while offset < chars.len() {
                    let end = std::cmp::min(offset + cs, chars.len());
                    let chunk: String = chars[offset..end].iter().collect();
                    chunks.push(chunk);
                    offset = end;
                }
                let list = PyList::new(py, &chunks)?;
                Ok(list.call_method0("__iter__")?.into())
            } else {
                let list = PyList::new(py, &[text])?;
                Ok(list.call_method0("__iter__")?.into())
            }
        }
    }

    fn iter_lines(&mut self, py: Python<'_>) -> PyResult<Py<PyAny>> {
        if self.content_bytes.is_none() && self.stream.is_some() {
            self.read_impl(py)?;
        }
        let text = self.text_impl(py)?;
        let mut lines: Vec<String> = Vec::new();
        let mut current = String::new();
        let chars: Vec<char> = text.chars().collect();
        let mut i = 0;
        while i < chars.len() {
            if chars[i] == '\r' {
                lines.push(std::mem::take(&mut current));
                if i + 1 < chars.len() && chars[i + 1] == '\n' {
                    i += 1; // skip \n after \r
                }
            } else if chars[i] == '\n' {
                lines.push(std::mem::take(&mut current));
            } else {
                current.push(chars[i]);
            }
            i += 1;
        }
        if !current.is_empty() {
            lines.push(current);
        }
        let list = PyList::new(py, &lines)?;
        Ok(list.call_method0("__iter__")?.into())
    }

    #[pyo3(signature = (chunk_size=None))]
    fn aiter_bytes(slf: Bound<'_, Self>, chunk_size: Option<usize>) -> PyResult<Py<PyAny>> {
        let py = slf.py();
        {
            let self_ = slf.borrow();
            if self_.is_stream_consumed && self_.content_bytes.is_none() {
                return Err(crate::exceptions::StreamConsumed::new_err(
                    "Attempted to read or stream content, but the stream has already been consumed."));
            }
            if self_.is_closed_flag && self_.content_bytes.is_none() {
                return Err(crate::exceptions::StreamClosed::new_err(
                    "Attempted to read or stream content, but the stream has been closed.",
                ));
            }

            if let Some(ref stream) = self_.stream {
                let s = stream.bind(py);
                let has_aiter = s.hasattr("__aiter__").unwrap_or(false);
                let has_iter = s.hasattr("__iter__").unwrap_or(false);
                if has_iter && !has_aiter {
                    return Err(pyo3::exceptions::PyRuntimeError::new_err(
                        "Attempted to read an synchronous response using an asynchronous method.",
                    ));
                }
            }
        }

        let stream_py = {
            let mut self_mut = slf.borrow_mut();
            self_mut.stream.take()
        };
        if let Some(stream_py) = stream_py {
            let ns = pyo3::types::PyDict::new(py);
            ns.set_item("_stream", stream_py)?;
            ns.set_item("_chunk_size", chunk_size)?;
            ns.set_item("_response", &slf)?;

            py.run(
                c"
async def _aiter_bytes_impl():
    chunks = []
    async for chunk in _stream:
        b = bytes(chunk)
        if b:
            chunks.append(b)
    return chunks

class _AsyncChunkIter:
    def __init__(self, coro, chunk_size, response):
        self._coro = coro
        self._chunk_size = chunk_size
        self._response = response
        self._chunks = None
        self._pos = 0
    
    def __aiter__(self):
        return self
    
    async def __anext__(self):
        if self._chunks is None:
            # First call: drain the stream
            raw_chunks = await self._coro
            # Re-chunk if needed
            if self._chunk_size is not None:
                total = b''.join(raw_chunks)
                cs = self._chunk_size
                self._chunks = [total[i:i+cs] for i in range(0, len(total), cs)]
            else:
                self._chunks = raw_chunks
        
        if self._pos >= len(self._chunks):
            raise StopAsyncIteration
        
        chunk = self._chunks[self._pos]
        self._pos += 1
        self._response._add_bytes_downloaded(len(chunk))
        return chunk

_result = _AsyncChunkIter(_aiter_bytes_impl(), _chunk_size, _response)
",
                Some(&ns),
                Some(&ns),
            )?;

            let result = ns.get_item("_result")?.unwrap();

            {
                let mut self_mut = slf.borrow_mut();
                self_mut.is_stream_consumed = true;
                self_mut.is_closed_flag = true;
            }

            return Ok(result.unbind());
        }

        let self_ = slf.borrow();
        let content = self_.content_bytes.clone().unwrap_or_default();
        if content.is_empty() {
            let iter = crate::types::ChunkedIter {
                chunks: Vec::new(),
                pos: 0,
                byte_counter: None,
                chunk_byte_sizes: None,
            };
            return Ok(Py::new(py, iter)?.into_any());
        }
        if let Some(cs) = chunk_size {
            let mut chunks = Vec::new();
            let mut offset = 0;
            while offset < content.len() {
                let end = std::cmp::min(offset + cs, content.len());
                chunks.push(PyBytes::new(py, &content[offset..end]).into_any().unbind());
                offset = end;
            }
            let iter = crate::types::ChunkedIter {
                chunks,
                pos: 0,
                byte_counter: None,
                chunk_byte_sizes: None,
            };
            Ok(Py::new(py, iter)?.into_any())
        } else {
            let bs = crate::types::ByteStream {
                data: content,
                pos: 0,
            };
            let init = pyo3::PyClassInitializer::from(crate::types::AsyncByteStream)
                .add_subclass(crate::types::SyncByteStream)
                .add_subclass(bs);
            Ok(Py::new(py, init)?.into_any())
        }
    }

    #[pyo3(signature = (chunk_size=None))]
    fn aiter_raw(slf: Bound<'_, Self>, chunk_size: Option<usize>) -> PyResult<Py<PyAny>> {
        Self::aiter_bytes(slf, chunk_size)
    }

    #[pyo3(signature = (chunk_size=None))]
    fn aiter_text(&mut self, py: Python<'_>, chunk_size: Option<usize>) -> PyResult<Py<PyAny>> {
        let text = self.text_impl(py)?;
        if text.is_empty() {
            let iter = crate::types::ChunkedIter {
                chunks: Vec::new(),
                pos: 0,
                byte_counter: None,
                chunk_byte_sizes: None,
            };
            return Ok(Py::new(py, iter)?.into_any());
        }
        if let Some(cs) = chunk_size {
            let mut chunks: Vec<Py<PyAny>> = Vec::new();
            let chars: Vec<char> = text.chars().collect();
            let mut offset = 0;
            while offset < chars.len() {
                let end = std::cmp::min(offset + cs, chars.len());
                let chunk: String = chars[offset..end].iter().collect();
                chunks.push(chunk.into_pyobject(py)?.into_any().unbind());
                offset = end;
            }
            let iter = crate::types::ChunkedIter {
                chunks,
                pos: 0,
                byte_counter: None,
                chunk_byte_sizes: None,
            };
            Ok(Py::new(py, iter)?.into_any())
        } else {
            let chunks = vec![text.into_pyobject(py)?.into_any().unbind()];
            let iter = crate::types::ChunkedIter {
                chunks,
                pos: 0,
                byte_counter: None,
                chunk_byte_sizes: None,
            };
            Ok(Py::new(py, iter)?.into_any())
        }
    }

    fn aiter_lines(&mut self, py: Python<'_>) -> PyResult<Py<PyAny>> {
        let text = self.text_impl(py)?;
        let chunks: Vec<Py<PyAny>> = text
            .lines()
            .map(|l| l.to_string().into_pyobject(py).unwrap().into_any().unbind())
            .collect();
        let iter = crate::types::ChunkedIter {
            chunks,
            pos: 0,
            byte_counter: None,
            chunk_byte_sizes: None,
        };
        Ok(Py::new(py, iter)?.into_any())
    }

    fn aclose<'py>(&mut self, py: Python<'py>) -> PyResult<Bound<'py, PyAny>> {
        if let Some(ref stream) = self.stream {
            let s = stream.bind(py);
            let has_aiter = s.hasattr("__aiter__").unwrap_or(false);
            let has_iter = s.hasattr("__iter__").unwrap_or(false);
            if has_iter && !has_aiter {
                return Err(pyo3::exceptions::PyRuntimeError::new_err(
                    "Attempted to close a synchronous response using 'await response.aclose()'. Use 'response.close()' instead."));
            }
        }
        self.is_closed_flag = true;
        pyo3_async_runtimes::tokio::future_into_py(py, async move { Ok(()) })
    }

    #[getter]
    fn is_stream_consumed(&self) -> bool {
        self.is_stream_consumed
    }
    fn __repr__(&self) -> String {
        let reason = self.reason_phrase_impl();
        if reason.is_empty() {
            format!("<Response [{}]>", self.status_code)
        } else {
            format!("<Response [{} {}]>", self.status_code, reason)
        }
    }
    fn __bool__(&self) -> bool {
        self.is_success_impl()
    }

    fn __reduce__(&mut self, py: Python<'_>) -> PyResult<Py<PyAny>> {
        let cls = py.import("httpxr")?.getattr("Response")?;
        let args =
            pyo3::types::PyTuple::new(py, &[self.status_code.into_pyobject(py)?.into_any()])?;
        let state = pyo3::types::PyDict::new(py);
        let hdrs_py = self.headers(py).unwrap();
        let hdrs = hdrs_py.borrow(py);
        let h_list = PyList::empty(py);
        for (k, v) in &hdrs.raw {
            let t = pyo3::types::PyTuple::new(
                py,
                &[
                    PyBytes::new(py, k).into_any(),
                    PyBytes::new(py, v).into_any(),
                ],
            )?;
            h_list.append(t)?;
        }
        state.set_item("headers", h_list)?;
        if let Some(ref c) = self.content_bytes {
            state.set_item("content", PyBytes::new(py, c))?;
            state.set_item("has_content", true)?;
        } else {
            state.set_item("has_content", false)?;
        }
        state.set_item("is_stream_consumed", self.is_stream_consumed)?;
        state.set_item("was_streaming", self.was_streaming)?;
        state.set_item(
            "num_bytes_downloaded",
            self.num_bytes_downloaded_counter
                .load(std::sync::atomic::Ordering::Relaxed),
        )?;
        if let Some(ref req) = self.request {
            state.set_item("request", req.clone().into_pyobject(py)?)?;
        }
        let result =
            pyo3::types::PyTuple::new(py, &[cls.into_any(), args.into_any(), state.into_any()])?;
        Ok(result.into())
    }

    fn __setstate__(&mut self, py: Python<'_>, state: &Bound<'_, PyAny>) -> PyResult<()> {
        let dict = state.cast::<pyo3::types::PyDict>()?;
        if let Some(has_content) = dict.get_item("has_content")? {
            if has_content.extract::<bool>()? {
                if let Some(content) = dict.get_item("content")? {
                    self.content_bytes = Some(content.extract::<Vec<u8>>()?);
                }
            } else {
                self.content_bytes = None;
            }
        } else if let Some(content) = dict.get_item("content")? {
            self.content_bytes = Some(content.extract::<Vec<u8>>()?);
        }
        if let Some(headers) = dict.get_item("headers")? {
            let h_list: Vec<(Vec<u8>, Vec<u8>)> = headers.extract()?;
            let hdrs = Headers {
                raw: h_list,
                encoding: "utf-8".to_string(),
            };
            self.headers = Some(Py::new(py, hdrs)?);
        }
        if let Some(consumed) = dict.get_item("is_stream_consumed")? {
            self.is_stream_consumed = consumed.extract::<bool>()?;
        } else {
            self.is_stream_consumed = self.content_bytes.is_some();
        }
        if let Some(was_str) = dict.get_item("was_streaming")? {
            self.was_streaming = was_str.extract::<bool>()?;
        }
        if let Some(nbd) = dict.get_item("num_bytes_downloaded")? {
            self.num_bytes_downloaded_counter.store(
                nbd.extract::<usize>()?,
                std::sync::atomic::Ordering::Relaxed,
            );
        } else if let Some(ref c) = self.content_bytes {
            self.num_bytes_downloaded_counter
                .store(c.len(), std::sync::atomic::Ordering::Relaxed);
        }
        self.is_closed_flag = true;
        self.stream = None;
        if let Some(req) = dict.get_item("request")? {
            if !req.is_none() {
                self.request = Some(req.extract::<Request>()?);
            }
        }
        Ok(())
    }
}
