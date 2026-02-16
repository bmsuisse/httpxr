use pyo3::prelude::*;
use pyo3::types::PyDict;
use std::collections::HashMap;

/// HTTP status codes and reason phrases.
#[pyclass]
#[derive(Clone)]
pub struct StatusCodes;

// Static map of status codes to reason phrases
pub(crate) fn status_code_map() -> HashMap<u16, &'static str> {
    let mut m = HashMap::new();
    // Informational
    m.insert(100, "Continue");
    m.insert(101, "Switching Protocols");
    m.insert(102, "Processing");
    m.insert(103, "Early Hints");
    // Success
    m.insert(200, "OK");
    m.insert(201, "Created");
    m.insert(202, "Accepted");
    m.insert(203, "Non-Authoritative Information");
    m.insert(204, "No Content");
    m.insert(205, "Reset Content");
    m.insert(206, "Partial Content");
    m.insert(207, "Multi-Status");
    m.insert(208, "Already Reported");
    m.insert(226, "IM Used");
    // Redirection
    m.insert(300, "Multiple Choices");
    m.insert(301, "Moved Permanently");
    m.insert(302, "Found");
    m.insert(303, "See Other");
    m.insert(304, "Not Modified");
    m.insert(305, "Use Proxy");
    m.insert(307, "Temporary Redirect");
    m.insert(308, "Permanent Redirect");
    // Client Error
    m.insert(400, "Bad Request");
    m.insert(401, "Unauthorized");
    m.insert(402, "Payment Required");
    m.insert(403, "Forbidden");
    m.insert(404, "Not Found");
    m.insert(405, "Method Not Allowed");
    m.insert(406, "Not Acceptable");
    m.insert(407, "Proxy Authentication Required");
    m.insert(408, "Request Timeout");
    m.insert(409, "Conflict");
    m.insert(410, "Gone");
    m.insert(411, "Length Required");
    m.insert(412, "Precondition Failed");
    m.insert(413, "Request Entity Too Large");
    m.insert(414, "Request-URI Too Long");
    m.insert(415, "Unsupported Media Type");
    m.insert(416, "Requested Range Not Satisfiable");
    m.insert(417, "Expectation Failed");
    m.insert(418, "I'm a teapot");
    m.insert(421, "Misdirected Request");
    m.insert(422, "Unprocessable Entity");
    m.insert(423, "Locked");
    m.insert(424, "Failed Dependency");
    m.insert(425, "Too Early");
    m.insert(426, "Upgrade Required");
    m.insert(428, "Precondition Required");
    m.insert(429, "Too Many Requests");
    m.insert(431, "Request Header Fields Too Large");
    m.insert(451, "Unavailable For Legal Reasons");
    // Server Error
    m.insert(500, "Internal Server Error");
    m.insert(501, "Not Implemented");
    m.insert(502, "Bad Gateway");
    m.insert(503, "Service Unavailable");
    m.insert(504, "Gateway Timeout");
    m.insert(505, "HTTP Version Not Supported");
    m.insert(506, "Variant Also Negotiates");
    m.insert(507, "Insufficient Storage");
    m.insert(508, "Loop Detected");
    m.insert(510, "Not Extended");
    m.insert(511, "Network Authentication Required");
    m
}

/// We implement the codes module using a Python-side IntEnum class,
/// because PyO3 doesn't natively support IntEnum subclassing.
/// Instead, we create it dynamically in Python.
pub fn register(m: &Bound<'_, PyModule>) -> PyResult<()> {
    let py = m.py();

    // Build the codes IntEnum class in Python to match the original API exactly
    let code = r#"
from enum import IntEnum

class codes(IntEnum):
    """HTTP status codes and reason phrases"""

    def __new__(cls, value, phrase=""):
        obj = int.__new__(cls, value)
        obj._value_ = value
        obj.phrase = phrase
        return obj

    def __str__(self):
        return str(self.value)

    @classmethod
    def get_reason_phrase(cls, value):
        try:
            return cls(value).phrase
        except ValueError:
            return ""

    @classmethod
    def is_informational(cls, value):
        return 100 <= value <= 199

    @classmethod
    def is_success(cls, value):
        return 200 <= value <= 299

    @classmethod
    def is_redirect(cls, value):
        return 300 <= value <= 399

    @classmethod
    def is_client_error(cls, value):
        return 400 <= value <= 499

    @classmethod
    def is_server_error(cls, value):
        return 500 <= value <= 599

    @classmethod
    def is_error(cls, value):
        return 400 <= value <= 599

    # informational
    CONTINUE = 100, "Continue"
    SWITCHING_PROTOCOLS = 101, "Switching Protocols"
    PROCESSING = 102, "Processing"
    EARLY_HINTS = 103, "Early Hints"

    # success
    OK = 200, "OK"
    CREATED = 201, "Created"
    ACCEPTED = 202, "Accepted"
    NON_AUTHORITATIVE_INFORMATION = 203, "Non-Authoritative Information"
    NO_CONTENT = 204, "No Content"
    RESET_CONTENT = 205, "Reset Content"
    PARTIAL_CONTENT = 206, "Partial Content"
    MULTI_STATUS = 207, "Multi-Status"
    ALREADY_REPORTED = 208, "Already Reported"
    IM_USED = 226, "IM Used"

    # redirection
    MULTIPLE_CHOICES = 300, "Multiple Choices"
    MOVED_PERMANENTLY = 301, "Moved Permanently"
    FOUND = 302, "Found"
    SEE_OTHER = 303, "See Other"
    NOT_MODIFIED = 304, "Not Modified"
    USE_PROXY = 305, "Use Proxy"
    TEMPORARY_REDIRECT = 307, "Temporary Redirect"
    PERMANENT_REDIRECT = 308, "Permanent Redirect"

    # client error
    BAD_REQUEST = 400, "Bad Request"
    UNAUTHORIZED = 401, "Unauthorized"
    PAYMENT_REQUIRED = 402, "Payment Required"
    FORBIDDEN = 403, "Forbidden"
    NOT_FOUND = 404, "Not Found"
    METHOD_NOT_ALLOWED = 405, "Method Not Allowed"
    NOT_ACCEPTABLE = 406, "Not Acceptable"
    PROXY_AUTHENTICATION_REQUIRED = 407, "Proxy Authentication Required"
    REQUEST_TIMEOUT = 408, "Request Timeout"
    CONFLICT = 409, "Conflict"
    GONE = 410, "Gone"
    LENGTH_REQUIRED = 411, "Length Required"
    PRECONDITION_FAILED = 412, "Precondition Failed"
    REQUEST_ENTITY_TOO_LARGE = 413, "Request Entity Too Large"
    REQUEST_URI_TOO_LONG = 414, "Request-URI Too Long"
    UNSUPPORTED_MEDIA_TYPE = 415, "Unsupported Media Type"
    REQUESTED_RANGE_NOT_SATISFIABLE = 416, "Requested Range Not Satisfiable"
    EXPECTATION_FAILED = 417, "Expectation Failed"
    IM_A_TEAPOT = 418, "I'm a teapot"
    MISDIRECTED_REQUEST = 421, "Misdirected Request"
    UNPROCESSABLE_ENTITY = 422, "Unprocessable Entity"
    LOCKED = 423, "Locked"
    FAILED_DEPENDENCY = 424, "Failed Dependency"
    TOO_EARLY = 425, "Too Early"
    UPGRADE_REQUIRED = 426, "Upgrade Required"
    PRECONDITION_REQUIRED = 428, "Precondition Required"
    TOO_MANY_REQUESTS = 429, "Too Many Requests"
    REQUEST_HEADER_FIELDS_TOO_LARGE = 431, "Request Header Fields Too Large"
    UNAVAILABLE_FOR_LEGAL_REASONS = 451, "Unavailable For Legal Reasons"

    # server errors
    INTERNAL_SERVER_ERROR = 500, "Internal Server Error"
    NOT_IMPLEMENTED = 501, "Not Implemented"
    BAD_GATEWAY = 502, "Bad Gateway"
    SERVICE_UNAVAILABLE = 503, "Service Unavailable"
    GATEWAY_TIMEOUT = 504, "Gateway Timeout"
    HTTP_VERSION_NOT_SUPPORTED = 505, "HTTP Version Not Supported"
    VARIANT_ALSO_NEGOTIATES = 506, "Variant Also Negotiates"
    INSUFFICIENT_STORAGE = 507, "Insufficient Storage"
    LOOP_DETECTED = 508, "Loop Detected"
    NOT_EXTENDED = 510, "Not Extended"
    NETWORK_AUTHENTICATION_REQUIRED = 511, "Network Authentication Required"

# Include lower-case styles for `requests` compatibility.
for code in codes:
    setattr(codes, code._name_.lower(), int(code))
"#;

    let c_code = std::ffi::CString::new(code).unwrap();
    let locals = PyDict::new(py);
    py.run(&c_code, None, Some(&locals))?;
    let codes_class = locals.get_item("codes")?.ok_or_else(|| {
        pyo3::exceptions::PyRuntimeError::new_err("Failed to create status codes class")
    })?;
    m.add("codes", codes_class)?;
    Ok(())
}
