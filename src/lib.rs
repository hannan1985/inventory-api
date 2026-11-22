//! Inventory API: items, stock movements and their HTTP routes.
//! `main.rs` only wires configuration and starts the server, so the same
//! router can be built in integration tests.

pub mod error;
pub mod models;
pub mod routes;

use sqlx::PgPool;

#[derive(Clone)]
pub struct AppState {
    pub pool: PgPool,
}

pub fn app(state: AppState) -> axum::Router {
    routes::router(state)
}
