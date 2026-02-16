use pyo3::prelude::*;
use pyo3::types::{PyBytes, PyDict, PyList, PyTuple};

use crate::exceptions::InvalidURL;
use crate::urlparse::{path_safe, quote, urlparse_impl, userinfo_safe, validate_host, ParseResult};

fn check_control_chars(s: &str, position_offset: usize, component: Option<&str>) -> PyResult<()> {
    for (i, c) in s.char_indices() {
        if c.is_control() {
            let msg = if let Some(comp) = component {
                format!("Invalid non-printable ASCII character in URL {} component, {:?} at position {}.", comp, c, i + position_offset)
            } else {
                format!(
                    "Invalid non-printable ASCII character in URL, {:?} at position {}.",
                    c,
                    i + position_offset
                )
            };
            return Err(InvalidURL::new_err(msg));
        }
    }
    Ok(())
}

fn check_length(s: &str, limit: usize, component: Option<&str>) -> PyResult<()> {
    if s.len() > limit {
        let msg = if let Some(comp) = component {
            format!("URL component '{}' too long", comp)
        } else {
            "URL too long".to_string()
        };
        return Err(InvalidURL::new_err(msg));
    }
    Ok(())
}

/// A URL class that wraps a ParseResult and provides a friendly API.
#[pyclass(from_py_object)]
#[derive(Clone, Debug)]
pub struct URL {
    pub(crate) parsed: ParseResult,
}

/// Public Rust-side helper methods
fn extract_str_or_bytes(obj: &Bound<'_, PyAny>) -> PyResult<String> {
    if let Ok(s) = obj.extract::<String>() {
        Ok(s)
    } else if let Ok(b) = obj.cast::<PyBytes>() {
        let bytes = b.as_bytes();
        std::str::from_utf8(bytes)
            .map(|s| s.to_string())
            .map_err(|e| pyo3::exceptions::PyUnicodeDecodeError::new_err(e.to_string()))
    } else {
        Err(pyo3::exceptions::PyTypeError::new_err(
            "Expected str or bytes",
        ))
    }
}

impl URL {
    pub fn create_from_str(url_str: &str) -> PyResult<Self> {
        let parsed = urlparse_impl(url_str, None)?;
        Ok(URL { parsed })
    }

    pub fn to_string(&self) -> String {
        self.parsed.to_url_string()
    }

    pub fn get_host(&self) -> String {
        self.parsed.host.clone()
    }

    pub fn get_raw_host(&self) -> &str {
        &self.parsed.host
    }

    pub fn is_relative_internal(&self) -> bool {
        self.parsed.scheme.is_empty() && self.parsed.host.is_empty()
    }

    pub fn join_relative(&self, url: &str) -> PyResult<URL> {
        let base_str = self.to_string();
        // Handle relative base URL by prepending dummy scheme/host
        let (is_relative, parse_base) = match url::Url::parse(&base_str) {
            Ok(u) => (false, u),
            Err(url::ParseError::RelativeUrlWithoutBase) => {
                let dummy = format!(
                    "http://dummy{}",
                    if base_str.starts_with('/') { "" } else { "/" }
                );
                let u = url::Url::parse(&format!("{}{}", dummy, base_str))
                    .map_err(|e| pyo3::exceptions::PyValueError::new_err(e.to_string()))?;
                (true, u)
            }
            Err(e) => return Err(pyo3::exceptions::PyValueError::new_err(e.to_string())),
        };

        // Extract explicit port from the redirect URL before joining
        // (url::Url normalizes away default ports like :443 for https)
        let explicit_port = {
            let url_trimmed = url.trim();
            if let Some(scheme_end) = url_trimmed.find("://") {
                let after_scheme = &url_trimmed[scheme_end + 3..];
                let authority_end = after_scheme.find('/').unwrap_or(after_scheme.len());
                let authority = &after_scheme[..authority_end];
                if let Some(colon_pos) = authority.rfind(':') {
                    authority[colon_pos + 1..].parse::<u16>().ok()
                } else {
                    None
                }
            } else {
                None
            }
        };

        let joined = match parse_base.join(url) {
            Ok(u) => u,
            Err(url::ParseError::EmptyHost) => {
                // If EmptyHost error, try to recover using base host
                if let Some(base_host) = parse_base.host_str() {
                    if url.starts_with("https://:") {
                        let new_url = format!("https://{}{}", base_host, &url[8..]);
                        parse_base
                            .join(&new_url)
                            .map_err(|e| pyo3::exceptions::PyValueError::new_err(e.to_string()))?
                    } else if url.starts_with("http://:") {
                        let new_url = format!("http://{}{}", base_host, &url[7..]);
                        parse_base
                            .join(&new_url)
                            .map_err(|e| pyo3::exceptions::PyValueError::new_err(e.to_string()))?
                    } else {
                        return Err(pyo3::exceptions::PyValueError::new_err("Empty host"));
                    }
                } else {
                    return Err(pyo3::exceptions::PyValueError::new_err("Empty host"));
                }
            }
            Err(e) => return Err(pyo3::exceptions::PyValueError::new_err(e.to_string())),
        };

        let final_url_obj = if joined.host_str().unwrap_or("").is_empty()
            && (joined.scheme() == "http" || joined.scheme() == "https")
        {
            // Existing fallback (if join succeeded but host empty)
            if let Some(base_host) = parse_base.host_str() {
                let mut j = joined.clone();
                if let Err(_) = j.set_host(Some(base_host)) {
                    joined
                } else {
                    j
                }
            } else {
                joined
            }
        } else {
            joined
        };

        let final_url = if is_relative {
            // Strip http://dummy
            let s = final_url_obj.as_str();
            let path_start = s.find("http://dummy").map(|i| i + 12).unwrap_or(0);
            &s[path_start..]
        } else {
            final_url_obj.as_str()
        };

        let mut parsed_url = urlparse_impl(final_url, None)?;

        // Restore explicit port if it was stripped by url::Url normalization
        if let Some(port) = explicit_port {
            if parsed_url.port.is_none() {
                parsed_url.port = Some(port);
            }
        }

        Ok(URL { parsed: parsed_url })
    }

    pub fn path_str(&self) -> &str {
        &self.parsed.path
    }

    /// Internal Rust helper: returns raw_path as a String (path + query + fragment)
    pub fn raw_path_str(&self) -> String {
        let mut rp = if self.parsed.path.is_empty()
            && (self.parsed.scheme == "http" || self.parsed.scheme == "https")
        {
            "/".to_string()
        } else {
            self.parsed.path.clone()
        };
        if let Some(ref q) = self.parsed.query {
            rp.push('?');
            rp.push_str(q);
        }
        rp
    }

    pub fn query_str(&self) -> Option<&str> {
        self.parsed.query.as_deref()
    }

    pub fn userinfo_str(&self) -> &str {
        &self.parsed.userinfo
    }

    pub fn get_scheme(&self) -> &str {
        &self.parsed.scheme
    }

    pub fn get_port(&self) -> Option<u16> {
        self.parsed.port
    }

    pub fn get_username(&self) -> &str {
        if let Some(pos) = self.parsed.userinfo.find(':') {
            &self.parsed.userinfo[..pos]
        } else {
            &self.parsed.userinfo
        }
    }

    pub fn get_password(&self) -> Option<&str> {
        if let Some(pos) = self.parsed.userinfo.find(':') {
            Some(&self.parsed.userinfo[pos + 1..])
        } else {
            None
        }
    }
}

#[pymethods]
impl URL {
    #[new]
    #[pyo3(signature = (url=None, *, params=None, scheme=None, host=None, port=None, path=None, query=None, fragment=None, userinfo=None, **kwargs))]
    pub fn new(
        url: Option<&Bound<'_, PyAny>>,
        params: Option<&Bound<'_, PyAny>>,
        scheme: Option<&str>,
        host: Option<&str>,
        port: Option<u16>,
        path: Option<&str>,
        query: Option<&str>,
        fragment: Option<&str>,
        userinfo: Option<&str>,
        kwargs: Option<&Bound<'_, PyDict>>,
    ) -> PyResult<Self> {
        if let Some(kw) = kwargs {
            if !kw.is_empty() {
                let key = kw.keys().get_item(0)?;
                let key_str: String = key.extract()?;
                return Err(pyo3::exceptions::PyTypeError::new_err(format!(
                    "'{}' is an invalid keyword argument for URL()",
                    key_str
                )));
            }
        }
        let url_str = if let Some(u) = url {
            if let Ok(existing) = u.extract::<URL>() {
                // Clone existing URL, then apply kwargs
                let mut parsed = existing.parsed.clone();
                if let Some(s) = scheme {
                    parsed.scheme = s.to_lowercase();
                }
                if let Some(h) = host {
                    let h_lower = h.to_lowercase();
                    parsed.host = if h_lower.starts_with('[') && h_lower.ends_with(']') {
                        h_lower[1..h_lower.len() - 1].to_string()
                    } else {
                        h_lower
                    };
                    if !parsed.host.contains(':') && !parsed.host.contains('%') {
                        validate_host(&parsed.host)?;
                    }
                }
                if let Some(p) = port {
                    if Some(p) == crate::urlparse::default_port(&parsed.scheme) {
                        parsed.port = None;
                    } else {
                        parsed.port = Some(p);
                    }
                }
                if let Some(p) = path {
                    parsed.path = quote(p, &path_safe());
                }
                if let Some(q) = query {
                    parsed.query = Some(q.to_string());
                }
                if let Some(f) = fragment {
                    parsed.fragment = Some(f.to_string());
                }
                if let Some(u) = userinfo {
                    parsed.userinfo = u.to_string();
                }

                // Apply query params if provided
                if let Some(p) = params {
                    if !p.is_none() {
                        let qp = QueryParams::create(Some(p))?;
                        parsed.query = Some(qp.encode());
                    }
                }

                return Ok(URL { parsed });
            } else if let Ok(s) = u.extract::<String>() {
                check_length(&s, 65536, None)?;
                check_control_chars(&s, 0, None)?;
                s
            } else if let Ok(b) = u.cast::<PyBytes>() {
                let s = std::str::from_utf8(b.as_bytes())
                    .map_err(|e| pyo3::exceptions::PyUnicodeDecodeError::new_err(e.to_string()))?
                    .to_string();
                check_length(&s, 65536, None)?;
                check_control_chars(&s, 0, None)?;
                s
            } else {
                return Err(pyo3::exceptions::PyTypeError::new_err(
                    "Expected str, bytes or URL instance",
                ));
            }
        } else {
            String::new()
        };

        // Component validation
        if let Some(p) = path {
            check_length(p, 65536, Some("path"))?;
            check_control_chars(p, 0, Some("path"))?;
        }
        if let Some(h) = host {
            check_length(h, 65536, Some("host"))?;
        }

        if let Some(s) = scheme {
            // Validate scheme characters
            if !s
                .chars()
                .all(|c| c.is_ascii_alphanumeric() || c == '+' || c == '-' || c == '.')
            {
                return Err(InvalidURL::new_err(format!(
                    "Invalid URL component 'scheme'"
                )));
            }
        }

        if let Some(p) = path {
            // Absolute URL checks
            match (scheme, host) {
                (Some(_), Some(_)) | (Some(_), None) => {
                    if !p.is_empty() && !p.starts_with('/') {
                        return Err(InvalidURL::new_err(
                            "For absolute URLs, path must be empty or begin with '/'",
                        ));
                    }
                }
                (None, _) => {
                    if p.starts_with("//") {
                        return Err(InvalidURL::new_err(
                            "Relative URLs cannot have a path starting with '//'",
                        ));
                    }
                    if p.starts_with(":") {
                        return Err(InvalidURL::new_err(
                            "Relative URLs cannot have a path starting with ':'",
                        ));
                    }
                }
            }
        }

        let mut parsed = urlparse_impl(&url_str, None)?;

        // Apply kwargs overrides
        if let Some(s) = scheme {
            parsed.scheme = s.to_lowercase();
        }
        // Validate host if it's not an IP address (heuristic: contains :) or encoded
        if !parsed.host.contains(':') && !parsed.host.contains('%') {
            validate_host(&parsed.host)?;
        }
        if let Some(h) = host {
            let h_lower = h.to_lowercase();
            // Strip brackets from IPv6 host if present
            parsed.host = if h_lower.starts_with('[') && h_lower.ends_with(']') {
                h_lower[1..h_lower.len() - 1].to_string()
            } else {
                h_lower
            };
            if !parsed.host.contains(':') && !parsed.host.contains('%') {
                validate_host(&parsed.host)?;
            }
        }
        if let Some(p) = port {
            if Some(p) == crate::urlparse::default_port(&parsed.scheme) {
                parsed.port = None;
            } else {
                parsed.port = Some(p);
            }
        }
        if let Some(p) = path {
            parsed.path = quote(p, &path_safe());
        }
        if let Some(q) = query {
            parsed.query = Some(q.to_string());
        }
        if let Some(f) = fragment {
            parsed.fragment = Some(f.to_string());
        }
        if let Some(u) = userinfo {
            parsed.userinfo = u.to_string();
        }

        // Apply query params if provided
        if let Some(p) = params {
            if !p.is_none() {
                let qp = QueryParams::create(Some(p))?;
                parsed.query = Some(qp.encode());
            }
        }

        Ok(URL { parsed })
    }

    #[getter]
    fn scheme(&self) -> &str {
        &self.parsed.scheme
    }
    #[getter]
    fn raw_scheme<'py>(&self, py: Python<'py>) -> Bound<'py, PyBytes> {
        PyBytes::new(py, self.parsed.scheme.as_bytes())
    }
    #[getter]
    fn host(&self) -> String {
        let (decoded, _) = idna::domain_to_unicode(&self.parsed.host);
        decoded
    }
    #[getter]
    fn port(&self) -> Option<u16> {
        self.parsed.port
    }
    #[getter]
    #[pyo3(name = "path")]
    fn get_path(&self) -> String {
        let p = percent_decode(&self.parsed.path);
        if p.is_empty()
            && (!self.parsed.host.is_empty()
                || self.parsed.scheme == "http"
                || self.parsed.scheme == "https")
        {
            "/".to_string()
        } else {
            p
        }
    }
    #[getter]
    fn query<'py>(&self, py: Python<'py>) -> Bound<'py, PyBytes> {
        if let Some(ref q) = self.parsed.query {
            PyBytes::new(py, q.as_bytes())
        } else {
            PyBytes::new(py, b"")
        }
    }
    #[getter]
    #[pyo3(name = "fragment")]
    fn fragment_py(&self) -> String {
        self.parsed
            .fragment
            .as_deref()
            .map(|s| percent_decode(s))
            .unwrap_or_else(String::new)
    }
    #[getter]
    fn userinfo<'py>(&self, py: Python<'py>) -> Bound<'py, PyBytes> {
        PyBytes::new(py, self.parsed.userinfo.as_bytes())
    }

    #[getter]
    pub fn raw_path<'py>(&self, py: Python<'py>) -> Bound<'py, PyBytes> {
        let rp = self.raw_path_str();
        PyBytes::new(py, rp.as_bytes())
    }

    #[getter]
    fn username(&self) -> String {
        percent_decode(self.get_username())
    }
    #[getter]
    fn password(&self) -> Option<String> {
        self.get_password().map(|s| percent_decode(s))
    }

    #[getter]
    fn netloc<'py>(&self, py: Python<'py>) -> PyResult<Bound<'py, PyBytes>> {
        let host = &self.parsed.host;
        // If it looks like an IPv6 literal (contains colon), don't IDNA encode
        // Note: parsed.host is unbracketed for IPv6
        let encoded_host = if host.contains(':') {
            format!("[{}]", host)
        } else {
            idna::domain_to_ascii(host).map_err(|e| {
                crate::exceptions::InvalidURL::new_err(format!("Invalid host: {}", e))
            })?
        };

        let mut netloc = encoded_host;
        if let Some(port) = self.parsed.port {
            netloc.push(':');
            netloc.push_str(&port.to_string());
        }
        Ok(PyBytes::new(py, netloc.as_bytes()))
    }

    #[getter]
    fn authority<'py>(&self, py: Python<'py>) -> PyResult<Bound<'py, PyBytes>> {
        let host = &self.parsed.host;
        let encoded_host = if host.contains(':') {
            format!("[{}]", host)
        } else {
            idna::domain_to_ascii(host).map_err(|e| {
                crate::exceptions::InvalidURL::new_err(format!("Invalid host: {}", e))
            })?
        };

        let mut authority = String::new();
        if !self.parsed.userinfo.is_empty() {
            authority.push_str(&self.parsed.userinfo);
            authority.push('@');
        }
        authority.push_str(&encoded_host);
        if let Some(port) = self.parsed.port {
            authority.push(':');
            authority.push_str(&port.to_string());
        }
        Ok(PyBytes::new(py, authority.as_bytes()))
    }

    #[getter]
    fn is_absolute(&self) -> bool {
        !self.parsed.scheme.is_empty()
    }
    #[getter]
    fn is_relative(&self) -> bool {
        self.parsed.scheme.is_empty()
    }

    #[getter]
    fn raw_host<'py>(&self, py: Python<'py>) -> PyResult<Bound<'py, PyBytes>> {
        let host = &self.parsed.host;
        if host.contains(':') {
            // IPv6 literal - return as bytes (unbracketed for raw_host?)
            Ok(PyBytes::new(py, host.as_bytes()))
        } else {
            let ascii = idna::domain_to_ascii(host).map_err(|e| {
                crate::exceptions::InvalidURL::new_err(format!("Invalid host: {}", e))
            })?;
            Ok(PyBytes::new(py, ascii.as_bytes()))
        }
    }

    #[getter]
    fn params(&self) -> QueryParams {
        if let Some(ref q) = self.parsed.query {
            QueryParams::from_query_string(q)
        } else {
            QueryParams { items: Vec::new() }
        }
    }

    #[pyo3(signature = (**kwargs))]
    fn copy_with(&self, kwargs: Option<&Bound<'_, PyDict>>) -> PyResult<URL> {
        let mut new_parsed = self.parsed.clone();
        if let Some(kw) = kwargs {
            for (key, value) in kw.iter() {
                let key_str: String = key.extract()?;
                match key_str.as_str() {
                    "scheme" => {
                        let s = extract_str_or_bytes(&value)?;
                        // Validate scheme chars: a-z A-Z 0-9 + . - and must start with alpha
                        if s.is_empty() || !s.chars().next().unwrap().is_ascii_alphabetic() {
                            return Err(crate::exceptions::InvalidURL::new_err(format!(
                                "Invalid URL: scheme '{}' is invalid",
                                s
                            )));
                        }
                        for c in s.chars() {
                            if !c.is_ascii_alphanumeric() && c != '+' && c != '.' && c != '-' {
                                return Err(crate::exceptions::InvalidURL::new_err(format!(
                                    "Invalid URL: scheme '{}' is invalid",
                                    s
                                )));
                            }
                        }
                        new_parsed.scheme = s;
                    }
                    "host" => {
                        let h: String = extract_str_or_bytes(&value)?;
                        let h_lower = h.to_lowercase();
                        // Strip brackets from IPv6 host if present
                        new_parsed.host = if h_lower.starts_with('[') && h_lower.ends_with(']') {
                            h_lower[1..h_lower.len() - 1].to_string()
                        } else {
                            h_lower
                        };
                        if !new_parsed.host.contains(':') && !new_parsed.host.contains('%') {
                            validate_host(&new_parsed.host)?;
                        }
                    }
                    "port" => {
                        if value.is_none() {
                            new_parsed.port = None;
                        } else {
                            let port_val: i64 = value.extract()?;
                            new_parsed.port = Some(port_val as u16);
                        }
                    }
                    "path" => {
                        let p = extract_str_or_bytes(&value)?;
                        new_parsed.path = quote(&p, &path_safe());
                    }
                    "raw_path" => {
                        let rp_str = extract_str_or_bytes(&value)?;
                        // Parse as partial/relative URL to extract path, query, fragment
                        let partial = urlparse_impl(&rp_str, None)?;
                        new_parsed.path = partial.path;
                        new_parsed.query = partial.query;
                        new_parsed.fragment = partial.fragment;
                    }
                    "query" => {
                        if value.is_none() {
                            new_parsed.query = None;
                        } else {
                            new_parsed.query = Some(extract_str_or_bytes(&value)?);
                        }
                    }
                    "fragment" => {
                        if value.is_none() {
                            new_parsed.fragment = None;
                        } else {
                            new_parsed.fragment = Some(extract_str_or_bytes(&value)?);
                        }
                    }
                    "userinfo" => {
                        // Enforce bytes for userinfo as per httpx semantic expectations
                        if !value.is_instance_of::<pyo3::types::PyBytes>() {
                            return Err(pyo3::exceptions::PyTypeError::new_err(
                                "userinfo must be bytes",
                            ));
                        }
                        new_parsed.userinfo = extract_str_or_bytes(&value)?;
                    }
                    "netloc" => {
                        let netloc_str = extract_str_or_bytes(&value)?;
                        // Parse as http://<netloc> to extract components
                        let temp_url = format!("http://{}", netloc_str);
                        let partial = urlparse_impl(&temp_url, None)?;
                        new_parsed.userinfo = partial.userinfo;
                        new_parsed.host = partial.host;
                        new_parsed.port = partial.port;
                    }
                    "username" => {
                        let user = quote(&extract_str_or_bytes(&value)?, &userinfo_safe());
                        let pass = if let Some(pos) = new_parsed.userinfo.find(':') {
                            &new_parsed.userinfo[pos + 1..]
                        } else {
                            ""
                        };
                        new_parsed.userinfo = if pass.is_empty() {
                            user
                        } else {
                            format!("{}:{}", user, pass)
                        };
                    }
                    "password" => {
                        let pass = quote(&extract_str_or_bytes(&value)?, &userinfo_safe());
                        let user = if let Some(pos) = new_parsed.userinfo.find(':') {
                            &new_parsed.userinfo[..pos]
                        } else {
                            &new_parsed.userinfo
                        };
                        new_parsed.userinfo = format!("{}:{}", user, pass);
                    }
                    _ => {
                        return Err(pyo3::exceptions::PyTypeError::new_err(format!(
                            "'{}' is an invalid keyword argument for copy_with()",
                            key_str
                        )))
                    }
                }
            }

            // Normalize port after potential scheme change
            if let Some(p) = new_parsed.port {
                if Some(p) == crate::urlparse::default_port(&new_parsed.scheme) {
                    new_parsed.port = None;
                }
            }
        }
        Ok(URL { parsed: new_parsed })
    }

    fn join(&self, url: &str) -> PyResult<URL> {
        self.join_relative(url)
    }

    fn __str__(&self) -> String {
        self.to_string()
    }
    fn __repr__(&self) -> String {
        // Mask password in repr for security
        if !self.parsed.userinfo.is_empty() && self.parsed.userinfo.contains(':') {
            let username = self.get_username();
            let mut masked = self.parsed.clone();
            masked.userinfo = format!("{}:[secure]", username);
            let masked_url = URL { parsed: masked };
            format!("URL('{}')", masked_url.to_string())
        } else {
            format!("URL('{}')", self.to_string())
        }
    }
    fn __eq__(&self, other: &Bound<'_, PyAny>) -> bool {
        if let Ok(other_url) = other.extract::<URL>() {
            self.to_string() == other_url.to_string()
        } else if let Ok(s) = other.extract::<String>() {
            let self_str = self.to_string();
            // Allow comparison with/without trailing slash
            self_str == s || self_str.trim_end_matches('/') == s.trim_end_matches('/')
        } else {
            false
        }
    }
    fn __hash__(&self) -> u64 {
        use std::hash::{Hash, Hasher};
        let mut hasher = std::collections::hash_map::DefaultHasher::new();
        self.to_string().hash(&mut hasher);
        hasher.finish()
    }

    fn copy_add_param(&self, key: &str, value: &str) -> PyResult<URL> {
        let mut qp = if let Some(ref q) = self.parsed.query {
            QueryParams::from_query_string(q)
        } else {
            QueryParams { items: Vec::new() }
        };
        qp.items.push((key.to_string(), value.to_string()));
        let mut parsed = self.parsed.clone();
        parsed.query = Some(qp.encode());
        Ok(URL { parsed })
    }

    fn copy_remove_param(&self, key: &str) -> PyResult<URL> {
        let qp = if let Some(ref q) = self.parsed.query {
            QueryParams::from_query_string(q)
        } else {
            QueryParams { items: Vec::new() }
        };
        let items: Vec<(String, String)> = qp.items.into_iter().filter(|(k, _)| k != key).collect();
        let mut parsed = self.parsed.clone();
        if items.is_empty() {
            parsed.query = None;
        } else {
            parsed.query = Some(QueryParams { items }.encode());
        }
        Ok(URL { parsed })
    }

    fn copy_set_param(&self, key: &str, value: &str) -> PyResult<URL> {
        let qp = if let Some(ref q) = self.parsed.query {
            QueryParams::from_query_string(q)
        } else {
            QueryParams { items: Vec::new() }
        };
        let mut items: Vec<(String, String)> =
            qp.items.into_iter().filter(|(k, _)| k != key).collect();
        items.push((key.to_string(), value.to_string()));
        let mut parsed = self.parsed.clone();
        parsed.query = Some(QueryParams { items }.encode());
        Ok(URL { parsed })
    }

    fn copy_merge_params(&self, params: &Bound<'_, PyAny>) -> PyResult<URL> {
        let other_qp = QueryParams::create(Some(params))?;
        let mut qp = if let Some(ref q) = self.parsed.query {
            QueryParams::from_query_string(q)
        } else {
            QueryParams { items: Vec::new() }
        };
        qp.items.extend(other_qp.items);
        let mut parsed = self.parsed.clone();
        parsed.query = Some(qp.encode());
        Ok(URL { parsed })
    }
}

/// Query parameters multidict.
#[pyclass(from_py_object)]
#[derive(Clone, Debug)]
pub struct QueryParams {
    items: Vec<(String, String)>,
}

impl QueryParams {
    pub fn from_query_string(query: &str) -> Self {
        let items: Vec<(String, String)> = query
            .split('&')
            .filter(|s| !s.is_empty())
            .map(|pair| {
                if let Some(pos) = pair.find('=') {
                    (
                        percent_decode(&pair[..pos]),
                        percent_decode(&pair[pos + 1..]),
                    )
                } else {
                    (percent_decode(pair), String::new())
                }
            })
            .collect();
        QueryParams { items }
    }

    pub fn encode(&self) -> String {
        let mut serializer = url::form_urlencoded::Serializer::new(String::new());
        for (k, v) in &self.items {
            serializer.append_pair(k, v);
        }
        serializer.finish()
    }

    fn value_to_string(v: &Bound<'_, PyAny>) -> PyResult<String> {
        if v.is_none() {
            Ok(String::new())
        } else if let Ok(b) = v.extract::<bool>() {
            Ok(if b {
                "true".to_string()
            } else {
                "false".to_string()
            })
        } else if let Ok(i) = v.extract::<i64>() {
            Ok(i.to_string())
        } else if let Ok(f) = v.extract::<f64>() {
            // Format without trailing zeros for clean display
            let s = format!("{}", f);
            Ok(s)
        } else {
            Ok(v.str()?.extract::<String>()?)
        }
    }

    pub fn items_raw(&self) -> Vec<(String, String)> {
        self.items.clone()
    }

    pub fn from_items(items: Vec<(String, String)>) -> Self {
        QueryParams { items }
    }

    pub fn create(params: Option<&Bound<'_, PyAny>>) -> PyResult<Self> {
        let mut items = Vec::new();
        if let Some(p) = params {
            if p.is_none() {
                // Nothing
            } else if let Ok(existing) = p.extract::<QueryParams>() {
                items = existing.items;
            } else if let Ok(s) = p.extract::<String>() {
                return Ok(Self::from_query_string(&s));
            } else if let Ok(b) = p.cast::<PyBytes>() {
                let s = std::str::from_utf8(b.as_bytes()).unwrap_or_default();
                return Ok(Self::from_query_string(s));
            } else if let Ok(d) = p.cast::<PyDict>() {
                for (k, v) in d.iter() {
                    let key: String = k.str()?.extract()?;
                    // Check if value is a list or tuple (expand to multiple entries)
                    if let Ok(list) = v.cast::<PyList>() {
                        for item in list.iter() {
                            items.push((key.clone(), Self::value_to_string(&item)?));
                        }
                    } else if let Ok(tuple) = v.cast::<PyTuple>() {
                        for i in 0..tuple.len() {
                            let item = tuple.get_item(i)?;
                            items.push((key.clone(), Self::value_to_string(&item)?));
                        }
                    } else {
                        items.push((key, Self::value_to_string(&v)?));
                    }
                }
            } else if let Ok(l) = p.cast::<PyList>() {
                for item in l.iter() {
                    let tuple = item.cast::<PyTuple>()?;
                    let k = tuple.get_item(0)?.str()?.extract::<String>()?;
                    let v = Self::value_to_string(&tuple.get_item(1)?)?;
                    items.push((k, v));
                }
            } else if let Ok(t) = p.cast::<PyTuple>() {
                // Handle tuple of tuples
                for i in 0..t.len() {
                    let item = t.get_item(i)?;
                    let inner = item.cast::<PyTuple>()?;
                    let k = inner.get_item(0)?.str()?.extract::<String>()?;
                    let v = Self::value_to_string(&inner.get_item(1)?)?;
                    items.push((k, v));
                }
            }
        }
        Ok(QueryParams { items })
    }
}

#[allow(dead_code)]
fn percent_encode(s: &str) -> String {
    percent_encoding::utf8_percent_encode(s, percent_encoding::NON_ALPHANUMERIC).to_string()
}

fn percent_decode(s: &str) -> String {
    percent_encoding::percent_decode_str(s)
        .decode_utf8_lossy()
        .to_string()
}

#[pymethods]
impl QueryParams {
    #[new]
    #[pyo3(signature = (params=None))]
    fn new(params: Option<&Bound<'_, PyAny>>) -> PyResult<Self> {
        Self::create(params)
    }

    fn keys(&self) -> Vec<String> {
        let mut seen = std::collections::HashSet::new();
        let mut result = Vec::new();
        for (k, _) in &self.items {
            if seen.insert(k.clone()) {
                result.push(k.clone());
            }
        }
        result
    }
    fn values(&self) -> Vec<String> {
        let mut seen = std::collections::HashSet::new();
        let mut result = Vec::new();
        for (k, v) in &self.items {
            if seen.insert(k.clone()) {
                result.push(v.clone());
            }
        }
        result
    }
    fn items(&self) -> Vec<(String, String)> {
        let mut seen = std::collections::HashSet::new();
        let mut result = Vec::new();
        for (k, v) in &self.items {
            if seen.insert(k.clone()) {
                result.push((k.clone(), v.clone()));
            }
        }
        result
    }
    fn multi_items(&self) -> Vec<(String, String)> {
        self.items.clone()
    }

    #[pyo3(signature = (key, default=None))]
    fn get(&self, key: &str, default: Option<&str>) -> Option<String> {
        self.items
            .iter()
            .find(|(k, _)| k == key)
            .map(|(_, v)| v.clone())
            .or_else(|| default.map(|s| s.to_string()))
    }

    fn get_list(&self, key: &str) -> Vec<String> {
        self.items
            .iter()
            .filter(|(k, _)| k == key)
            .map(|(_, v)| v.clone())
            .collect()
    }

    fn set(&self, key: &str, value: &str) -> Self {
        let mut items: Vec<(String, String)> = self
            .items
            .iter()
            .filter(|(k, _)| k != key)
            .cloned()
            .collect();
        items.push((key.to_string(), value.to_string()));
        QueryParams { items }
    }

    fn add(&self, key: &str, value: &str) -> Self {
        let mut items = self.items.clone();
        items.push((key.to_string(), value.to_string()));
        QueryParams { items }
    }

    fn remove(&self, key: &str) -> Self {
        let items: Vec<(String, String)> = self
            .items
            .iter()
            .filter(|(k, _)| k != key)
            .cloned()
            .collect();
        QueryParams { items }
    }

    fn merge(&self, other: &Bound<'_, PyAny>) -> PyResult<Self> {
        let other_qp = Self::create(Some(other))?;
        let mut items = Vec::new();
        let other_keys: std::collections::HashSet<String> =
            other_qp.items.iter().map(|(k, _)| k.clone()).collect();
        // Keep existing items whose keys are not in other
        for (k, v) in &self.items {
            if !other_keys.contains(k) {
                items.push((k.clone(), v.clone()));
            }
        }
        // Add all other items
        items.extend(other_qp.items);
        Ok(QueryParams { items })
    }

    fn __str__(&self) -> String {
        self.encode()
    }
    fn __repr__(&self) -> String {
        format!("QueryParams('{}')", self.encode())
    }
    fn __getitem__(&self, key: &str) -> PyResult<String> {
        self.get(key, None)
            .ok_or_else(|| pyo3::exceptions::PyKeyError::new_err(key.to_string()))
    }
    fn __setitem__(&self, _key: &str, _value: &str) -> PyResult<()> {
        Err(pyo3::exceptions::PyRuntimeError::new_err(
            "QueryParams are immutable",
        ))
    }
    fn __contains__(&self, key: &str) -> bool {
        self.items.iter().any(|(k, _)| k == key)
    }
    fn __len__(&self) -> usize {
        let mut seen = std::collections::HashSet::new();
        for (k, _) in &self.items {
            seen.insert(k);
        }
        seen.len()
    }
    fn __bool__(&self) -> bool {
        !self.items.is_empty()
    }
    fn __eq__(&self, other: &Bound<'_, PyAny>) -> bool {
        if let Ok(other_qp) = other.extract::<QueryParams>() {
            // Compare sorted for order-independent equality
            let mut a = self.items.clone();
            let mut b = other_qp.items.clone();
            a.sort();
            b.sort();
            a == b
        } else {
            false
        }
    }
    fn __hash__(&self) -> u64 {
        use std::hash::{Hash, Hasher};
        let mut hasher = std::collections::hash_map::DefaultHasher::new();
        let mut sorted = self.items.clone();
        sorted.sort();
        sorted.hash(&mut hasher);
        hasher.finish()
    }
    fn __iter__(&self, py: Python<'_>) -> PyResult<Py<PyAny>> {
        let keys = self.keys();
        let list = PyList::new(py, &keys)?;
        Ok(list.call_method0("__iter__")?.into())
    }

    fn update(&self, _params: &Bound<'_, PyAny>) -> PyResult<Self> {
        Err(pyo3::exceptions::PyRuntimeError::new_err(
            "QueryParams are immutable",
        ))
    }
}

pub fn register(m: &Bound<'_, PyModule>) -> PyResult<()> {
    m.add_class::<URL>()?;
    m.add_class::<QueryParams>()?;
    Ok(())
}
