use pyo3::prelude::*;
use pyo3::types::{PyBytes, PyDict};

use crate::exceptions::InvalidURL;
use crate::query_params::{percent_decode, QueryParams};
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

    /// Fast URL constructor for URLs already validated by reqwest.
    /// Skips urlparse_impl (url::Url::parse, IDNA, control char checks).
    /// Only use for URLs known to be valid http/https URLs.
    pub fn create_from_str_fast(url_str: &str) -> Self {
        let (scheme, rest) = if let Some(pos) = url_str.find("://") {
            (&url_str[..pos], &url_str[pos + 3..])
        } else {
            return URL {
                parsed: ParseResult {
                    scheme: String::new(),
                    userinfo: String::new(),
                    host: String::new(),
                    port: None,
                    path: url_str.to_string(),
                    query: None,
                    fragment: None,
                },
            };
        };

        let (authority, path_query_frag) = if let Some(slash_pos) = rest.find('/') {
            (&rest[..slash_pos], &rest[slash_pos..])
        } else if let Some(q_pos) = rest.find('?') {
            (&rest[..q_pos], &rest[q_pos..])
        } else {
            (rest, "")
        };

        let (userinfo, host_port) = if let Some(at_pos) = authority.find('@') {
            (authority[..at_pos].to_string(), &authority[at_pos + 1..])
        } else {
            (String::new(), authority)
        };

        let (host, port) = if host_port.starts_with('[') {
            if let Some(bracket_end) = host_port.find(']') {
                let h = &host_port[1..bracket_end];
                let port_part = &host_port[bracket_end + 1..];
                let p = if port_part.starts_with(':') {
                    port_part[1..].parse::<u16>().ok()
                } else {
                    None
                };
                (h.to_string(), p)
            } else {
                (host_port.to_string(), None)
            }
        } else if let Some(colon_pos) = host_port.rfind(':') {
            if let Ok(p) = host_port[colon_pos + 1..].parse::<u16>() {
                (host_port[..colon_pos].to_string(), Some(p))
            } else {
                (host_port.to_string(), None)
            }
        } else {
            (host_port.to_string(), None)
        };

        let (path_str, fragment) = if let Some(h_pos) = path_query_frag.find('#') {
            (&path_query_frag[..h_pos], Some(path_query_frag[h_pos + 1..].to_string()))
        } else {
            (path_query_frag, None)
        };
        let (path, query) = if let Some(q_pos) = path_str.find('?') {
            (path_str[..q_pos].to_string(), Some(path_str[q_pos + 1..].to_string()))
        } else {
            (path_str.to_string(), None)
        };

        let normalized_port = match (scheme, port) {
            ("http", Some(80)) | ("https", Some(443)) => None,
            _ => port,
        };

        URL {
            parsed: ParseResult {
                scheme: scheme.to_string(),
                userinfo,
                host,
                port: normalized_port,
                path,
                query,
                fragment,
            },
        }
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
            let s = final_url_obj.as_str();
            let path_start = s.find("http://dummy").map(|i| i + 12).unwrap_or(0);
            &s[path_start..]
        } else {
            final_url_obj.as_str()
        };

        let mut parsed_url = urlparse_impl(final_url, None)?;

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

        if let Some(p) = path {
            check_length(p, 65536, Some("path"))?;
            check_control_chars(p, 0, Some("path"))?;
        }
        if let Some(h) = host {
            check_length(h, 65536, Some("host"))?;
        }

        if let Some(s) = scheme {
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

        if let Some(s) = scheme {
            parsed.scheme = s.to_lowercase();
        }
        if !parsed.host.contains(':') && !parsed.host.contains('%') {
            validate_host(&parsed.host)?;
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
                        if !value.is_instance_of::<pyo3::types::PyBytes>() {
                            return Err(pyo3::exceptions::PyTypeError::new_err(
                                "userinfo must be bytes",
                            ));
                        }
                        new_parsed.userinfo = extract_str_or_bytes(&value)?;
                    }
                    "netloc" => {
                        let netloc_str = extract_str_or_bytes(&value)?;
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

pub fn register(m: &Bound<'_, PyModule>) -> PyResult<()> {
    m.add_class::<URL>()?;
    Ok(())
}
