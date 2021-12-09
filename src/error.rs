use actix_web::{http::StatusCode, HttpResponse, ResponseError};

pub type Result<T> = actix_web::error::Result<T, Error>;

#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error("Io error: {0}")]
    Io(#[from] tokio::io::Error),

    #[error("Actix error: {0}")]
    Payload(#[from] actix_web::error::PayloadError),
}
impl ResponseError for Error {
    fn status_code(&self) -> StatusCode {
        match &self {
            Error::Payload(e) => e.status_code(),
            _ => StatusCode::INTERNAL_SERVER_ERROR,
        }
    }

    fn error_response(&self) -> actix_web::HttpResponse {
        match &self {
            Error::Payload(e) => e.error_response(),
            Error::Io(e) => HttpResponse::InternalServerError().body(e.to_string()),
        }
    }
}
