//! The axum router, JSON handlers, MCP transport and the scheduler.
//!
//! Must not depend on: nothing. Nothing in the workspace may depend on this crate.
//!
//! sqlx is deliberately absent from this crate's `Cargo.toml`. The pool arrives as
//! `watcher_store::Db` and every query lives behind a function in that crate, which is what
//! makes "the store talks to Postgres only" readable from the manifest instead of inferred
//! from handlers.

pub mod error;
pub mod watchlist;

use std::net::SocketAddr;
use std::sync::Arc;

use axum::routing::{delete, get};
use axum::Router;
use watcher_core::PriceSource;
use watcher_store::Db;

/// What every handler is given.
///
/// `user_id` is state rather than something derived from the request because there is no login
/// in v1 (`PLAN.md` D14) and there is one seeded user. Carrying it explicitly is what keeps
/// N-43 honest: the day a request does carry an identity, it replaces this field, and every
/// query already takes the parameter.
pub struct AppState<P> {
    pub db: Db,
    pub prices: Arc<P>,
    pub user_id: &'static str,
}

/// Written by hand because the derive would demand `P: Clone`, and a price source holding an
/// HTTP client is shared rather than cloned.
impl<P> Clone for AppState<P> {
    fn clone(&self) -> Self {
        Self {
            db: self.db.clone(),
            prices: Arc::clone(&self.prices),
            user_id: self.user_id,
        }
    }
}

/// Every route the API answers today.
pub fn router<P>(state: AppState<P>) -> Router
where
    P: PriceSource + Send + Sync + 'static,
{
    Router::new()
        .route(
            "/api/watchlist",
            get(watchlist::list::<P>).post(watchlist::add::<P>),
        )
        .route("/api/watchlist/{ticker}", delete(watchlist::remove::<P>))
        // Axum answers an unmatched path or a wrong method with an empty body. That is not
        // HTML, so it keeps the letter of "everything under /api is JSON" and breaks a client
        // that reads `error.code` on every failure — which is the one thing the frontend is
        // built to do. Both become the same envelope as every other refusal.
        .fallback(no_such_endpoint)
        .method_not_allowed_fallback(wrong_method)
        .with_state(state)
}

async fn no_such_endpoint() -> error::ApiError {
    error::ApiError::not_found("no_such_endpoint", "There is no endpoint at that path.")
}

async fn wrong_method() -> error::ApiError {
    error::ApiError::new(
        axum::http::StatusCode::METHOD_NOT_ALLOWED,
        "wrong_method",
        "That endpoint exists but does not take this method.",
    )
}

/// Serve until the process is asked to stop.
pub async fn serve(addr: SocketAddr, router: Router) -> std::io::Result<()> {
    let listener = tokio::net::TcpListener::bind(addr).await?;
    axum::serve(listener, router)
        .with_graceful_shutdown(shutdown())
        .await
}

/// Finish what is in flight, then stop.
///
/// SIGTERM is what `docker stop` sends and what the Dockerfile's exec-form entrypoint exists
/// to deliver. Without this, a deploy cuts whatever request was mid-flight — which for a
/// watchlist write means the client never learns whether it happened.
async fn shutdown() {
    let interrupt = tokio::signal::ctrl_c();

    let mut terminate =
        match tokio::signal::unix::signal(tokio::signal::unix::SignalKind::terminate()) {
            Ok(signal) => signal,
            // Registering a handler can only fail at the OS level. Interrupt still works, and
            // refusing to serve over it would be the wrong trade.
            Err(e) => {
                eprintln!("could not listen for SIGTERM, falling back to interrupt only: {e}");
                let _ = interrupt.await;
                return;
            }
        };

    tokio::select! {
        _ = terminate.recv() => {}
        _ = interrupt => {}
    }
}
