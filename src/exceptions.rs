use pyo3::create_exception;
use pyo3::exceptions::PyException;
use pyo3::prelude::*;

create_exception!(
    _httpxr,
    HTTPError,
    PyException,
    "Base class for RequestError and HTTPStatusError."
);

create_exception!(
    _httpxr,
    RequestError,
    HTTPError,
    "Base class for all exceptions that may occur when issuing a .request()."
);
create_exception!(
    _httpxr,
    TransportError,
    RequestError,
    "Base class for all exceptions that occur at the level of the Transport API."
);

create_exception!(
    _httpxr,
    TimeoutException,
    TransportError,
    "The base class for timeout errors."
);
create_exception!(
    _httpxr,
    ConnectTimeout,
    TimeoutException,
    "Timed out while connecting to the host."
);
create_exception!(
    _httpxr,
    ReadTimeout,
    TimeoutException,
    "Timed out while receiving data from the host."
);
create_exception!(
    _httpxr,
    WriteTimeout,
    TimeoutException,
    "Timed out while sending data to the host."
);
create_exception!(
    _httpxr,
    PoolTimeout,
    TimeoutException,
    "Timed out waiting to acquire a connection from the pool."
);

create_exception!(
    _httpxr,
    NetworkError,
    TransportError,
    "The base class for network-related errors."
);
create_exception!(
    _httpxr,
    ReadError,
    NetworkError,
    "Failed to receive data from the network."
);
create_exception!(
    _httpxr,
    WriteError,
    NetworkError,
    "Failed to send data through the network."
);
create_exception!(
    _httpxr,
    ConnectError,
    NetworkError,
    "Failed to establish a connection."
);
create_exception!(
    _httpxr,
    CloseError,
    NetworkError,
    "Failed to close a connection."
);

create_exception!(
    _httpxr,
    ProxyError,
    TransportError,
    "An error occurred while establishing a proxy connection."
);
create_exception!(
    _httpxr,
    UnsupportedProtocol,
    TransportError,
    "Attempted to make a request to an unsupported protocol."
);
create_exception!(
    _httpxr,
    ProtocolError,
    TransportError,
    "The protocol was violated."
);
create_exception!(
    _httpxr,
    LocalProtocolError,
    ProtocolError,
    "A protocol was violated by the client."
);
create_exception!(
    _httpxr,
    RemoteProtocolError,
    ProtocolError,
    "The protocol was violated by the server."
);

create_exception!(
    _httpxr,
    DecodingError,
    RequestError,
    "Decoding of the response failed, due to a malformed encoding."
);
create_exception!(
    _httpxr,
    TooManyRedirects,
    RequestError,
    "Too many redirects."
);

create_exception!(
    _httpxr,
    HTTPStatusError,
    HTTPError,
    "Response sent an error status code."
);

create_exception!(
    _httpxr,
    StreamError,
    PyException,
    "The base class for stream errors."
);
create_exception!(
    _httpxr,
    StreamConsumed,
    StreamError,
    "Attempted to read or stream content, but the content has already been streamed."
);
create_exception!(
    _httpxr,
    StreamClosed,
    StreamError,
    "Attempted to read or stream response content, but the request has been closed."
);
create_exception!(
    _httpxr,
    ResponseNotRead,
    StreamError,
    "Attempted to access streaming response content, without having called `read()`."
);
create_exception!(
    _httpxr,
    RequestNotRead,
    StreamError,
    "Attempted to access streaming request content, without having called `read()`."
);

create_exception!(
    _httpxr,
    InvalidURL,
    PyException,
    "URL was missing a hostname, or included an unsupported scheme."
);

create_exception!(
    _httpxr,
    CookieConflict,
    PyException,
    "Attempted to access a cookie by name, but multiple cookies existed."
);

pub fn register(m: &Bound<'_, PyModule>) -> PyResult<()> {
    m.add("HTTPError", m.py().get_type::<HTTPError>())?;
    m.add("RequestError", m.py().get_type::<RequestError>())?;
    m.add("TransportError", m.py().get_type::<TransportError>())?;
    m.add("TimeoutException", m.py().get_type::<TimeoutException>())?;
    m.add("ConnectTimeout", m.py().get_type::<ConnectTimeout>())?;
    m.add("ReadTimeout", m.py().get_type::<ReadTimeout>())?;
    m.add("WriteTimeout", m.py().get_type::<WriteTimeout>())?;
    m.add("PoolTimeout", m.py().get_type::<PoolTimeout>())?;
    m.add("NetworkError", m.py().get_type::<NetworkError>())?;
    m.add("ReadError", m.py().get_type::<ReadError>())?;
    m.add("WriteError", m.py().get_type::<WriteError>())?;
    m.add("ConnectError", m.py().get_type::<ConnectError>())?;
    m.add("CloseError", m.py().get_type::<CloseError>())?;
    m.add("ProxyError", m.py().get_type::<ProxyError>())?;
    m.add(
        "UnsupportedProtocol",
        m.py().get_type::<UnsupportedProtocol>(),
    )?;
    m.add("ProtocolError", m.py().get_type::<ProtocolError>())?;
    m.add(
        "LocalProtocolError",
        m.py().get_type::<LocalProtocolError>(),
    )?;
    m.add(
        "RemoteProtocolError",
        m.py().get_type::<RemoteProtocolError>(),
    )?;
    m.add("DecodingError", m.py().get_type::<DecodingError>())?;
    m.add("TooManyRedirects", m.py().get_type::<TooManyRedirects>())?;
    m.add("HTTPStatusError", m.py().get_type::<HTTPStatusError>())?;
    m.add("StreamError", m.py().get_type::<StreamError>())?;
    m.add("StreamConsumed", m.py().get_type::<StreamConsumed>())?;
    m.add("StreamClosed", m.py().get_type::<StreamClosed>())?;
    m.add("ResponseNotRead", m.py().get_type::<ResponseNotRead>())?;
    m.add("RequestNotRead", m.py().get_type::<RequestNotRead>())?;
    m.add("InvalidURL", m.py().get_type::<InvalidURL>())?;
    m.add("CookieConflict", m.py().get_type::<CookieConflict>())?;
    Ok(())
}
