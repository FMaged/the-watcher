//! One error shape for everything under `/api`.
//!
//! Every failure leaves here as JSON (`PLAN.md`: no endpoint returns HTML) with a stable
//! `code` the frontend can branch on and a `message` a person can read — F-1 and F-2 both ask
//! for refusals that explain themselves, and "400 Bad Request" explains nothing.
//!
//! Two rules about what goes in a message. It never echoes input that failed to parse: the
//! text may have come from the agent, and an error that reflects untrusted input into a log or
//! a page is how an injection travels (A-4, N-44). And it never carries the text of a database
//! or provider error, which can name a host, a user or a connection string (N-40) — those are
//! logged and replaced with something true but general.

use axum::extract::rejection::JsonRejection;
use axum::http::StatusCode;
use axum::response::{IntoResponse, Response};
use axum::Json;
use serde_json::json;

/// `Debug` is derived rather than withheld: every field here is text already judged safe to
/// send to the caller, because the constructors below are what make it safe. A provider or
/// database error's own words never reach these fields.
#[derive(Debug)]
pub struct ApiError {
    status: StatusCode,
    code: &'static str,
    message: String,
}

impl ApiError {
    pub fn new(status: StatusCode, code: &'static str, message: impl Into<String>) -> Self {
        Self {
            status,
            code,
            message: message.into(),
        }
    }

    pub fn bad_request(code: &'static str, message: impl Into<String>) -> Self {
        Self::new(StatusCode::BAD_REQUEST, code, message)
    }

    pub fn conflict(code: &'static str, message: impl Into<String>) -> Self {
        Self::new(StatusCode::CONFLICT, code, message)
    }

    pub fn not_found(code: &'static str, message: impl Into<String>) -> Self {
        Self::new(StatusCode::NOT_FOUND, code, message)
    }

    /// A query failed. The caller is told nothing it could act on, because there is nothing.
    ///
    /// The detail goes to stderr for now. Structured logs and counters are T6.4; what matters
    /// here is that the detail is not lost and not returned.
    pub fn database(error: &dyn std::error::Error) -> Self {
        eprintln!("database error: {error}");
        Self::new(
            StatusCode::INTERNAL_SERVER_ERROR,
            "database_unavailable",
            "The database could not be reached. Nothing was changed.",
        )
    }

    /// A provider failed in a way that is not the user's fault (I-12).
    ///
    /// 502 rather than 500: the request was fine and our side is fine, and the distinction is
    /// what tells someone reading the logs whether to look at us or at the endpoint we read.
    pub fn provider(error: &dyn std::error::Error) -> Self {
        eprintln!("price source error: {error}");
        Self::new(
            StatusCode::BAD_GATEWAY,
            "price_source_unavailable",
            "The price source could not be reached, so the symbol could not be checked. \
             Nothing was changed.",
        )
    }
}

impl From<JsonRejection> for ApiError {
    /// Axum's own rejections are plain text, which would be the one non-JSON answer the API
    /// gives. The rejection's own message is not passed on: it quotes the body it could not
    /// parse (A-4).
    fn from(rejection: JsonRejection) -> Self {
        Self::bad_request(
            "malformed_body",
            match rejection {
                JsonRejection::MissingJsonContentType(_) => {
                    "The request needs a Content-Type of application/json."
                }
                _ => "The request body is not the JSON this endpoint expects.",
            },
        )
    }
}

impl IntoResponse for ApiError {
    fn into_response(self) -> Response {
        (
            self.status,
            Json(json!({ "error": { "code": self.code, "message": self.message } })),
        )
            .into_response()
    }
}

impl ApiError {
    /// For tests and for the handlers' own assertions about what they return.
    pub fn status(&self) -> StatusCode {
        self.status
    }

    /// The stable machine-readable half of the answer.
    pub fn code(&self) -> &'static str {
        self.code
    }

    pub fn message(&self) -> &str {
        &self.message
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn an_error_is_json_with_a_code_and_a_message() {
        // Everything under /api is JSON, failures included: a frontend that has to parse an
        // error page is a frontend that will show the wrong thing.
        let response =
            ApiError::conflict("watchlist_full", "A watchlist holds at most 50 tickers.")
                .into_response();

        assert_eq!(response.status(), StatusCode::CONFLICT);
        assert_eq!(
            response
                .headers()
                .get(axum::http::header::CONTENT_TYPE)
                .and_then(|v| v.to_str().ok()),
            Some("application/json")
        );

        let body = axum::body::to_bytes(response.into_body(), 4096)
            .await
            .expect("a small body");
        let parsed: serde_json::Value = serde_json::from_slice(&body).expect("valid JSON");

        assert_eq!(parsed["error"]["code"], "watchlist_full");
        assert_eq!(
            parsed["error"]["message"],
            "A watchlist holds at most 50 tickers."
        );
    }

    #[test]
    fn a_database_error_is_not_handed_to_the_caller() {
        // N-40: a sqlx error can name a host, a role or a connection string. The caller gets
        // a sentence it can act on and nothing it should not see.
        let leaky = std::io::Error::other("connection to postgres://watcher_app:hunter2@db failed");
        let e = ApiError::database(&leaky);

        assert_eq!(e.status(), StatusCode::INTERNAL_SERVER_ERROR);
        assert!(!e.message().contains("hunter2"), "{}", e.message());
        assert!(!e.message().contains("postgres://"), "{}", e.message());
    }

    #[test]
    fn a_provider_error_is_not_handed_to_the_caller_either() {
        let leaky = std::io::Error::other("GET https://provider.example/?token=sk-secret failed");
        let e = ApiError::provider(&leaky);

        assert_eq!(e.status(), StatusCode::BAD_GATEWAY);
        assert!(!e.message().contains("sk-secret"), "{}", e.message());
    }
}
