use pyo3::prelude::*;
use pyo3::types::{PyDict, PyTuple};
use base64::Engine;
use base64::engine::general_purpose::STANDARD as BASE64_STANDARD;
use sha1::Sha1;
use sha2::{Sha256, Sha512};
use std::sync::{Arc, Mutex};

/// Auth base class
#[pyclass(from_py_object, subclass)]
#[derive(Clone)]
pub struct Auth;

#[pymethods]
impl Auth {
    #[new]
    #[pyo3(signature = (*_args, **_kwargs))]
    fn new(_args: &Bound<'_, PyTuple>, _kwargs: Option<&Bound<'_, PyDict>>) -> Self {
        Auth
    }

    fn sync_auth_flow(&self, py: Python<'_>, request: &crate::models::Request) -> PyResult<Py<PyAny>> {
        // Default: yield the request unchanged (generator)
        let flow = SyncAuthFlow { request: Some(request.clone()) };
        Ok(Py::new(py, flow)?.into_any())
    }
}

#[pyclass]
struct SyncAuthFlow {
    request: Option<crate::models::Request>,
}

#[pymethods]
impl SyncAuthFlow {
    fn __iter__(slf: PyRef<'_, Self>) -> PyRef<'_, Self> { slf }
    fn __next__(&mut self, py: Python<'_>) -> Option<Py<PyAny>> {
        if let Some(req) = self.request.take() {
            Some(req.into_pyobject(py).unwrap().into())
        } else {
            None
        }
    }
    fn send(&mut self, _response: &Bound<'_, PyAny>) -> PyResult<Py<PyAny>> {
        Err(pyo3::exceptions::PyStopIteration::new_err(()))
    }
}

/// Basic HTTP authentication.
#[pyclass(from_py_object, extends=Auth)]
#[derive(Clone)]
pub struct BasicAuth {
    pub username: String,
    pub password: String,
}

#[pymethods]
impl BasicAuth {
    #[new]
    fn new(username: &str, password: &str) -> (Self, Auth) {
        (
            BasicAuth {
                username: username.to_string(),
                password: password.to_string(),
            },
            Auth,
        )
    }

    fn sync_auth_flow(&self, py: Python<'_>, request: &mut crate::models::Request) -> PyResult<Py<PyAny>> {
        let credentials = format!("{}:{}", self.username, self.password);
        let encoded = BASE64_STANDARD.encode(credentials.as_bytes());
        let auth_header = format!("Basic {}", encoded);
        request.headers.bind(py).borrow_mut().set_header("authorization", &auth_header);
        
        let flow = BasicAuthFlow { request: Some(request.clone()) };
        Ok(Py::new(py, flow)?.into_any())
    }
}

#[pyclass]
struct BasicAuthFlow {
    request: Option<crate::models::Request>,
}

#[pymethods]
impl BasicAuthFlow {
    fn __iter__(slf: PyRef<'_, Self>) -> PyRef<'_, Self> { slf }
    fn __next__(&mut self, py: Python<'_>) -> Option<Py<PyAny>> {
        if let Some(req) = self.request.take() {
            Some(req.into_pyobject(py).unwrap().into())
        } else {
            None
        }
    }
    fn send(&mut self, _response: &Bound<'_, PyAny>) -> PyResult<Py<PyAny>> {
        Err(pyo3::exceptions::PyStopIteration::new_err(()))
    }
}

/// Digest HTTP authentication.
#[pyclass(from_py_object, extends=Auth, dict)]
#[derive(Clone)]
pub struct DigestAuth {
    pub username: String,
    pub password: String,
    pub last_nonce: Arc<Mutex<Option<String>>>,
    pub nonce_count: Arc<Mutex<u32>>,
    pub cached_realm: Arc<Mutex<Option<String>>>,
    pub cached_qop: Arc<Mutex<Option<String>>>,
    pub cached_opaque: Arc<Mutex<Option<String>>>,
    pub cached_algorithm: Arc<Mutex<Option<String>>>,
}

#[pymethods]
impl DigestAuth {
    #[new]
    fn new(username: &str, password: &str) -> (Self, Auth) {
        (
            DigestAuth {
                username: username.to_string(),
                password: password.to_string(),
                last_nonce: Arc::new(Mutex::new(None)),
                nonce_count: Arc::new(Mutex::new(0)),
                cached_realm: Arc::new(Mutex::new(None)),
                cached_qop: Arc::new(Mutex::new(None)),
                cached_opaque: Arc::new(Mutex::new(None)),
                cached_algorithm: Arc::new(Mutex::new(None)),
            },
            Auth,
        )
    }

    fn sync_auth_flow(slf: PyRef<'_, Self>, py: Python<'_>, request: &crate::models::Request) -> PyResult<Py<PyAny>> {
        let auth: Py<DigestAuth> = slf.into();
        
        // Check if we have a cached nonce: if so, build auth header immediately
        let has_cached_nonce = {
            let auth_bound = auth.bind(py);
            let auth_ref = auth_bound.borrow();
            let last_nonce = auth_ref.last_nonce.lock().unwrap();
            last_nonce.is_some()
        };
        
        let flow = DigestAuthFlow {
            auth, 
            request: request.clone(),
            state: if has_cached_nonce { 3 } else { 0 }, // 3 = has cached nonce, need to build
            pending_response: None,
        };
        Ok(Py::new(py, flow)?.into_any())
    }

    fn _get_client_nonce(&self, _nonce_count: u32, _nonce: &str) -> String {
        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        format!("{:x}", now)
    }

    /// Build digest auth header from a challenge.
    fn build_auth_header(
        &self,
        method: &str,
        uri: &str,
        realm: &str,
        nonce: &str,
        qop: Option<&str>,
        opaque: Option<&str>,
        algorithm: Option<&str>,
        nonce_count: u32,
        cnonce: &str,
    ) -> PyResult<String> {
        use md5::{Md5, Digest as Md5Digest};

        let algo = algorithm.unwrap_or("MD5");
        let is_sess = algo.to_uppercase().ends_with("-SESS");
        let base_algo = algo.to_uppercase().replace("-SESS", "");

        let hash = |data: &str| -> String {
            match base_algo.as_str() {
                "SHA-256" => {
                    let mut hasher = Sha256::new();
                    hasher.update(data.as_bytes());
                    format!("{:x}", hasher.finalize())
                }
                "SHA-512" => {
                    let mut hasher = Sha512::new();
                    hasher.update(data.as_bytes());
                    format!("{:x}", hasher.finalize())
                }
                "SHA" => {
                    let mut hasher = Sha1::new();
                    hasher.update(data.as_bytes());
                    format!("{:x}", hasher.finalize())
                }
                _ => {
                    // MD5 and default
                    let mut hasher = Md5::new();
                    hasher.update(data.as_bytes());
                    format!("{:x}", hasher.finalize())
                }
            }
        };

        let ha1_base = hash(&format!("{}:{}:{}", self.username, realm, self.password));
        let ha1 = if is_sess {
            hash(&format!("{}:{}:{}", ha1_base, nonce, cnonce))
        } else {
            ha1_base
        };
        let ha2 = hash(&format!("{}:{}", method, uri));

        let response = if let Some(qop_val) = qop {
            let nc = format!("{:08}", nonce_count);
            let response = hash(&format!("{}:{}:{}:{}:{}:{}", ha1, nonce, nc, cnonce, qop_val, ha2));
            format!(
                "Digest username=\"{}\", realm=\"{}\", nonce=\"{}\", uri=\"{}\", qop={}, nc={}, cnonce=\"{}\", response=\"{}\", algorithm={}{}",
                self.username, realm, nonce, uri, qop_val, nc, cnonce, response, algo,
                opaque.map(|o| format!(", opaque=\"{}\"", o)).unwrap_or_default()
            )
        } else {
            let response = hash(&format!("{}:{}:{}", ha1, nonce, ha2));
            format!(
                "Digest username=\"{}\", realm=\"{}\", nonce=\"{}\", uri=\"{}\", response=\"{}\", algorithm={}{}",
                self.username, realm, nonce, uri, response, algo,
                opaque.map(|o| format!(", opaque=\"{}\"", o)).unwrap_or_default()
            )
        };

        Ok(response)
    }
}

#[pyclass]
struct DigestAuthFlow {
    auth: Py<DigestAuth>,
    request: crate::models::Request,
    state: u8, // 0 = initial, 1 = waiting for response, 2 = done
    pending_response: Option<Py<PyAny>>,
}

#[pymethods]
impl DigestAuthFlow {
    fn __iter__(slf: PyRef<'_, Self>) -> PyRef<'_, Self> { slf }

    fn __next__(&mut self, py: Python<'_>) -> PyResult<Option<Py<PyAny>>> {
        match self.state {
            0 => {
                // First call: yield the initial request (without auth)
                self.state = 1;
                Ok(Some(self.request.clone().into_pyobject(py).unwrap().into()))
            }
            1 => {
                // If send() was called, pending_response will be set
                if let Some(response) = self.pending_response.take() {
                    let result = self.process_response(response.bind(py))?;
                    self.state = 2;
                    Ok(Some(result))
                } else {
                    // No response was sent, stop iteration
                    Ok(None)
                }
            }
            3 => {
                // Has cached nonce: build authenticated request immediately
                self.state = 1; // Can still receive 401 and retry
                let auth_bound = self.auth.bind(py);
                let auth_ref = auth_bound.borrow();
                let last_nonce = auth_ref.last_nonce.lock().unwrap().clone();
                if let Some(nonce_val) = last_nonce {
                    drop(auth_ref); // Release borrow before calling process
                    let result = self.build_cached_auth_request(py, &nonce_val)?;
                    Ok(Some(result))
                } else {
                    // No cached nonce available, yield request without auth
                    Ok(Some(self.request.clone().into_pyobject(py).unwrap().into()))
                }
            }
            _ => Ok(None), // Done
        }
    }

    fn send(&mut self, _py: Python<'_>, response: &Bound<'_, PyAny>) -> PyResult<Py<PyAny>> {
        if self.state != 1 {
            return Err(pyo3::exceptions::PyStopIteration::new_err(()));
        }
        // Process the response directly in send()
        let result = self.process_response(response)?;
        self.state = 2;
        Ok(result)
    }
}

impl DigestAuthFlow {
    fn process_response(&mut self, response: &Bound<'_, PyAny>) -> PyResult<Py<PyAny>> {
        let status_code: u16 = response.getattr("status_code")?.extract()?;
        if status_code != 401 {
            return Err(pyo3::exceptions::PyStopIteration::new_err(()));
        }

        let headers = response.getattr("headers")?;
        let www_authenticate: Option<String> = if let Ok(val) = headers.call_method1("get", ("www-authenticate",)) {
             val.extract::<Option<String>>()?
        } else {
            None
        };

        let challenge = if let Some(c) = www_authenticate {
            c
        } else {
             return Err(pyo3::exceptions::PyStopIteration::new_err(()));
        };
        
        if !challenge.to_lowercase().starts_with("digest") {
             return Err(pyo3::exceptions::PyStopIteration::new_err(()));
        }

        // Parse challenge
        let mut realm = None;
        let mut nonce = None;
        let mut qop = None;
        let mut opaque = None;
        let mut algorithm = None;

        let params_str = challenge[7..].trim();
        
        let mut parts = Vec::new();
        let mut start = 0;
        let mut in_quote = false;
        let chars: Vec<char> = params_str.chars().collect();
        for (i, &c) in chars.iter().enumerate() {
            if c == '"' { in_quote = !in_quote; }
            else if c == ',' && !in_quote {
                parts.push(params_str[start..i].trim());
                start = i + 1;
            }
        }
        if start < params_str.len() {
             parts.push(params_str[start..].trim());
        }

        for part in parts {
            if let Some(idx) = part.find('=') {
                let key = part[..idx].trim();
                let val = part[idx+1..].trim().trim_matches('"');
                match key {
                    "realm" => realm = Some(val.to_string()),
                    "nonce" => nonce = Some(val.to_string()),
                    "qop" => {
                        // Parse QOP options and select the best one
                        let qop_options: Vec<&str> = val.split(',').map(|s| s.trim()).collect();
                        if qop_options.iter().any(|q| *q == "auth") {
                            qop = Some("auth".to_string());
                        } else if qop_options.iter().any(|q| *q == "auth-int") {
                            return Err(pyo3::exceptions::PyNotImplementedError::new_err(
                                "Digest auth with qop='auth-int' is not implemented"
                            ));
                        } else {
                            return Err(crate::exceptions::ProtocolError::new_err(
                                format!("Unexpected qop value in digest challenge: {}", val)
                            ));
                        }
                    },
                    "opaque" => opaque = Some(val.to_string()),
                    "algorithm" => algorithm = Some(val.to_string()),
                    _ => {}
                }
            }
        }

        if realm.is_none() || nonce.is_none() {
             return Err(crate::exceptions::ProtocolError::new_err(
                 "Malformed Digest authentication challenge: missing required fields (realm, nonce)"
             ));
        }
        
        let realm_val = realm.unwrap();
        let nonce_val = nonce.unwrap();

        let py = response.py();
        let auth_bound = self.auth.bind(py);

        let nc = {
            let auth_rust = auth_bound.borrow();
            let mut nc_lock = auth_rust.nonce_count.lock().unwrap();
            let mut last_nonce_lock = auth_rust.last_nonce.lock().unwrap();
            
            if last_nonce_lock.as_deref() != Some(&nonce_val) {
                *last_nonce_lock = Some(nonce_val.clone());
                *nc_lock = 1;
            } else {
                *nc_lock += 1;
            }
            
            // Cache challenge values for subsequent flows
            *auth_rust.cached_realm.lock().unwrap() = Some(realm_val.clone());
            *auth_rust.cached_qop.lock().unwrap() = qop.clone();
            *auth_rust.cached_opaque.lock().unwrap() = opaque.clone();
            *auth_rust.cached_algorithm.lock().unwrap() = algorithm.clone();
            
            *nc_lock
        };

        let cnonce: String = if let Ok(res) = auth_bound.call_method1("_get_client_nonce", (nc, nonce_val.as_bytes())) {
            // The method may return bytes or str
            if let Ok(s) = res.extract::<String>() {
                s
            } else if let Ok(b) = res.extract::<Vec<u8>>() {
                String::from_utf8_lossy(&b).to_string()
            } else {
                auth_bound.borrow()._get_client_nonce(nc, &nonce_val)
            }
        } else {
            auth_bound.borrow()._get_client_nonce(nc, &nonce_val)
        };
        
        let auth_header = auth_bound.borrow().build_auth_header(
            &self.request.method,
            &self.request.url.raw_path_str(), 
            &realm_val,
            &nonce_val,
            qop.as_deref(),
            opaque.as_deref(),
            algorithm.as_deref(),
            nc,
            &cnonce
        )?;

        let new_request = self.request.clone();
        new_request.headers.bind(py).borrow_mut().set_header("authorization", &auth_header);
        
        if let Ok(cookies) = response.getattr("cookies") {
             if let Ok(jar) = cookies.getattr("jar") {
                 let mut cookie_list = Vec::new();
                 if let Ok(iterator) = jar.try_iter() {
                     for cookie in iterator {
                         if let Ok(cookie) = cookie {
                             if let (Ok(name), Ok(value)) = (cookie.getattr("name"), cookie.getattr("value")) {
                                 if let (Ok(n), Ok(v)) = (name.extract::<String>(), value.extract::<String>()) {
                                     cookie_list.push(format!("{}={}", n, v));
                                 }
                             }
                         }
                     }
                 }
                 if !cookie_list.is_empty() {
                     let cookie_str = cookie_list.join("; ");
                     if new_request.headers.bind(py).borrow().contains_header("cookie") {
                          if let Some(existing) = new_request.headers.bind(py).borrow().get_first_value("cookie") {
                              new_request.headers.bind(py).borrow_mut().set_header("cookie", &format!("{}; {}", existing, cookie_str));
                          } else {
                              new_request.headers.bind(py).borrow_mut().set_header("cookie", &cookie_str);
                          }
                     } else {
                          new_request.headers.bind(py).borrow_mut().set_header("cookie", &cookie_str);
                     }
                 }
             }
        }

        Ok(new_request.into_pyobject(response.py()).unwrap().into())
    }

    fn build_cached_auth_request(&mut self, py: Python<'_>, nonce_val: &str) -> PyResult<Py<PyAny>> {
        let auth_bound = self.auth.bind(py);
        
        let (realm_val, qop, opaque, algorithm, nc) = {
            let auth_ref = auth_bound.borrow();
            let realm = auth_ref.cached_realm.lock().unwrap().clone();
            let qop = auth_ref.cached_qop.lock().unwrap().clone();
            let opaque = auth_ref.cached_opaque.lock().unwrap().clone();
            let algorithm = auth_ref.cached_algorithm.lock().unwrap().clone();
            
            // Increment nonce count
            let mut nc_lock = auth_ref.nonce_count.lock().unwrap();
            *nc_lock += 1;
            let nc = *nc_lock;
            
            (realm.unwrap_or_default(), qop, opaque, algorithm, nc)
        };
        
        let cnonce: String = if let Ok(res) = auth_bound.call_method1("_get_client_nonce", (nc, nonce_val.as_bytes())) {
            if let Ok(s) = res.extract::<String>() {
                s
            } else if let Ok(b) = res.extract::<Vec<u8>>() {
                String::from_utf8_lossy(&b).to_string()
            } else {
                auth_bound.borrow()._get_client_nonce(nc, nonce_val)
            }
        } else {
            auth_bound.borrow()._get_client_nonce(nc, nonce_val)
        };
        
        let auth_header = auth_bound.borrow().build_auth_header(
            &self.request.method,
            &self.request.url.raw_path_str(),
            &realm_val,
            nonce_val,
            qop.as_deref(),
            opaque.as_deref(),
            algorithm.as_deref(),
            nc,
            &cnonce
        )?;
        
        let new_request = self.request.clone();
        new_request.headers.bind(py).borrow_mut().set_header("authorization", &auth_header);
        
        Ok(new_request.into_pyobject(py).unwrap().into())
    }
}


/// Function-based authentication.
#[pyclass(extends=Auth)]
pub struct FunctionAuth {
    func: Py<PyAny>,
}

#[pymethods]
impl FunctionAuth {
    #[new]
    fn new(func: Py<PyAny>) -> (Self, Auth) {
        (FunctionAuth { func }, Auth)
    }

    fn sync_auth_flow(&self, py: Python<'_>, request: &mut crate::models::Request) -> PyResult<Py<PyAny>> {
        let result = self.func.call1(py, (request.clone(),))?;
        // Logic for function auth might need to be wrapped in a generator if it returns one?
        // Standard httpx FunctionAuth usually expects the function to yield requests.
        // If the user function returns a generator, we return it.
        // If it returns a request/list, we wrap it.
        // Check if result is iterator?
        // For now, assume it behaves like existing implementation but we need to return iterator.
        // Existing impl returned a list. We can wrap that list in an iterator.
        
        // Actually, if `self.func` returns a generator, we just return it.
        // If it returns a request/None, we probably need to handle it.
        // Let's assume for now the user function follows the protocol.
        // But to be safe and compatible with previous list-return implementation:
        // If it returns a Request or list, we make an iterator.
        
        let iter_check = result.call_method0(py, "__iter__");
        if iter_check.is_ok() {
            return Ok(result)
        }
        
        // Wrap single result in 1-item iterator
        let list = pyo3::types::PyList::new(py, &[result])?;
        let iter = list.call_method0("__iter__")?;
        Ok(iter.into())
    }
}

/// NetRC authentication.
#[pyclass(extends=Auth)]
pub struct NetRCAuth {
    netrc_info: Py<PyAny>,
}

#[pymethods]
impl NetRCAuth {
    #[new]
    #[pyo3(signature = (file=None))]
    fn new(py: Python<'_>, file: Option<&str>) -> PyResult<(Self, Auth)> {
        let netrc = py.import("netrc")?;
        let info = if let Some(f) = file {
            netrc.call_method1("netrc", (f,))?
        } else {
            netrc.call_method0("netrc")?
        };
        Ok((
            NetRCAuth { netrc_info: info.into() },
            Auth,
        ))
    }

    fn sync_auth_flow(&self, py: Python<'_>, request: &mut crate::models::Request) -> PyResult<Py<PyAny>> {
        let host = request.url.get_raw_host();
        let info = self.netrc_info.bind(py);
        let auth_tuple = info.call_method1("authenticators", (host,))?;

        if !auth_tuple.is_none() {
            let username: String = auth_tuple.get_item(0)?.extract()?;
            let password: String = auth_tuple.get_item(2)?.extract()?;

            let credentials = format!("{}:{}", username, password);
            let encoded = BASE64_STANDARD.encode(credentials.as_bytes());
            let auth_header = format!("Basic {}", encoded);
            request.headers.bind(py).borrow_mut().set_header("authorization", &auth_header);
        }

        let flow = BasicAuthFlow { request: Some(request.clone()) };
        Ok(Py::new(py, flow)?.into_any())
    }
}

pub fn register(m: &Bound<'_, PyModule>) -> PyResult<()> {
    m.add_class::<Auth>()?;
    m.add_class::<BasicAuth>()?;
    m.add_class::<DigestAuth>()?;
    m.add_class::<FunctionAuth>()?;
    m.add_class::<NetRCAuth>()?;

    Ok(())
}
