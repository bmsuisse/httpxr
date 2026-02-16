use pyo3::exceptions::PyTypeError;
use pyo3::prelude::*;
use pyo3::types::{PyBytes, PyDict, PyList, PyString, PyTuple};
use rand::RngExt;

/// Generate a random multipart boundary.
fn generate_boundary() -> String {
    let mut rng = rand::rng();
    let chars: Vec<char> = "abcdefghijklmnopqrstuvwxyz0123456789".chars().collect();
    let boundary: String = (0..16)
        .map(|_| chars[rng.random_range(0..chars.len())])
        .collect();
    format!("----httpr-boundary-{}", boundary)
}

/// DataField: a single form data field.
#[pyclass(from_py_object)]
#[derive(Clone)]
pub struct DataField {
    name: String,
    value: Vec<u8>,
}

#[pymethods]
impl DataField {
    #[new]
    fn new(name: &str, value: &[u8]) -> Self {
        DataField {
            name: name.to_string(),
            value: value.to_vec(),
        }
    }

    fn render_headers(&self) -> Vec<u8> {
        format!(
            "Content-Disposition: form-data; name=\"{}\"\r\n\r\n",
            self.name
        )
        .into_bytes()
    }

    fn render_data(&self) -> Vec<u8> {
        self.value.clone()
    }
}

/// FileField: a file upload field.
#[pyclass(from_py_object)]
#[derive(Clone)]
pub struct FileField {
    name: String,
    filename: Option<String>,
    content_type: Option<String>,
    data: Vec<u8>,
    headers: Vec<(String, String)>,
}

#[pymethods]
impl FileField {
    #[new]
    #[pyo3(signature = (name, filename, data, content_type=None, headers=None))]
    fn new(
        name: &str,
        filename: Option<&str>,
        data: &[u8],
        content_type: Option<&str>,
        headers: Option<Vec<(String, String)>>,
    ) -> Self {
        FileField {
            name: name.to_string(),
            filename: filename.map(|s| s.to_string()),
            content_type: content_type.map(|s| s.to_string()),
            data: data.to_vec(),
            headers: headers.unwrap_or_default(),
        }
    }

    fn render_headers(&self) -> Vec<u8> {
        let mut headers_str = format!("Content-Disposition: form-data; name=\"{}\"", self.name);

        if let Some(ref fname) = self.filename {
            headers_str.push_str(&format!("; filename=\"{}\"", fname));
        }

        headers_str.push_str("\r\n");

        // Render extra headers first
        for (k, v) in &self.headers {
            headers_str.push_str(&format!("{}: {}\r\n", k, v));
        }

        // Render Content-Type if not in extra headers and we have one
        let has_ct = self
            .headers
            .iter()
            .any(|(k, _)| k.eq_ignore_ascii_case("content-type"));

        if !has_ct {
            if let Some(ref ct) = self.content_type {
                headers_str.push_str(&format!("Content-Type: {}\r\n", ct));
            }
        }

        headers_str.push_str("\r\n");
        headers_str.into_bytes()
    }

    fn render_data(&self) -> Vec<u8> {
        self.data.clone()
    }
}

/// MultipartStream: streams multipart/form-data encoded content.
#[pyclass]
pub struct MultipartStream {
    boundary: String,
    fields: Vec<MultipartField>,
    buffer: Vec<u8>,
    position: usize,
}

enum MultipartField {
    Data(DataField),
    File(FileField),
}

impl MultipartStream {
    fn boundary_bytes(&self) -> Vec<u8> {
        format!("--{}", self.boundary).into_bytes()
    }

    fn closing_boundary_bytes(&self) -> Vec<u8> {
        format!("--{}--", self.boundary).into_bytes()
    }

    fn render_full_content(&self) -> Vec<u8> {
        let mut body = Vec::new();
        for field in &self.fields {
            body.extend_from_slice(&self.boundary_bytes());
            body.extend_from_slice(b"\r\n");
            match field {
                MultipartField::Data(df) => {
                    body.extend_from_slice(&df.render_headers());
                    body.extend_from_slice(&df.render_data());
                }
                MultipartField::File(ff) => {
                    body.extend_from_slice(&ff.render_headers());
                    body.extend_from_slice(&ff.render_data());
                }
            }
            body.extend_from_slice(b"\r\n");
        }
        body.extend_from_slice(&self.closing_boundary_bytes());
        body.extend_from_slice(b"\r\n"); // Trailing newline!
        body
    }
}

fn escape_filename(filename: &str) -> String {
    filename
        .replace('\\', "\\\\")
        .replace('"', "%22")
        .replace('\r', "%0D")
        .replace('\n', "%0A")
        .chars()
        .map(|c| {
            if c.is_control() && c != '\x1b' {
                format!("%{:02X}", c as u32)
            } else {
                c.to_string()
            }
        })
        .collect()
}

fn extract_data_value(obj: &Bound<'_, PyAny>) -> PyResult<Vec<u8>> {
    if let Ok(b) = obj.cast::<PyBytes>() {
        Ok(b.as_bytes().to_vec())
    } else if let Ok(s) = obj.cast::<PyString>() {
        Ok(s.to_string().into_bytes())
    } else if let Ok(b) = obj.cast::<pyo3::types::PyBool>() {
        let val = b.is_true();
        Ok(if val {
            "true".as_bytes().to_vec()
        } else {
            "false".as_bytes().to_vec()
        })
    } else if let Ok(i) = obj.cast::<pyo3::types::PyInt>() {
        Ok(i.to_string().into_bytes())
    } else if let Ok(f) = obj.cast::<pyo3::types::PyFloat>() {
        Ok(f.to_string().into_bytes())
    } else {
        Err(PyTypeError::new_err("Invalid type for value"))
    }
}

fn process_file_item(
    _py: Python<'_>,
    name: &str,
    item: &Bound<'_, PyAny>,
    fields: &mut Vec<MultipartField>,
    mimetypes: &Bound<'_, PyModule>,
) -> PyResult<()> {
    let (filename, file_obj, explicit_ctype, headers) = if let Ok(tuple) = item.cast::<PyTuple>() {
        let fname_obj = tuple.get_item(0)?;
        let fname: Option<String> = if fname_obj.is_none() {
            None
        } else {
            Some(fname_obj.extract::<String>().or_else(|_| {
                fname_obj
                    .extract::<Vec<u8>>()
                    .map(|b| String::from_utf8_lossy(&b).to_string())
            })?)
        };

        let fobj = tuple.get_item(1)?;
        let ctype = if tuple.len() > 2 {
            tuple.get_item(2)?.extract::<String>().ok()
        } else {
            None
        };
        let hdrs = if tuple.len() > 3 {
            if let Ok(d) = tuple.get_item(3)?.cast::<PyDict>() {
                let mut h = Vec::new();
                for (k, v) in d.iter() {
                    let k_str: String = k
                        .extract()
                        .map_err(|_| PyTypeError::new_err("Invalid type for header key"))?;
                    let v_str: String = v
                        .extract()
                        .map_err(|_| PyTypeError::new_err("Invalid type for header value"))?;
                    h.push((k_str, v_str));
                }
                Some(h)
            } else {
                None
            }
        } else {
            None
        };
        (fname, fobj, ctype, hdrs)
    } else {
        // Direct file object
        let fname = if let Ok(n) = item.getattr("name") {
            n.extract::<String>()
                .unwrap_or_else(|_| "upload".to_string())
        } else {
            "upload".to_string()
        };
        // Extract only the basename of the file
        let fname_path = std::path::Path::new(&fname);
        let basename = fname_path
            .file_name()
            .map(|s| s.to_string_lossy().to_string())
            .unwrap_or(fname);

        (Some(basename), item.clone(), None, None)
    };

    if let Ok(mode) = file_obj.getattr("mode") {
        if let Ok(mode_str) = mode.extract::<String>() {
            if !mode_str.contains('b') {
                return Err(PyTypeError::new_err(
                    "Expected bytes, got str (text mode file)",
                ));
            }
        }
    } else if let Ok(has_encoding) = file_obj.call_method1("__getattribute__", ("encoding",)) {
        if !has_encoding.is_none() {
            return Err(PyTypeError::new_err(
                "Expected bytes, got str (text mode file)",
            ));
        }
    } else if let Ok(true) = file_obj.hasattr("encoding") {
        if let Ok(enc) = file_obj.getattr("encoding") {
            if !enc.is_none() {
                return Err(PyTypeError::new_err(
                    "Expected bytes, got str (text mode file)",
                ));
            }
        }
    }

    let seekable = file_obj.call_method1("seek", (0,)).is_ok();

    let file_data: Vec<u8> = if let Ok(b) = file_obj.cast::<PyBytes>() {
        b.as_bytes().to_vec()
    } else if let Ok(s) = file_obj.cast::<PyString>() {
        s.to_string().into_bytes()
    } else if seekable {
        if let Ok(read_res) = file_obj.call_method0("read") {
            if let Ok(bytes) = read_res.cast::<PyBytes>() {
                bytes.as_bytes().to_vec()
            } else if read_res.cast::<PyString>().is_ok() {
                return Err(PyTypeError::new_err("Expected bytes, got str"));
            } else if read_res.extract::<String>().is_ok() {
                return Err(PyTypeError::new_err("Expected bytes, got str"));
            } else {
                // Unknown type, try force extract or empty
                if let Ok(b) = read_res.extract::<Vec<u8>>() {
                    b
                } else {
                    Vec::new()
                }
            }
        } else {
            Vec::new()
        }
    } else {
        if let Ok(read_res) = file_obj.call_method0("read") {
            if let Ok(bytes) = read_res.cast::<PyBytes>() {
                bytes.as_bytes().to_vec()
            } else if read_res.cast::<PyString>().is_ok() || read_res.extract::<String>().is_ok() {
                return Err(PyTypeError::new_err("Expected bytes, got str"));
            } else {
                Vec::new()
            }
        } else {
            let mut buffer = Vec::new();
            if let Ok(iter) = file_obj.try_iter() {
                for item in iter {
                    let chunk = item?;
                    if let Ok(b) = chunk.cast::<PyBytes>() {
                        buffer.extend_from_slice(b.as_bytes());
                    } else if chunk.cast::<PyString>().is_ok() {
                        return Err(PyTypeError::new_err("Expected bytes, got str"));
                    }
                }
                buffer
            } else {
                Vec::new()
            }
        }
    };

    let content_type: Option<String> = if let Some(ct) = explicit_ctype {
        Some(ct)
    } else if let Some(ref f) = filename {
        let guessed = mimetypes.call_method1("guess_type", (f.as_str(),))?;
        let tuple = guessed.cast::<PyTuple>()?;
        let mime = tuple.get_item(0)?;
        if mime.is_none() {
            Some("application/octet-stream".to_string())
        } else {
            Some(mime.extract::<String>()?)
        }
    } else {
        None
    };

    let escaped_filename = filename.as_ref().map(|f| escape_filename(f));

    fields.push(MultipartField::File(FileField::new(
        name,
        escaped_filename.as_deref(),
        &file_data,
        content_type.as_deref(),
        headers,
    )));
    Ok(())
}

#[pymethods]
impl MultipartStream {
    #[new]
    #[pyo3(signature = (data=None, files=None, boundary=None))]
    pub fn new(
        py: Python<'_>,
        data: Option<&Bound<'_, PyAny>>,
        files: Option<&Bound<'_, PyAny>>,
        boundary: Option<&str>,
    ) -> PyResult<Self> {
        let boundary = boundary
            .map(|s| s.to_string())
            .unwrap_or_else(generate_boundary);

        let mut fields: Vec<MultipartField> = Vec::new();

        // Process data fields
        if let Some(d) = data {
            if !d.is_none() {
                if let Ok(dict) = d.cast::<PyDict>() {
                    for (k, v) in dict.iter() {
                        let name: String = k.extract().map_err(|_| {
                            let repr = k
                                .repr()
                                .ok()
                                .map(|s| s.to_string_lossy().to_string())
                                .unwrap_or_else(|| "unknown".to_string());
                            PyTypeError::new_err(format!("Invalid type for name: {}", repr))
                        })?;

                        // Check for list/tuple (repeated fields)
                        if let Ok(list) = v.cast::<PyList>() {
                            for item in list.iter() {
                                let value = extract_data_value(&item)?;
                                fields.push(MultipartField::Data(DataField::new(&name, &value)));
                            }
                        } else if let Ok(tuple) = v.cast::<PyTuple>() {
                            for item in tuple.iter() {
                                let value = extract_data_value(&item)?;
                                fields.push(MultipartField::Data(DataField::new(&name, &value)));
                            }
                        } else {
                            // Single value
                            let value = extract_data_value(&v)?;
                            fields.push(MultipartField::Data(DataField::new(&name, &value)));
                        }
                    }
                } else if let Ok(list) = d.cast::<PyList>() {
                    for item in list.iter() {
                        let tuple = item.cast::<PyTuple>()?;
                        let name: String = tuple.get_item(0)?.extract().map_err(|_| {
                            let k = tuple.get_item(0).unwrap();
                            let repr = k
                                .repr()
                                .ok()
                                .map(|s| s.to_string_lossy().to_string())
                                .unwrap_or_else(|| "unknown".to_string());
                            PyTypeError::new_err(format!("Invalid type for name: {}", repr))
                        })?;
                        let v = tuple.get_item(1)?;
                        let value = extract_data_value(&v)?;
                        fields.push(MultipartField::Data(DataField::new(&name, &value)));
                    }
                }
            }
        }

        // Process file fields
        if let Some(f) = files {
            if !f.is_none() {
                let mimetypes = py.import("mimetypes")?;

                if let Ok(dict) = f.cast::<PyDict>() {
                    for (k, v) in dict.iter() {
                        let name: String = k.extract().map_err(|_| {
                            let repr = k
                                .repr()
                                .ok()
                                .map(|s| s.to_string_lossy().to_string())
                                .unwrap_or_else(|| "unknown".to_string());
                            PyTypeError::new_err(format!("Invalid type for name: {}", repr))
                        })?;

                        if let Ok(list) = v.cast::<PyList>() {
                            for item in list.iter() {
                                process_file_item(py, &name, &item, &mut fields, &mimetypes)?;
                            }
                        } else {
                            process_file_item(py, &name, &v, &mut fields, &mimetypes)?;
                        }
                    }
                } else if let Ok(list) = f.cast::<PyList>() {
                    for item in list.iter() {
                        let tuple = item.cast::<PyTuple>()?;
                        let name: String = tuple.get_item(0)?.extract().map_err(|_| {
                            let k = tuple.get_item(0).unwrap();
                            let repr = k
                                .repr()
                                .ok()
                                .map(|s| s.to_string_lossy().to_string())
                                .unwrap_or_else(|| "unknown".to_string());
                            PyTypeError::new_err(format!("Invalid type for name: {}", repr))
                        })?;
                        let v = tuple.get_item(1)?;
                        process_file_item(py, &name, &v, &mut fields, &mimetypes)?;
                    }
                }
            }
        }

        let mut instance = MultipartStream {
            boundary,
            fields,
            buffer: Vec::new(),
            position: 0,
        };
        instance.buffer = instance.render_full_content();
        Ok(instance)
    }

    fn get_headers(&self) -> PyResult<crate::models::Headers> {
        let ct = format!("multipart/form-data; boundary={}", self.boundary);
        let py_headers = Python::attach(|py| {
            let d = PyDict::new(py);
            d.set_item("Content-Type", ct)?;
            crate::models::Headers::create(Some(&d.as_any()), "utf-8")
        })?;
        Ok(py_headers)
    }

    #[getter]
    pub fn content_type(&self) -> String {
        format!("multipart/form-data; boundary={}", self.boundary)
    }

    pub fn get_content(&self) -> Vec<u8> {
        self.buffer.clone()
    }

    // Iterator implementation for Python
    fn __iter__(slf: PyRef<'_, Self>) -> PyRef<'_, Self> {
        slf
    }

    fn __next__(&mut self) -> Option<Vec<u8>> {
        if self.position >= self.buffer.len() {
            return None;
        }
        let chunk_size = 65536; // 64KB chunks
        let end = std::cmp::min(self.position + chunk_size, self.buffer.len());
        let chunk = self.buffer[self.position..end].to_vec();
        self.position = end;
        Some(chunk)
    }
}

pub fn register(m: &Bound<'_, PyModule>) -> PyResult<()> {
    m.add_class::<DataField>()?;
    m.add_class::<FileField>()?;
    m.add_class::<MultipartStream>()?;
    Ok(())
}
