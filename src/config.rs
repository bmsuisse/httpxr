use pyo3::exceptions::PyValueError;
use pyo3::prelude::*;
use pyo3::types::{PyModule, PyTuple};
use std::ffi::CStr;

/// Default timeout (5 seconds for all operations)
#[allow(dead_code)]
pub const DEFAULT_TIMEOUT: f64 = 5.0;
pub const DEFAULT_MAX_REDIRECTS: u32 = 20;
#[allow(dead_code)]
pub const DEFAULT_MAX_CONNECTIONS: u32 = 100;
#[allow(dead_code)]
pub const DEFAULT_MAX_KEEPALIVE: u32 = 20;
pub const DEFAULT_KEEPALIVE_EXPIRY: f64 = 5.0;

/// Timeout configuration.
#[pyclass(from_py_object)]
#[derive(Clone, Debug)]
pub struct Timeout {
    #[pyo3(get, set)]
    pub connect: Option<f64>,
    #[pyo3(get, set)]
    pub read: Option<f64>,
    #[pyo3(get, set)]
    pub write: Option<f64>,
    #[pyo3(get, set)]
    pub pool: Option<f64>,
}

#[pymethods]
impl Timeout {
    #[new]
    #[pyo3(signature = (timeout=None, *, connect=None, read=None, write=None, pool=None))]
    pub fn new(
        _py: Python<'_>,
        timeout: Option<&Bound<'_, PyAny>>,
        connect: Option<f64>,
        read: Option<f64>,
        write: Option<f64>,
        pool: Option<f64>,
    ) -> PyResult<Self> {
        let (mut c, mut r, mut w, mut p) = (connect, read, write, pool);

        if let Some(t) = timeout {
            if t.is_none() {
            } else if let Ok(existing) = t.extract::<Timeout>() {
                return Ok(existing);
            } else if let Ok(tuple) = t.cast::<PyTuple>() {
                let len = tuple.len();
                if len > 0 {
                    if let Ok(val) = tuple.get_item(0).unwrap().extract::<f64>() {
                        c = c.or(Some(val));
                    }
                }
                if len > 1 {
                    if let Ok(val) = tuple.get_item(1).unwrap().extract::<f64>() {
                        r = r.or(Some(val));
                    }
                }
                if len > 2 {
                    if let Ok(val) = tuple.get_item(2).unwrap().extract::<f64>() {
                        w = w.or(Some(val));
                    }
                }
                if len > 3 {
                    if let Ok(val) = tuple.get_item(3).unwrap().extract::<f64>() {
                        p = p.or(Some(val));
                    }
                }
            } else if let Ok(val) = t.extract::<f64>() {
                c = c.or(Some(val));
                r = r.or(Some(val));
                w = w.or(Some(val));
                p = p.or(Some(val));
            }
        }

        Ok(Timeout {
            connect: c,
            read: r,
            write: w,
            pool: p,
        })
    }

    fn __eq__(&self, other: &Timeout) -> bool {
        self.connect == other.connect
            && self.read == other.read
            && self.write == other.write
            && self.pool == other.pool
    }

    fn __repr__(&self) -> String {
        fn fmt_opt(v: Option<f64>) -> String {
            match v {
                Some(f) => {
                    if f == f.floor() {
                        format!("{:.1}", f)
                    } else {
                        format!("{}", f)
                    }
                }
                None => "None".to_string(),
            }
        }
        if self.connect == self.read
            && self.read == self.write
            && self.write == self.pool
            && self.connect.is_some()
        {
            format!("Timeout(timeout={})", fmt_opt(self.connect))
        } else {
            format!(
                "Timeout(connect={}, read={}, write={}, pool={})",
                fmt_opt(self.connect),
                fmt_opt(self.read),
                fmt_opt(self.write),
                fmt_opt(self.pool)
            )
        }
    }
}

/// Connection pool limits.
#[pyclass(from_py_object)]
#[derive(Clone, Debug)]
pub struct Limits {
    #[pyo3(get, set)]
    pub max_connections: Option<u32>,
    #[pyo3(get, set)]
    pub max_keepalive_connections: Option<u32>,
    #[pyo3(get, set)]
    pub keepalive_expiry: Option<f64>,
}

#[pymethods]
impl Limits {
    #[new]
    #[pyo3(signature = (*, max_connections=None, max_keepalive_connections=None, keepalive_expiry=5.0))]
    fn new(
        max_connections: Option<u32>,
        max_keepalive_connections: Option<u32>,
        keepalive_expiry: Option<f64>,
    ) -> Self {
        Limits {
            max_connections,
            max_keepalive_connections,
            keepalive_expiry: keepalive_expiry.or(Some(DEFAULT_KEEPALIVE_EXPIRY)),
        }
    }

    fn __eq__(&self, other: &Limits) -> bool {
        self.max_connections == other.max_connections
            && self.max_keepalive_connections == other.max_keepalive_connections
            && self.keepalive_expiry == other.keepalive_expiry
    }

    fn __repr__(&self) -> String {
        fn fmt_opt_u32(v: Option<u32>) -> String {
            match v {
                Some(n) => format!("{}", n),
                None => "None".to_string(),
            }
        }
        fn fmt_opt_f64(v: Option<f64>) -> String {
            match v {
                Some(f) => {
                    if f == f.floor() {
                        format!("{:.1}", f)
                    } else {
                        format!("{}", f)
                    }
                }
                None => "None".to_string(),
            }
        }
        format!(
            "Limits(max_connections={}, max_keepalive_connections={}, keepalive_expiry={})",
            fmt_opt_u32(self.max_connections),
            fmt_opt_u32(self.max_keepalive_connections),
            fmt_opt_f64(self.keepalive_expiry)
        )
    }
}

/// Proxy configuration.
#[pyclass(from_py_object)]
#[derive(Clone, Debug)]
pub struct Proxy {
    #[pyo3(get, set)]
    pub url: String,
    #[pyo3(get, set)]
    pub auth: Option<(String, String)>,
    #[pyo3(get, set)]
    pub headers: Option<crate::models::Headers>,
}
impl Proxy {
    pub fn create_from_url(url: &str) -> PyResult<Self> {
        let valid_schemes = ["http://", "https://", "socks5://", "socks5h://"];
        if !url.contains("://") || !valid_schemes.iter().any(|s| url.starts_with(s)) {
            return Err(PyValueError::new_err(
                format!("Proxy URL must have a supported scheme (http://, https://, socks5://, socks5h://), got: {}", url)
            ));
        }
        let mut auth = None;
        let clean_url = if url.contains('@') {
            if let Some(scheme_end) = url.find("://") {
                let after_scheme = &url[scheme_end + 3..];
                if let Some(at_pos) = after_scheme.find('@') {
                    let userinfo = &after_scheme[..at_pos];
                    if let Some(colon_pos) = userinfo.find(':') {
                        auth = Some((
                            userinfo[..colon_pos].to_string(),
                            userinfo[colon_pos + 1..].to_string(),
                        ));
                    } else {
                        auth = Some((userinfo.to_string(), "".to_string()));
                    }
                    format!("{}{}", &url[..scheme_end + 3], &after_scheme[at_pos + 1..])
                } else {
                    url.to_string()
                }
            } else {
                url.to_string()
            }
        } else {
            url.to_string()
        };
        Ok(Proxy {
            url: clean_url,
            auth: auth,
            headers: Some(crate::models::Headers::create(None, "utf-8")?),
        })
    }
}

#[pymethods]
impl Proxy {
    #[new]
    #[pyo3(signature = (url, *, auth=None, headers=None, ssl_context=None))]
    fn new(
        url: &Bound<'_, PyAny>,
        auth: Option<(String, String)>,
        headers: Option<&Bound<'_, PyAny>>,
        ssl_context: Option<&Bound<'_, PyAny>>,
    ) -> PyResult<Self> {
        let _ = ssl_context;
        let url_str: String = url.str()?.extract()?;

        let valid_schemes = ["http://", "https://", "socks5://", "socks5h://"];
        if !url_str.contains("://") || !valid_schemes.iter().any(|s| url_str.starts_with(s)) {
            return Err(PyValueError::new_err("Proxy URL must have a supported scheme (http://, https://, socks5://, socks5h://)."));
        }

        let mut extracted_auth = auth;

        let clean_url = if url_str.contains('@') {
            if let Some(scheme_end) = url_str.find("://") {
                let after_scheme = &url_str[scheme_end + 3..];
                if let Some(at_pos) = after_scheme.find('@') {
                    let userinfo = &after_scheme[..at_pos];
                    if extracted_auth.is_none() {
                        if let Some(colon_pos) = userinfo.find(':') {
                            extracted_auth = Some((
                                userinfo[..colon_pos].to_string(),
                                userinfo[colon_pos + 1..].to_string(),
                            ));
                        } else {
                            extracted_auth = Some((userinfo.to_string(), "".to_string()));
                        }
                    }
                    format!(
                        "{}{}",
                        &url_str[..scheme_end + 3],
                        &after_scheme[at_pos + 1..]
                    )
                } else {
                    url_str.clone()
                }
            } else {
                url_str.clone()
            }
        } else {
            url_str
        };
        let hdrs = if let Some(h) = headers {
            Some(crate::models::Headers::create(Some(h), "utf-8")?)
        } else {
            Some(crate::models::Headers::create(None, "utf-8")?)
        };
        Ok(Proxy {
            url: clean_url,
            auth: extracted_auth,
            headers: hdrs,
        })
    }

    fn __repr__(&self) -> String {
        if let Some(ref auth) = self.auth {
            format!("Proxy('{}', auth=('{}', '********'))", self.url, auth.0)
        } else {
            format!("Proxy('{}')", self.url)
        }
    }
}

/// Create an SSL context. Delegates to Python's ssl module.
#[pyfunction]
#[pyo3(signature = (verify=None, cert=None, trust_env=None))]
pub fn create_ssl_context(
    py: Python<'_>,
    verify: Option<&Bound<'_, PyAny>>,
    cert: Option<&Bound<'_, PyAny>>,
    trust_env: Option<bool>,
) -> PyResult<Py<PyAny>> {
    let _ = trust_env;
    let ssl = py.import("ssl")?;
    let ctx = ssl.call_method0("create_default_context")?;

    if let Some(v) = verify {
        if let Ok(false_val) = v.extract::<bool>() {
            if !false_val {
                ctx.setattr("check_hostname", false)?;
                ctx.setattr("verify_mode", ssl.getattr("CERT_NONE")?)?;
            }
        } else if let Ok(path) = v.extract::<String>() {
            ctx.call_method1("load_verify_locations", (path.clone(),))?;
            ctx.setattr("_cafile", path)?;
        }
    }

    let patch_code = CStr::from_bytes_with_nul(
        b"\
def patch(ctx):
    original_load = ctx.load_verify_locations
    def load_verify_locations(cafile=None, capath=None, cadata=None):
        if cafile:
            ctx._cafile = cafile
        return original_load(cafile=cafile, capath=capath, cadata=cadata)
    ctx.load_verify_locations = load_verify_locations
    return ctx
\0",
    )
    .unwrap();
    let patch_func =
        PyModule::from_code(py, patch_code, c"patcher.py", c"patcher")?.getattr("patch")?;
    patch_func.call1((&ctx,))?;

    if let Some(c) = cert {
        if !c.is_none() {
            if let Ok(cert_path) = c.extract::<String>() {
                ctx.call_method1("load_cert_chain", (cert_path,))?;
            } else if let Ok(tuple) = c.extract::<(String, String)>() {
                ctx.call_method1("load_cert_chain", (tuple.0, tuple.1))?;
            }
        }
    }

    Ok(ctx.into())
}

/// Retry configuration for ETL pipelines.
/// When attached to a Client or AsyncClient, requests that fail with
/// status codes in `retry_on_status` will be retried up to `max_retries`
/// times with exponential backoff.
#[pyclass(from_py_object)]
#[derive(Clone, Debug)]
pub struct RetryConfig {
    #[pyo3(get, set)]
    pub max_retries: u32,
    #[pyo3(get, set)]
    pub backoff_factor: f64,
    #[pyo3(get, set)]
    pub retry_on_status: Vec<u16>,
    #[pyo3(get, set)]
    pub jitter: bool,
}

#[pymethods]
impl RetryConfig {
    #[new]
    #[pyo3(signature = (max_retries=3, backoff_factor=0.5, retry_on_status=None, jitter=true))]
    fn new(
        max_retries: u32,
        backoff_factor: f64,
        retry_on_status: Option<Vec<u16>>,
        jitter: bool,
    ) -> Self {
        RetryConfig {
            max_retries,
            backoff_factor,
            retry_on_status: retry_on_status.unwrap_or_else(|| vec![429, 500, 502, 503, 504]),
            jitter,
        }
    }

    fn __repr__(&self) -> String {
        format!(
            "RetryConfig(max_retries={}, backoff_factor={}, retry_on_status={:?}, jitter={})",
            self.max_retries, self.backoff_factor, self.retry_on_status, self.jitter
        )
    }

    fn __eq__(&self, other: &RetryConfig) -> bool {
        self.max_retries == other.max_retries
            && self.backoff_factor == other.backoff_factor
            && self.retry_on_status == other.retry_on_status
            && self.jitter == other.jitter
    }

    /// Calculate the delay for the given attempt number.
    fn delay_for_attempt(&self, attempt: u32) -> f64 {
        let base = self.backoff_factor * 2.0f64.powi(attempt as i32);
        if self.jitter {
            base * (0.5 + rand_jitter())
        } else {
            base
        }
    }

    /// Check if the given status code should trigger a retry.
    fn should_retry(&self, status_code: u16) -> bool {
        self.retry_on_status.contains(&status_code)
    }
}

/// Simple deterministic-ish jitter (0.0 to 1.0) without adding a rand dependency.
fn rand_jitter() -> f64 {
    use std::time::SystemTime;
    let nanos = SystemTime::now()
        .duration_since(SystemTime::UNIX_EPOCH)
        .unwrap_or_default()
        .subsec_nanos();
    (nanos as f64) / 1_000_000_000.0
}

/// Rate limiting configuration.
/// Limits the number of requests per second to avoid overwhelming APIs.
/// Can be combined with RetryConfig for automatic 429 back-off.
#[pyclass(from_py_object)]
#[derive(Clone, Debug)]
pub struct RateLimit {
    #[pyo3(get, set)]
    pub requests_per_second: f64,
    #[pyo3(get, set)]
    pub burst: u32,
    /// Internal: tracks the last request time for simple token bucket
    #[pyo3(get)]
    pub _tokens: f64,
    #[pyo3(get)]
    pub _last_refill: f64,
}

#[pymethods]
impl RateLimit {
    #[new]
    #[pyo3(signature = (requests_per_second=10.0, burst=None))]
    fn new(requests_per_second: f64, burst: Option<u32>) -> Self {
        let burst_val = burst.unwrap_or(requests_per_second.ceil() as u32);
        RateLimit {
            requests_per_second,
            burst: burst_val,
            _tokens: burst_val as f64,
            _last_refill: 0.0,
        }
    }

    fn __repr__(&self) -> String {
        format!(
            "RateLimit(requests_per_second={}, burst={})",
            self.requests_per_second, self.burst
        )
    }

    fn __eq__(&self, other: &RateLimit) -> bool {
        self.requests_per_second == other.requests_per_second
            && self.burst == other.burst
    }

    /// Calculate how long to wait before the next request can be sent.
    fn wait_time(&mut self) -> f64 {
        use std::time::SystemTime;
        let now = SystemTime::now()
            .duration_since(SystemTime::UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs_f64();

        if self._last_refill == 0.0 {
            self._last_refill = now;
            self._tokens = self.burst as f64;
        }

        // Refill tokens
        let elapsed = now - self._last_refill;
        self._tokens = (self._tokens + elapsed * self.requests_per_second)
            .min(self.burst as f64);
        self._last_refill = now;

        if self._tokens >= 1.0 {
            self._tokens -= 1.0;
            0.0
        } else {
            (1.0 - self._tokens) / self.requests_per_second
        }
    }
}

pub fn register(m: &Bound<'_, PyModule>) -> PyResult<()> {
    m.add_class::<Timeout>()?;
    m.add_class::<Limits>()?;
    m.add_class::<Proxy>()?;
    m.add_class::<RetryConfig>()?;
    m.add_class::<RateLimit>()?;
    m.add_function(wrap_pyfunction!(create_ssl_context, m)?)?;
    let py = m.py();
    let default_bound = pyo3::types::PyFloat::new(py, 5.0);
    m.add(
        "DEFAULT_TIMEOUT_CONFIG",
        Timeout::new(py, Some(default_bound.as_any()), None, None, None, None)?,
    )?;
    m.add("DEFAULT_LIMITS", Limits::new(None, None, None))?;
    m.add("DEFAULT_MAX_REDIRECTS", DEFAULT_MAX_REDIRECTS)?;
    Ok(())
}
