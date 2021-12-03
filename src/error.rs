use actix_web::{http::StatusCode, HttpResponse, ResponseError};

pub type Result<T> = actix_web::error::Result<T, Error>;

#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error("{0}")]
    ImageManager(#[from] crate::image_manager::Error),

    #[error("{0}")]
    Payload(#[from] actix_web::error::PayloadError),
    // #[error("unknown error")]
    // Unknown,
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
            Error::ImageManager(e) => HttpResponse::InternalServerError().body(e.to_string()),
            // Error::Unknown => HttpResponse::InternalServerError().finish(),
        }
    }
}
