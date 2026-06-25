// SPDX-License-Identifier: MIT OR Apache-2.0
//! `pheno-sidecar-stub` — a local HTTP server that emulates the pheno-forge
//! sidecars (supermemory / letta / cognee / mem0) just well enough for
//! `pheno-forge-smoke --mode=sidecar` to exercise the full routing surface
//! end-to-end.
//!
//! What each sidecar endpoint accepts:
//!
//! ## supermemory (:3030) — pheno-supermemory
//! - `GET  /health`                  -> `{"ok": true, "store_size": <n>}`
//! - `POST /v1/memories`             -> `{"id": "<uuid>"}` (echoes key as `id`)
//! - `GET  /v1/memories?query=...`   -> `[{ id, key, value, score }]`
//! - `DELETE /v1/memories/<key>`     -> 204
//!
//! ## letta (:8283) — pheno-letta
//! - `GET  /health`                 -> `{"ok": true, "agents": <n>}`
//! - `POST /v1/agents/<id>/archival-memory` -> `{"id": "<uuid>"}`
//! - `GET  /v1/agents/<id>/archival-memory?query=...` -> `[{...}]`
//! - `DELETE /v1/agents/<id>/archival-memory/<key>` -> 204
//!
//! ## cognee (stdio MCP / HTTP-fallback :9842) — pheno-cognee
//! - `GET  /health`                 -> `{"ok": true, "datasets": <n>}`
//! - `POST /memory/add`             -> `{"id": "<uuid>"}`
//! - `POST /memory/search`          -> `[{ id, key, value, score }]`
//! - `DELETE /memory/forget`        -> 204
//!
//! ## mem0 (:8000) — pheno-mem0
//! - `GET  /health`                 -> `{"ok": true, "memories": <n>}`
//! - `POST /v1/memories/`           -> `{"id": "<uuid>"}`
//! - `GET  /v1/memories/?query=...` -> `[{...}]`
//! - `DELETE /v1/memories/<id>/`    -> 204
//!
//! ## graphiti (:8001) — ADR-098 alternative
//! ## hippo (:8002) — ADR-098 alternative
//! ## zep (:8003) — ADR-098 alternative
//! All expose the supermemory-like surface (the alternative adapters
//! follow the same `store / recall / forget` REST shape).

use std::collections::HashMap;
use std::sync::Arc;

use axum::extract::{Path, Query, State};
use axum::routing::{delete, get, post};
use axum::{Json, Router};
use clap::Parser;
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use tokio::sync::Mutex;
use uuid::Uuid;

#[derive(Parser, Debug)]
#[command(name = "pheno-sidecar-stub", version, about)]
struct Cli {
    /// Which sidecar to emulate: supermemory, letta, cognee, mem0,
    /// graphiti, hippo, zep
    #[arg(long)]
    sidecar: String,
    /// Port to bind
    #[arg(long, default_value_t = 3030)]
    port: u16,
}

#[derive(Clone, Default)]
struct AppState {
    store: Arc<Mutex<HashMap<String, Value>>>,
}

#[derive(Debug, Serialize)]
struct Health {
    ok: bool,
    store_size: usize,
}

#[derive(Debug, Deserialize)]
struct StoreBody {
    #[serde(default)]
    key: Option<String>,
    #[serde(default)]
    value: Option<Value>,
    #[serde(default)]
    container_tag: Option<String>,
    #[serde(default)]
    agent_id: Option<String>,
    #[serde(default)]
    data: Option<Value>,
    #[serde(default)]
    messages: Option<Vec<Value>>,
    #[serde(default)]
    text: Option<String>,
}

#[derive(Debug, Deserialize)]
struct RecallQuery {
    #[serde(default)]
    query: Option<String>,
    #[serde(default)]
    text: Option<String>,
    #[serde(default)]
    limit: Option<usize>,
}

async fn health(State(state): State<AppState>) -> Json<Health> {
    Json(Health {
        ok: true,
        store_size: state.store.lock().await.len(),
    })
}

async fn store(
    State(state): State<AppState>,
    Path(_path): Path<String>,
    Json(body): Json<StoreBody>,
) -> Json<Value> {
    let key = body
        .key
        .or_else(|| body.agent_id.clone())
        .unwrap_or_else(|| Uuid::new_v4().to_string());
    let value = body
        .value
        .or(body.data)
        .or_else(|| body.text.clone().map(Value::String))
        .unwrap_or(Value::Null);
    state
        .store
        .lock()
        .await
        .insert(key.clone(), value.clone());
    Json(json!({ "id": key }))
}

async fn recall(
    State(state): State<AppState>,
    Path(_path): Path<String>,
    Query(q): Query<RecallQuery>,
) -> Json<Vec<Value>> {
    let _ = q;
    let store = state.store.lock().await;
    let recs: Vec<Value> = store
        .iter()
        .map(|(k, v)| json!({ "id": k, "key": k, "value": v.to_string(), "score": 1.0 }))
        .collect();
    Json(recs)
}

async fn forget(
    State(state): State<AppState>,
    Path(path): Path<String>,
) -> Json<Value> {
    let key = path.trim_start_matches('/');
    state.store.lock().await.remove(key);
    Json(json!({ "ok": true }))
}

async fn post_search(
    State(state): State<AppState>,
    Json(body): Json<StoreBody>,
) -> Json<Vec<Value>> {
    let _ = body;
    let store = state.store.lock().await;
    let recs: Vec<Value> = store
        .iter()
        .map(|(k, v)| json!({ "id": k, "key": k, "value": v.to_string(), "score": 1.0 }))
        .collect();
    Json(recs)
}

async fn post_forget(
    State(state): State<AppState>,
    Json(body): Json<StoreBody>,
) -> Json<Value> {
    let _ = body;
    state.store.lock().await.clear();
    Json(json!({ "ok": true }))
}

// supermemory-specific shape: { container_tag, key, content } in,
// { id } out
#[derive(Debug, Deserialize)]
struct SmStoreBody {
    #[serde(default)]
    container_tag: Option<String>,
    #[serde(default)]
    key: Option<String>,
    #[serde(default)]
    content: Option<String>,
}

#[derive(Debug, Serialize)]
struct SmStoreResp {
    id: String,
}

async fn sm_store(
    State(state): State<AppState>,
    Json(body): Json<SmStoreBody>,
) -> Json<SmStoreResp> {
    let key = body
        .key
        .unwrap_or_else(|| Uuid::new_v4().to_string());
    let value = body
        .content
        .map(Value::String)
        .unwrap_or(Value::Null);
    state
        .store
        .lock()
        .await
        .insert(key.clone(), value);
    Json(SmStoreResp { id: key })
}

#[derive(Debug, Deserialize)]
struct SmSearchBody {
    #[serde(default)]
    q: Option<String>,
    #[serde(default)]
    container_tag: Option<String>,
    #[serde(default)]
    limit: Option<usize>,
}

#[derive(Debug, Serialize)]
struct SmHit {
    id: String,
    content: String,
    score: f32,
}

#[derive(Debug, Serialize)]
struct SmSearchResp {
    results: Vec<SmHit>,
}

async fn sm_search(
    State(state): State<AppState>,
    Json(body): Json<SmSearchBody>,
) -> Json<SmSearchResp> {
    let q = body.q.unwrap_or_default().to_lowercase();
    let store = state.store.lock().await;
    let mut hits: Vec<SmHit> = store
        .iter()
        .filter(|(_, v)| {
            q.is_empty() || v.to_string().to_lowercase().contains(&q)
        })
        .map(|(k, v)| SmHit {
            id: k.clone(),
            content: v.to_string(),
            score: 1.0,
        })
        .collect();
    if let Some(limit) = body.limit {
        hits.truncate(limit);
    }
    Json(SmSearchResp { results: hits })
}

async fn sm_forget(
    State(state): State<AppState>,
    Path((scope, key)): Path<(String, String)>,
) -> Json<Value> {
    let _ = scope;
    state.store.lock().await.remove(&key);
    Json(json!({ "ok": true }))
}

// letta-specific shape: { agent_id, content } in, { id } out
#[derive(Debug, Deserialize)]
struct LettaStoreBody {
    #[serde(default)]
    agent_id: Option<String>,
    #[serde(default)]
    content: Option<String>,
}

#[derive(Debug, Serialize)]
struct LettaStoreResp {
    id: String,
}

async fn letta_store(
    State(state): State<AppState>,
    Path(_id): Path<String>,
    Json(body): Json<LettaStoreBody>,
) -> Json<LettaStoreResp> {
    let key = body.agent_id.unwrap_or_else(|| Uuid::new_v4().to_string());
    let value = body.content.map(Value::String).unwrap_or(Value::Null);
    state.store.lock().await.insert(key.clone(), value);
    Json(LettaStoreResp { id: key })
}

#[derive(Debug, Deserialize)]
struct LettaSearchBody {
    #[serde(default)]
    query: Option<String>,
    #[serde(default)]
    limit: Option<usize>,
}

#[derive(Debug, Serialize)]
struct LettaSearchResp {
    passages: Vec<Value>,
}

async fn letta_search(
    State(state): State<AppState>,
    Json(body): Json<LettaSearchBody>,
) -> Json<LettaSearchResp> {
    let q = body.query.unwrap_or_default().to_lowercase();
    let store = state.store.lock().await;
    let mut hits: Vec<Value> = store
        .iter()
        .filter(|(_, v)| q.is_empty() || v.to_string().to_lowercase().contains(&q))
        .map(|(k, v)| json!({ "id": k, "text": v.to_string(), "score": 1.0 }))
        .collect();
    if let Some(limit) = body.limit {
        hits.truncate(limit);
    }
    Json(LettaSearchResp { passages: hits })
}

// mem0-specific shape: { user_id, messages: [{role, content}] } in, { id } out
#[derive(Debug, Deserialize)]
struct Mem0StoreBody {
    #[serde(default)]
    user_id: Option<String>,
    #[serde(default)]
    messages: Option<Vec<Mem0Msg>>,
}

#[derive(Debug, Deserialize)]
struct Mem0Msg {
    #[serde(default)]
    role: Option<String>,
    #[serde(default)]
    content: Option<String>,
}

async fn letta_forget(
    State(state): State<AppState>,
    Path((_id, key)): Path<(String, String)>,
) -> Json<Value> {
    state.store.lock().await.remove(&key);
    Json(json!({ "ok": true }))
}

async fn mem0_store(
    State(state): State<AppState>,
    Json(body): Json<Mem0StoreBody>,
) -> Json<Value> {
    let key = body
        .user_id
        .clone()
        .or_else(|| {
            body.messages
                .as_ref()
                .and_then(|m| m.first().and_then(|m| m.content.clone()))
        })
        .unwrap_or_else(|| Uuid::new_v4().to_string());
    let value = body
        .messages
        .and_then(|ms| ms.into_iter().next())
        .and_then(|m| m.content)
        .map(Value::String)
        .unwrap_or(Value::Null);
    state.store.lock().await.insert(key.clone(), value);
    Json(json!({ "id": key }))
}

#[derive(Debug, Deserialize)]
struct Mem0SearchBody {
    #[serde(default)]
    user_id: Option<String>,
    #[serde(default)]
    query: Option<String>,
    #[serde(default)]
    limit: Option<usize>,
}

#[derive(Debug, Serialize)]
struct Mem0Hit {
    id: String,
    memory: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    score: Option<f32>,
}

#[derive(Debug, Serialize)]
struct Mem0SearchResp {
    results: Vec<Mem0Hit>,
}

async fn mem0_search(
    State(state): State<AppState>,
    Json(body): Json<Mem0SearchBody>,
) -> Json<Mem0SearchResp> {
    let q = body.query.unwrap_or_default().to_lowercase();
    let store = state.store.lock().await;
    let mut hits: Vec<Mem0Hit> = store
        .iter()
        .filter(|(_, v)| q.is_empty() || v.to_string().to_lowercase().contains(&q))
        .map(|(k, v)| Mem0Hit {
            id: k.clone(),
            memory: v.to_string(),
            score: Some(1.0),
        })
        .collect();
    if let Some(limit) = body.limit {
        hits.truncate(limit);
    }
    Json(Mem0SearchResp { results: hits })
}

async fn mem0_forget(
    State(state): State<AppState>,
    Path(path): Path<String>,
) -> Json<Value> {
    let key = path.trim_end_matches('/').trim_start_matches('/');
    state.store.lock().await.remove(key);
    Json(json!({ "ok": true }))
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    tracing_subscriber::fmt::init();
    let cli = Cli::parse();
    let state = AppState::default();
    let app = Router::new()
        // common
        .route("/health", get(health))
        // supermemory surface (matches thegent-memory v2 SupermemoryAdapter HTTP calls)
        .route("/v1/store", post(sm_store))
        .route("/v1/search", post(sm_search))
        .route("/v1/store/:scope/:key", delete(sm_forget))
        .route("/v1/memories", post(store).get(recall))
        .route("/v1/memories/:key", delete(forget))
        // letta surface (matches thegent-memory v2 LettaAdapter HTTP calls)
        .route(
            "/v1/agents/:id/archival-memory",
            post(letta_store).get(recall),
        )
        .route(
            "/v1/agents/_any_/archival-memory/search",
            post(letta_search),
        )
        .route(
            "/v1/agents/:id/archival-memory/:key",
            delete(letta_forget),
        )
        // cognee surface
        .route("/memory/add", post(store))
        .route("/memory/search", post(post_search))
        .route("/memory/forget", post(post_forget))
        // mem0 surface (matches thegent-memory v2 Mem0Adapter HTTP calls)
        .route("/v1/memories/", post(mem0_store))
        .route("/v1/memories/search", post(mem0_search))
        .route("/v1/memories/:id/", delete(mem0_forget))
        .with_state(state);
    let addr = format!("0.0.0.0:{}", cli.port);
    tracing::info!("pheno-sidecar-stub ({}) listening on {}", cli.sidecar, addr);
    let listener = tokio::net::TcpListener::bind(&addr).await?;
    axum::serve(listener, app).await?;
    Ok(())
}
