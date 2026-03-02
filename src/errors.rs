use actix_web::HttpResponse;
use serde::Serialize;
use thiserror::Error;

#[derive(Debug, Error)]
pub enum AppError {
    #[error("Database error: {0}")]
    Database(#[from] diesel::result::Error),

    #[error("Connection pool error: {0}")]
    Pool(#[from] diesel::r2d2::PoolError),

    #[error("Not found")]
    NotFound,

    #[error("Unauthorized")]
    Unauthorized,

    #[error("Forbidden")]
    Forbidden,

    #[error("Bad request: {0}")]
    BadRequest(String),

    #[error("Internal server error")]
    Internal(#[from] anyhow::Error),
}

#[derive(Serialize)]
struct ErrorBody {
    error: String,
}

impl actix_web::ResponseError for AppError {
    fn error_response(&self) -> HttpResponse {
        let body = ErrorBody {
            error: self.to_string(),
        };
        match self {
            AppError::NotFound => HttpResponse::NotFound().json(body),
            AppError::Unauthorized => HttpResponse::Unauthorized().json(body),
            AppError::Forbidden => HttpResponse::Forbidden().json(body),
            AppError::BadRequest(_) => HttpResponse::BadRequest().json(body),
            _ => {
                tracing::error!(error = %self, "Unhandled internal error");
                HttpResponse::InternalServerError().json(body)
            }
        }
    }
}
