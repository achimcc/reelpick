mod common;

use axum::body::Body;
use axum::http::{Request, StatusCode};
use reelpick::config::Config;
use reelpick::store::{Pick, Store};
use reelpick::web::{State, router};
use tower::ServiceExt;

fn pick(date: &str, title: &str, poster: bool) -> Pick {
    Pick {
        date: date.into(),
        item_id: format!("item-{date}"),
        tmdb_id: 949,
        title: title.into(),
        year: Some(1995),
        runtime_min: Some(170),
        genres: vec!["Crime".into(), "Drama".into()],
        director: Some("Michael Mann".into()),
        rating: Some(7.9),
        votes: Some(6500),
        reason: "Because tonight.".into(),
        teaser: "Two men. One city.".into(),
        article_md: "### Why\n\nA *great* film.\n\n<script>alert(1)</script>".into(),
        generated_by: "ollama".into(),
        model: Some("qwen3:8b".into()),
        has_poster: poster,
    }
}

async fn app(picks: &[Pick]) -> (tempfile::TempDir, axum::Router) {
    let dir = tempfile::tempdir().unwrap();
    let text = format!(
        "base_path = \"/reelpick\"\ndata_dir = {:?}\nhome_url = \"https://home.example/\"\n[jellyfin]\nurl = \"http://j\"\npublic_url = \"https://jellyfin.example\"\nlibrary = \"Filme\"\n[ollama]\nurl = \"http://o\"\nmodel = \"m\"\n",
        dir.path().to_str().unwrap()
    );
    let config = Config::from_toml(&text).unwrap();
    let store = Store::open(&dir.path().join("db")).await.unwrap();
    for p in picks {
        store.insert_pick(p).await.unwrap();
        if p.has_poster {
            std::fs::create_dir_all(dir.path().join("posters")).unwrap();
            std::fs::write(
                dir.path().join(format!("posters/{}.jpg", p.date)),
                b"\xFF\xD8jpg",
            )
            .unwrap();
        }
    }
    (dir, router(State { config, store }))
}

async fn get(app: &axum::Router, path: &str) -> (StatusCode, axum::http::HeaderMap, String) {
    let res = app
        .clone()
        .oneshot(Request::get(path).body(Body::empty()).unwrap())
        .await
        .unwrap();
    let status = res.status();
    let headers = res.headers().clone();
    let bytes = axum::body::to_bytes(res.into_body(), 1 << 20)
        .await
        .unwrap();
    (
        status,
        headers,
        String::from_utf8_lossy(&bytes).into_owned(),
    )
}

#[tokio::test]
async fn the_fragment_shows_the_latest_pick_and_links_into_the_article() {
    let (_d, app) = app(&[
        pick("2026-09-14", "Older", false),
        pick("2026-09-15", "Heat", true),
    ])
    .await;
    let (status, _, body) = get(&app, "/reelpick/today.html").await;
    assert_eq!(status, StatusCode::OK);
    assert!(!body.contains("<html"));
    assert!(body.contains("Heat") && !body.contains("Older"));
    assert!(body.contains("href=\"/reelpick/2026-09-15\""));
    assert!(body.contains("src=\"/reelpick/poster/2026-09-15.jpg\""));
    assert!(body.contains("Two men. One city."));
    assert!(body.contains("class=\"reelpick-"));
}

#[tokio::test]
async fn without_any_pick_the_fragment_is_a_quiet_placeholder_and_healthz_is_503() {
    let (_d, app) = app(&[]).await;
    let (status, _, body) = get(&app, "/reelpick/today.html").await;
    assert_eq!(status, StatusCode::OK);
    assert!(body.contains("reelpick-empty"));
    assert_eq!(
        get(&app, "/reelpick/healthz").await.0,
        StatusCode::SERVICE_UNAVAILABLE
    );
    assert_eq!(get(&app, "/reelpick/today").await.0, StatusCode::NOT_FOUND);
}

#[tokio::test]
async fn the_article_renders_markdown_safely_and_links_to_jellyfin_and_its_neighbours() {
    let (_d, app) = app(&[
        pick("2026-09-14", "Older", false),
        pick("2026-09-15", "Heat", true),
        pick("2026-09-16", "Newer", false),
    ])
    .await;
    let (status, _, body) = get(&app, "/reelpick/2026-09-15").await;
    assert_eq!(status, StatusCode::OK);
    assert!(body.contains("<em>great</em>"));
    assert!(!body.contains("<script"));
    assert!(body.contains("https://jellyfin.example/web/index.html#/details?id=item-2026-09-15"));
    assert!(
        body.contains("href=\"/reelpick/2026-09-14\"")
            && body.contains("href=\"/reelpick/2026-09-16\"")
    );
    assert!(body.contains("Michael Mann") && body.contains("6500") && body.contains("7.9"));
    assert!(body.contains("https://home.example/"));
    assert!(!body.contains("<script>") && !body.contains("javascript:"));
    assert_eq!(
        get(&app, "/reelpick/2026-01-01").await.0,
        StatusCode::NOT_FOUND
    );
    assert_eq!(
        get(&app, "/reelpick/not-a-date").await.0,
        StatusCode::NOT_FOUND
    );
}

#[tokio::test]
async fn the_history_lists_everything_newest_first_grouped_by_month() {
    let (_d, app) = app(&[
        pick("2026-08-31", "August", false),
        pick("2026-09-15", "Heat", false),
    ])
    .await;
    let (status, _, body) = get(&app, "/reelpick/").await;
    assert_eq!(status, StatusCode::OK);
    let heat = body.find("Heat").unwrap();
    let august = body.find("August").unwrap();
    assert!(heat < august);
    assert!(body.contains("2026-09") && body.contains("2026-08"));
}

#[tokio::test]
async fn today_redirects_poster_serves_and_healthz_names_the_date() {
    let (_d, app) = app(&[pick("2026-09-15", "Heat", true)]).await;
    let (status, headers, _) = get(&app, "/reelpick/today").await;
    assert_eq!(status, StatusCode::FOUND);
    assert_eq!(headers["location"], "/reelpick/2026-09-15");
    let (status, headers, body) = get(&app, "/reelpick/poster/2026-09-15.jpg").await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(headers["content-type"], "image/jpeg");
    assert!(!body.is_empty());
    assert_eq!(
        get(&app, "/reelpick/poster/2026-09-14.jpg").await.0,
        StatusCode::NOT_FOUND
    );
    assert_eq!(
        get(&app, "/reelpick/poster/../db").await.0,
        StatusCode::NOT_FOUND
    );
    let (status, _, body) = get(&app, "/reelpick/healthz").await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(body.trim(), "2026-09-15");
    assert_eq!(get(&app, "/reelpick/style.css").await.0, StatusCode::OK);
}
