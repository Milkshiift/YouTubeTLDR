#[derive(Debug)]
#[allow(dead_code)]
pub enum GeminiResponseError {
    MinreqError(minreq::Error),
    ///Contains the response string
    StatusNotOk(String),
}

impl std::fmt::Display for GeminiResponseError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{:?}", self)
    }
}

impl std::error::Error for GeminiResponseError {}