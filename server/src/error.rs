use axum::{Json, http::StatusCode, response::IntoResponse};
use serde::Serialize;
use thiserror::Error;

#[derive(Debug, Error)]
pub enum DiagError {
    // 403 Forbidden - SAR denied
    #[error("DIAG-001: Authorization denied - SubjectAccessReview refused access")]
    AuthorizationDenied,

    // 404 Not Found - Instance not found
    #[error("DIAG-002: Instance not found")]
    InstanceNotFound,

    // 400 Bad Request - Invalid namespace/name
    #[error("DIAG-003: Invalid namespace or name format")]
    InvalidNameFormat,

    // 404 Not Found - Unknown kind
    #[error("DIAG-004: Unknown resource kind")]
    UnknownKind,

    // 404 Not Found - Unknown item
    #[error("DIAG-005: Unknown diagnostic item")]
    UnknownItem,

    // 401 Unauthorized - Identity absent
    #[error("DIAG-006: Authentication required - no valid identity provided")]
    AuthenticationRequired,

    // 404 Not Found - Packages endpoint disabled
    #[error("DIAG-007: Packages endpoint is disabled")]
    PackagesDisabled,

    // 500 Internal Server Error
    #[error("DIAG-500: Internal server error - {0}")]
    InternalError(String),

    // Kubernetes API errors
    #[error("Kubernetes API error: {0}")]
    KubeError(#[from] kube::Error),

    // Serialization errors
    #[error("Serialization error: {0}")]
    SerializationError(#[from] serde_json::Error),

    // YAML errors
    #[error("YAML error: {0}")]
    YamlError(#[from] serde_yaml::Error),
}

impl DiagError {
    pub const fn code(&self) -> &'static str {
        match self {
            Self::AuthorizationDenied => "DIAG-001",
            Self::InstanceNotFound => "DIAG-002",
            Self::InvalidNameFormat => "DIAG-003",
            Self::UnknownKind => "DIAG-004",
            Self::UnknownItem => "DIAG-005",
            Self::AuthenticationRequired => "DIAG-006",
            Self::PackagesDisabled => "DIAG-007",
            Self::InternalError(_)
            | Self::KubeError(_)
            | Self::SerializationError(_)
            | Self::YamlError(_) => "DIAG-500",
        }
    }

    pub const fn status_code(&self) -> StatusCode {
        match self {
            Self::AuthorizationDenied => StatusCode::FORBIDDEN,
            Self::InvalidNameFormat => StatusCode::BAD_REQUEST,
            Self::InstanceNotFound | Self::UnknownKind | Self::UnknownItem | Self::PackagesDisabled => {
                StatusCode::NOT_FOUND
            }
            Self::AuthenticationRequired => StatusCode::UNAUTHORIZED,
            Self::InternalError(_)
            | Self::KubeError(_)
            | Self::SerializationError(_)
            | Self::YamlError(_) => StatusCode::INTERNAL_SERVER_ERROR,
        }
    }
}

#[derive(Serialize)]
struct ErrorResponse {
    code: String,
    message: String,
}

impl IntoResponse for DiagError {
    fn into_response(self) -> axum::response::Response {
        let status = self.status_code();
        let response = ErrorResponse {
            code: self.code().to_string(),
            message: self.to_string(),
        };
        (status, Json(response)).into_response()
    }
}

// Helper function to create internal error
pub fn internal_error(msg: impl Into<String>) -> DiagError {
    DiagError::InternalError(msg.into())
}
