//! Integration tests: the real router against a real PostgreSQL.
//!
//! `#[sqlx::test]` creates a fresh, empty database for every test, runs the
//! migrations in ./migrations and drops it afterwards, so tests never share data.
//! It needs DATABASE_URL pointing at a server where the user may create
//! databases (the docker compose Postgres works).

use axum::{
    Router,
    body::Body,
    http::{Method, Request, StatusCode},
};
use http_body_util::BodyExt;
use inventory_api::{AppState, app, events::EventPublisher};
use serde_json::{Value, json};
use sqlx::PgPool;
use tower::ServiceExt;

fn test_app(pool: PgPool) -> Router {
    app(AppState {
        pool,
        events: EventPublisher::disabled(),
    })
}

/// Sends one request to the router and returns the status and parsed JSON body.
async fn send(app: &Router, method: Method, uri: &str, body: Option<Value>) -> (StatusCode, Value) {
    let mut request = Request::builder().method(method).uri(uri);
    let body = match body {
        Some(json) => {
            request = request.header("content-type", "application/json");
            Body::from(json.to_string())
        }
        None => Body::empty(),
    };

    let response = app
        .clone()
        .oneshot(request.body(body).unwrap())
        .await
        .unwrap();
    let status = response.status();
    let bytes = response.into_body().collect().await.unwrap().to_bytes();
    let value = if bytes.is_empty() {
        Value::Null
    } else {
        serde_json::from_slice(&bytes)
            .unwrap_or_else(|_| Value::String(String::from_utf8_lossy(&bytes).into()))
    };
    (status, value)
}

async fn create_item(app: &Router, sku: &str, quantity: i32) {
    let (status, _) = send(
        app,
        Method::POST,
        "/items",
        Some(json!({ "sku": sku, "name": format!("Item {sku}"), "quantity": quantity })),
    )
    .await;
    assert_eq!(status, StatusCode::CREATED);
}

async fn move_stock(app: &Router, sku: &str, kind: &str, quantity: i32) -> (StatusCode, Value) {
    send(
        app,
        Method::POST,
        &format!("/items/{sku}/movements"),
        Some(json!({ "kind": kind, "quantity": quantity })),
    )
    .await
}

async fn quantity_of(app: &Router, sku: &str) -> i64 {
    let (status, body) = send(app, Method::GET, &format!("/items/{sku}"), None).await;
    assert_eq!(status, StatusCode::OK);
    body["quantity"].as_i64().unwrap()
}

#[sqlx::test]
async fn health_returns_ok(pool: PgPool) {
    let app = test_app(pool);
    let (status, body) = send(&app, Method::GET, "/health", None).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(body, json!({ "status": "ok" }));
}

#[sqlx::test]
async fn create_then_get_and_list_items(pool: PgPool) {
    let app = test_app(pool);

    let (status, body) = send(
        &app,
        Method::POST,
        "/items",
        Some(json!({ "sku": "  BOLT-10 ", "name": " Bolt M10 ", "quantity": 50 })),
    )
    .await;
    assert_eq!(status, StatusCode::CREATED);
    assert_eq!(
        body,
        json!({ "sku": "BOLT-10", "name": "Bolt M10", "quantity": 50 })
    );

    create_item(&app, "ANCHOR-1", 0).await;

    let (status, body) = send(&app, Method::GET, "/items/BOLT-10", None).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(body["quantity"], 50);

    let (status, body) = send(&app, Method::GET, "/items", None).await;
    assert_eq!(status, StatusCode::OK);
    let skus: Vec<&str> = body
        .as_array()
        .unwrap()
        .iter()
        .map(|i| i["sku"].as_str().unwrap())
        .collect();
    assert_eq!(skus, ["ANCHOR-1", "BOLT-10"], "items are sorted by SKU");
}

#[sqlx::test]
async fn duplicate_sku_is_a_conflict(pool: PgPool) {
    let app = test_app(pool);
    create_item(&app, "BOLT-10", 1).await;

    let (status, body) = send(
        &app,
        Method::POST,
        "/items",
        Some(json!({ "sku": "BOLT-10", "name": "Again" })),
    )
    .await;
    assert_eq!(status, StatusCode::CONFLICT);
    assert!(body["error"].as_str().unwrap().contains("already exists"));
}

#[sqlx::test]
async fn invalid_input_returns_400_with_json_error(pool: PgPool) {
    let app = test_app(pool);

    for bad in [
        json!({ "sku": "", "name": "x" }),
        json!({ "sku": "has space", "name": "x" }),
        json!({ "sku": "OK-1", "name": "x", "quantity": -1 }),
        json!({ "sku": "OK-1", "name": "x", "quantity": "ten" }),
        json!({ "name": "missing sku" }),
    ] {
        let (status, body) = send(&app, Method::POST, "/items", Some(bad.clone())).await;
        assert_eq!(status, StatusCode::BAD_REQUEST, "input: {bad}");
        assert!(body["error"].is_string(), "input: {bad} gave body {body}");
    }
}

#[sqlx::test]
async fn unknown_item_is_404(pool: PgPool) {
    let app = test_app(pool);

    let (status, body) = send(&app, Method::GET, "/items/NOPE", None).await;
    assert_eq!(status, StatusCode::NOT_FOUND);
    assert_eq!(body["error"], "Item 'NOPE' not found");

    let (status, _) = move_stock(&app, "NOPE", "receive", 1).await;
    assert_eq!(status, StatusCode::NOT_FOUND);

    let (status, _) = send(&app, Method::DELETE, "/items/NOPE", None).await;
    assert_eq!(status, StatusCode::NOT_FOUND);
}

#[sqlx::test]
async fn receive_and_issue_update_quantity_and_history(pool: PgPool) {
    let app = test_app(pool);
    create_item(&app, "NUT-5", 0).await;

    let (status, body) = move_stock(&app, "NUT-5", "receive", 100).await;
    assert_eq!(status, StatusCode::CREATED);
    assert_eq!(body["new_quantity"], 100);

    let (status, body) = move_stock(&app, "NUT-5", "issue", 30).await;
    assert_eq!(status, StatusCode::CREATED);
    assert_eq!(body["new_quantity"], 70);
    assert_eq!(quantity_of(&app, "NUT-5").await, 70);

    let (status, body) = send(&app, Method::GET, "/items/NUT-5/movements", None).await;
    assert_eq!(status, StatusCode::OK);
    let kinds: Vec<&str> = body
        .as_array()
        .unwrap()
        .iter()
        .map(|m| m["kind"].as_str().unwrap())
        .collect();
    assert_eq!(kinds, ["receive", "issue"]);
}

#[sqlx::test]
async fn stock_can_never_go_below_zero(pool: PgPool) {
    let app = test_app(pool);
    create_item(&app, "NUT-5", 10).await;

    let (status, body) = move_stock(&app, "NUT-5", "issue", 11).await;
    assert_eq!(status, StatusCode::CONFLICT);
    assert!(
        body["error"]
            .as_str()
            .unwrap()
            .starts_with("Insufficient stock")
    );

    // The rejected movement changed nothing and left no history.
    assert_eq!(quantity_of(&app, "NUT-5").await, 10);
    let (_, history) = send(&app, Method::GET, "/items/NUT-5/movements", None).await;
    assert_eq!(history, json!([]));
}

#[sqlx::test]
async fn movement_quantity_must_be_positive(pool: PgPool) {
    let app = test_app(pool);
    create_item(&app, "NUT-5", 10).await;

    for q in [0, -5] {
        let (status, _) = move_stock(&app, "NUT-5", "issue", q).await;
        assert_eq!(status, StatusCode::BAD_REQUEST);
    }
    let (status, _) = move_stock(&app, "NUT-5", "steal", 1).await;
    assert_eq!(status, StatusCode::BAD_REQUEST);
}

#[sqlx::test]
async fn concurrent_issues_never_oversell(pool: PgPool) {
    let app = test_app(pool);
    create_item(&app, "LAST-UNITS", 0).await;
    move_stock(&app, "LAST-UNITS", "receive", 50).await;

    // 20 requests try to take 5 each, but only 50 are in stock.
    let handles: Vec<_> = (0..20)
        .map(|_| {
            let app = app.clone();
            tokio::spawn(async move { move_stock(&app, "LAST-UNITS", "issue", 5).await.0 })
        })
        .collect();

    let mut succeeded = 0;
    for handle in handles {
        match handle.await.unwrap() {
            StatusCode::CREATED => succeeded += 1,
            StatusCode::CONFLICT => {}
            other => panic!("unexpected status {other}"),
        }
    }

    assert_eq!(succeeded, 10);
    assert_eq!(quantity_of(&app, "LAST-UNITS").await, 0);
}

#[sqlx::test]
async fn delete_item(pool: PgPool) {
    let app = test_app(pool);
    create_item(&app, "TEMP", 0).await;
    create_item(&app, "USED", 0).await;
    move_stock(&app, "USED", "receive", 1).await;

    let (status, _) = send(&app, Method::DELETE, "/items/TEMP", None).await;
    assert_eq!(status, StatusCode::NO_CONTENT);
    let (status, _) = send(&app, Method::GET, "/items/TEMP", None).await;
    assert_eq!(status, StatusCode::NOT_FOUND);

    // Items with history are kept for traceability.
    let (status, _) = send(&app, Method::DELETE, "/items/USED", None).await;
    assert_eq!(status, StatusCode::CONFLICT);
}
