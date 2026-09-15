//! The router: a handful of read-only routes under the base path.

pub mod markup;
pub mod pages;

use std::sync::Arc;

use axum::Router;
use axum::extract::{Path, State as AxumState};
use axum::http::{StatusCode, header};
use axum::response::{IntoResponse, Response};
use axum::routing::get;

use crate::config::Config;
use crate::store::Store;

#[derive(Clone)]
pub struct State {
    pub config: Config,
    pub store: Store,
}

type Shared = Arc<State>;

pub fn router(state: State) -> Router {
    let base = state.config.base_path.clone();
    let shared = Arc::new(state);
    Router::new()
        .route(&format!("{base}/"), get(history))
        .route(&format!("{base}/today.html"), get(fragment))
        .route(&format!("{base}/today"), get(today))
        .route(&format!("{base}/healthz"), get(healthz))
        .route(&format!("{base}/style.css"), get(style))
        .route(&format!("{base}/poster/{{file}}"), get(poster))
        .route(&format!("{base}/{{date}}"), get(article))
        .with_state(shared)
}

fn failed(e: anyhow::Error) -> Response {
    eprintln!("reelpick: {e:#}");
    (StatusCode::INTERNAL_SERVER_ERROR, "something went wrong").into_response()
}

fn is_date(s: &str) -> bool {
    s.len() == 10 && jiff::civil::Date::strptime("%Y-%m-%d", s).is_ok()
}

async fn fragment(AxumState(s): AxumState<Shared>) -> Response {
    match s.store.latest_pick().await {
        Ok(p) => pages::fragment(&s.config, p.as_ref()).into_response(),
        Err(e) => failed(e),
    }
}

async fn today(AxumState(s): AxumState<Shared>) -> Response {
    match s.store.latest_pick().await {
        // `axum::response::Redirect` has no 302 constructor (only 303/307/308);
        // the brief and the tests want a plain 302 Found, so it is built by hand.
        Ok(Some(p)) => (
            StatusCode::FOUND,
            [(header::LOCATION, s.config.route(&format!("/{}", p.date)))],
        )
            .into_response(),
        Ok(None) => StatusCode::NOT_FOUND.into_response(),
        Err(e) => failed(e),
    }
}

async fn healthz(AxumState(s): AxumState<Shared>) -> Response {
    match s.store.latest_pick().await {
        Ok(Some(p)) => (StatusCode::OK, p.date).into_response(),
        Ok(None) => (StatusCode::SERVICE_UNAVAILABLE, "no picks yet").into_response(),
        Err(e) => failed(e),
    }
}

async fn style() -> Response {
    (
        [
            (header::CONTENT_TYPE, "text/css; charset=utf-8"),
            (header::CACHE_CONTROL, "public, max-age=86400"),
        ],
        include_str!("style.css"),
    )
        .into_response()
}

async fn history(AxumState(s): AxumState<Shared>) -> Response {
    match s.store.all_picks().await {
        Ok(picks) => pages::history(&s.config, &picks).into_response(),
        Err(e) => failed(e),
    }
}

async fn article(AxumState(s): AxumState<Shared>, Path(date): Path<String>) -> Response {
    if !is_date(&date) {
        return StatusCode::NOT_FOUND.into_response();
    }
    let pick = match s.store.pick_for(&date).await {
        Ok(Some(p)) => p,
        Ok(None) => return StatusCode::NOT_FOUND.into_response(),
        Err(e) => return failed(e),
    };
    let (prev, next) = match s.store.neighbours(&date).await {
        Ok(n) => n,
        Err(e) => return failed(e),
    };
    let body = markup::render(&pick.article_md);
    pages::article(&s.config, &pick, &body, prev.as_deref(), next.as_deref()).into_response()
}

async fn poster(AxumState(s): AxumState<Shared>, Path(file): Path<String>) -> Response {
    // Only `<date>.jpg` — the path is built from the date, never from the request.
    let Some(date) = file.strip_suffix(".jpg") else {
        return StatusCode::NOT_FOUND.into_response();
    };
    if !is_date(date) {
        return StatusCode::NOT_FOUND.into_response();
    }
    match tokio::fs::read(
        s.config
            .data_dir
            .join("posters")
            .join(format!("{date}.jpg")),
    )
    .await
    {
        Ok(bytes) => (
            [
                (header::CONTENT_TYPE, "image/jpeg"),
                (header::CACHE_CONTROL, "public, max-age=86400"),
            ],
            bytes,
        )
            .into_response(),
        Err(_) => StatusCode::NOT_FOUND.into_response(),
    }
}
