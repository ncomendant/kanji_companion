use std::fmt;

#[derive(Debug)]
pub enum Error {
    Io(std::io::Error),
    JsValue(wasm_bindgen::JsValue),
    WindowNotFound,
    DocumentNotFound,
    ElementNotFound,
}

impl fmt::Display for Error {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Error::Io(e) => write!(f, "{}", e),
            Error::JsValue(e) => write!(f, "{:?}", e),
            Error::WindowNotFound => write!(f, "window not found"),
            Error::DocumentNotFound => write!(f, "document not found"),
            Error::ElementNotFound => write!(f, "element not found"),
        }
    }
}

impl std::error::Error for Error {}

impl From<std::io::Error> for Error {
    fn from(e: std::io::Error) -> Self {
        Error::Io(e)
    }
}

impl From<wasm_bindgen::JsValue> for Error {
    fn from(e: wasm_bindgen::JsValue) -> Self {
        Error::JsValue(e)
    }
}