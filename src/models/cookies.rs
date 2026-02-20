use pyo3::prelude::*;
use pyo3::types::{PyDict, PyList, PyTuple};

use super::response::Response;

#[pyclass(from_py_object)]
pub struct Cookies {
    pub jar: Py<PyAny>,
}

impl Clone for Cookies {
    fn clone(&self) -> Self {
        Python::attach(|py| Cookies {
            jar: self.jar.clone_ref(py),
        })
    }
}

impl Cookies {
    pub fn create(py: Python<'_>, cookies: Option<&Bound<'_, PyAny>>) -> PyResult<Self> {
        let cookiejar = py.import("http.cookiejar")?;
        let jar = cookiejar.call_method0("CookieJar")?;
        if let Some(c) = cookies {
            if !c.is_none() {
                if let Ok(existing) = c.extract::<Cookies>() {
                    return Ok(existing);
                }
                let cookie_jar_class = cookiejar.getattr("CookieJar")?;
                if c.is_instance(&cookie_jar_class)? {
                    for cookie in c.try_iter()? {
                        let cookie = cookie?;
                        jar.call_method1("set_cookie", (cookie,))?;
                    }
                } else if let Ok(d) = c.cast::<PyDict>() {
                    for (k, v) in d.iter() {
                        let name: String = k.extract()?;
                        let value: String = v.extract()?;
                        Self::set_cookie_on_jar_inner(py, &jar, &name, &value, "", "/")?;
                    }
                } else if let Ok(l) = c.cast::<PyList>() {
                    for item in l.iter() {
                        let tuple = item.cast::<PyTuple>()?;
                        let name: String = tuple.get_item(0)?.extract()?;
                        let value: String = tuple.get_item(1)?.extract()?;
                        Self::set_cookie_on_jar_inner(py, &jar, &name, &value, "", "/")?;
                    }
                }
            }
        }
        Ok(Cookies { jar: jar.into() })
    }

    pub(crate) fn set_cookie_on_jar_inner(
        py: Python<'_>,
        jar: &Bound<'_, PyAny>,
        name: &str,
        value: &str,
        domain: &str,
        path: &str,
    ) -> PyResult<()> {
        let cookiejar = py.import("http.cookiejar")?;
        let kwargs = PyDict::new(py);
        kwargs.set_item("version", 0i32)?;
        kwargs.set_item("name", name)?;
        kwargs.set_item("value", value)?;
        kwargs.set_item("port", py.None())?;
        kwargs.set_item("port_specified", false)?;
        kwargs.set_item("domain", domain)?;
        kwargs.set_item("domain_specified", false)?;
        kwargs.set_item("domain_initial_dot", domain.starts_with('.'))?;
        kwargs.set_item("path", path)?;
        kwargs.set_item("path_specified", path != "/")?;
        kwargs.set_item("secure", false)?;
        kwargs.set_item("expires", py.None())?;
        kwargs.set_item("discard", false)?;
        kwargs.set_item("comment", py.None())?;
        kwargs.set_item("comment_url", py.None())?;
        kwargs.set_item("rest", PyDict::new(py))?;
        let cookie = cookiejar.getattr("Cookie")?.call((), Some(&kwargs))?;
        jar.call_method1("set_cookie", (cookie,))?;
        Ok(())
    }
}

#[pymethods]
impl Cookies {
    #[new]
    #[pyo3(signature = (cookies=None))]
    fn new(py: Python<'_>, cookies: Option<&Bound<'_, PyAny>>) -> PyResult<Self> {
        Self::create(py, cookies)
    }

    #[pyo3(signature = (name, value, domain=None, path=None))]
    fn set(
        &self,
        py: Python<'_>,
        name: &str,
        value: &str,
        domain: Option<&str>,
        path: Option<&str>,
    ) -> PyResult<()> {
        Self::set_cookie_on_jar_inner(
            py,
            self.jar.bind(py),
            name,
            value,
            domain.unwrap_or(""),
            path.unwrap_or("/"),
        )
    }

    fn delete(
        &self,
        py: Python<'_>,
        name: &str,
        domain: Option<&str>,
        path: Option<&str>,
    ) -> PyResult<()> {
        let jar_bound = self.jar.bind(py);
        let mut to_remove = Vec::new();
        for cookie in jar_bound.try_iter()? {
            let cookie = cookie?;
            let cookie_name: String = cookie.getattr("name")?.extract()?;
            if cookie_name != name {
                continue;
            }
            if let Some(d) = domain {
                let cookie_domain: String = cookie.getattr("domain")?.extract()?;
                if cookie_domain != d {
                    continue;
                }
            }
            if let Some(p) = path {
                let cookie_path: String = cookie.getattr("path")?.extract()?;
                if cookie_path != p {
                    continue;
                }
            }
            to_remove.push(cookie.clone().unbind());
        }
        for c in to_remove {
            jar_bound.call_method1(
                "clear",
                (
                    c.bind(py).getattr("domain")?,
                    c.bind(py).getattr("path")?,
                    c.bind(py).getattr("name")?,
                ),
            )?;
        }
        Ok(())
    }

    #[pyo3(signature = (domain=None, path=None))]
    fn clear(&self, py: Python<'_>, domain: Option<&str>, path: Option<&str>) -> PyResult<()> {
        let jar_bound = self.jar.bind(py);
        if domain.is_none() && path.is_none() {
            jar_bound.call_method0("clear")?;
        } else {
            let mut to_remove = Vec::new();
            for cookie in jar_bound.try_iter()? {
                let cookie = cookie?;
                let mut matches = true;
                if let Some(d) = domain {
                    let cookie_domain: String = cookie.getattr("domain")?.extract()?;
                    if cookie_domain != d {
                        matches = false;
                    }
                }
                if let Some(p) = path {
                    let cookie_path: String = cookie.getattr("path")?.extract()?;
                    if cookie_path != p {
                        matches = false;
                    }
                }
                if matches {
                    to_remove.push(cookie.clone().unbind());
                }
            }
            for c in to_remove {
                jar_bound.call_method1(
                    "clear",
                    (
                        c.bind(py).getattr("domain")?,
                        c.bind(py).getattr("path")?,
                        c.bind(py).getattr("name")?,
                    ),
                )?;
            }
        }
        Ok(())
    }

    fn extract_cookies(&self, py: Python<'_>, response: &mut Response) -> PyResult<()> {
        let headers_py = response.headers(py).unwrap(); let headers = headers_py.bind(py).borrow();
        for (k, v) in headers.get_multi_items() {
            if k == "set-cookie" {
                if let Some(pos) = v.find('=') {
                    let name = &v[..pos];
                    let rest = &v[pos + 1..];
                    let value = rest.split(';').next().unwrap_or(rest);
                    Self::set_cookie_on_jar_inner(py, self.jar.bind(py), name, value, "", "/")?;
                }
            }
        }
        Ok(())
    }

    fn update(&self, py: Python<'_>, other: Option<&Bound<'_, PyAny>>) -> PyResult<()> {
        if let Some(o) = other {
            if let Ok(cookies) = o.extract::<Cookies>() {
                for cookie in cookies.jar.bind(py).try_iter()? {
                    let cookie = cookie?;
                    self.jar.bind(py).call_method1("set_cookie", (cookie,))?;
                }
            } else if let Ok(d) = o.cast::<PyDict>() {
                for (k, v) in d.iter() {
                    let name: String = k.extract()?;
                    let value: String = v.extract()?;
                    Self::set_cookie_on_jar_inner(py, self.jar.bind(py), &name, &value, "", "/")?;
                }
            }
        }
        Ok(())
    }

    fn keys(&self, py: Python<'_>) -> PyResult<Vec<String>> {
        let mut keys = Vec::new();
        for cookie in self.jar.bind(py).try_iter()? {
            let cookie = cookie?;
            keys.push(cookie.getattr("name")?.extract()?);
        }
        Ok(keys)
    }

    fn values(&self, py: Python<'_>) -> PyResult<Vec<String>> {
        let mut values = Vec::new();
        for cookie in self.jar.bind(py).try_iter()? {
            let cookie = cookie?;
            values.push(cookie.getattr("value")?.extract()?);
        }
        Ok(values)
    }

    #[pyo3(signature = (name, default=None, domain=None, path=None))]
    fn get(
        &self,
        py: Python<'_>,
        name: &str,
        default: Option<&str>,
        domain: Option<&str>,
        path: Option<&str>,
    ) -> PyResult<Option<String>> {
        let jar_bound = self.jar.bind(py);
        for cookie in jar_bound.try_iter()? {
            let cookie: Bound<'_, PyAny> = cookie?;
            let cookie_name: String = cookie.getattr("name")?.extract()?;
            if cookie_name == name {
                if let Some(d) = domain {
                    let cookie_domain: String = cookie.getattr("domain")?.extract()?;
                    if cookie_domain != d {
                        continue;
                    }
                }
                if let Some(p) = path {
                    let cookie_path: String = cookie.getattr("path")?.extract()?;
                    if cookie_path != p {
                        continue;
                    }
                }
                return Ok(Some(cookie.getattr("value")?.extract()?));
            }
        }
        Ok(default.map(|s| s.to_string()))
    }

    #[getter]
    fn jar(&self, py: Python<'_>) -> Py<PyAny> {
        self.jar.clone_ref(py)
    }

    fn __len__(&self, py: Python<'_>) -> PyResult<usize> {
        let mut count = 0;
        for _ in self.jar.bind(py).try_iter()? {
            count += 1;
        }
        Ok(count)
    }

    fn items(&self, py: Python<'_>) -> PyResult<Vec<(String, String)>> {
        let mut items = Vec::new();
        for cookie in self.jar.bind(py).try_iter()? {
            let cookie = cookie?;
            let name: String = cookie.getattr("name")?.extract()?;
            let value: String = cookie.getattr("value")?.extract()?;
            items.push((name, value));
        }
        Ok(items)
    }

    fn __repr__(&self, py: Python<'_>) -> PyResult<String> {
        let mut parts = Vec::new();
        for cookie in self.jar.bind(py).try_iter()? {
            let cookie = cookie?;
            let name: String = cookie.getattr("name")?.extract()?;
            let value: String = cookie.getattr("value")?.extract()?;
            let domain: String = cookie.getattr("domain")?.extract()?;
            let path: String = cookie.getattr("path")?.extract()?;
            if domain.is_empty() {
                parts.push(format!("<Cookie {}={} for {}>", name, value, path));
            } else {
                parts.push(format!(
                    "<Cookie {}={} for {} {}>",
                    name, value, domain, path
                ));
            }
        }
        Ok(format!("<Cookies[{}]>", parts.join(", ")))
    }

    fn __iter__(&self, py: Python<'_>) -> PyResult<Py<PyAny>> {
        let keys = self.keys(py)?;
        let list = PyList::new(py, &keys)?;
        Ok(list.call_method0("__iter__")?.into())
    }

    fn __getitem__(&self, py: Python<'_>, key: &str) -> PyResult<String> {
        let jar_bound = self.jar.bind(py);
        let mut values = Vec::new();
        for cookie in jar_bound.try_iter()? {
            let cookie = cookie?;
            let cookie_name: String = cookie.getattr("name")?.extract()?;
            if cookie_name == key {
                values.push(cookie.getattr("value")?.extract::<String>()?);
            }
        }
        if values.len() > 1 {
            return Err(crate::exceptions::CookieConflict::new_err(
                format!("Attempted to access the cookie '{}', but multiple cookies with that name exist. Use 'cookies.get(\"{}\")', passing the appropriate 'domain' or 'path' arguments.", key, key)
            ));
        }
        values
            .into_iter()
            .next()
            .ok_or_else(|| pyo3::exceptions::PyKeyError::new_err(key.to_string()))
    }

    fn __setitem__(&self, py: Python<'_>, key: &str, value: &str) -> PyResult<()> {
        Self::set_cookie_on_jar_inner(py, self.jar.bind(py), key, value, "", "/")
    }

    fn __delitem__(&self, py: Python<'_>, key: &str) -> PyResult<()> {
        self.delete(py, key, None, None)
    }

    fn __contains__(&self, py: Python<'_>, key: &str) -> PyResult<bool> {
        let jar_bound = self.jar.bind(py);
        for cookie in jar_bound.try_iter()? {
            let cookie = cookie?;
            let cookie_name: String = cookie.getattr("name")?.extract()?;
            if cookie_name == key {
                return Ok(true);
            }
        }
        Ok(false)
    }

    fn __bool__(&self, py: Python<'_>) -> PyResult<bool> {
        Ok(self.__len__(py)? > 0)
    }
}
