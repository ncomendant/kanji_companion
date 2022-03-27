use std::fmt;

#[derive(Debug)]
pub enum Error {
    Io(std::io::Error),
    JsValue(wasm_bindgen::JsValue),
}

impl fmt::Display for Error {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Error::Io(e) => write!(f, "{}", e),
            Error::JsValue(e) => write!(f, "{:?}", e),
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