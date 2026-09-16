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

/// Same as `app`, but with an explicit `language` — the chrome texts are the
/// only thing that changes with it, so both cases are worth a page each.
async fn app_in(language: &str, picks: &[Pick]) -> (tempfile::TempDir, axum::Router) {
    let dir = tempfile::tempdir().unwrap();
    let text = format!(
        "base_path = \"/reelpick\"\nlanguage = {language:?}\ndata_dir = {:?}\nhome_url = \"https://home.example/\"\n[jellyfin]\nurl = \"http://j\"\npublic_url = \"https://jellyfin.example\"\nlibrary = \"Filme\"\n[ollama]\nurl = \"http://o\"\nmodel = \"m\"\n",
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

/// Same as `app`, but the config omits `base_path` entirely, so every route
/// is served from the root instead of under `/reelpick`.
async fn app_at_root(picks: &[Pick]) -> (tempfile::TempDir, axum::Router) {
    let dir = tempfile::tempdir().unwrap();
    let text = format!(
        "data_dir = {:?}\nhome_url = \"https://home.example/\"\n[jellyfin]\nurl = \"http://j\"\npublic_url = \"https://jellyfin.example\"\nlibrary = \"Filme\"\n[ollama]\nurl = \"http://o\"\nmodel = \"m\"\n",
        dir.path().to_str().unwrap()
    );
    let config = Config::from_toml(&text).unwrap();
    assert_eq!(config.base_path, "");
    let store = Store::open(&dir.path().join("db")).await.unwrap();
    for p in picks {
        store.insert_pick(p).await.unwrap();
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
    let (status, headers, body) = get(&app, "/reelpick/today.html").await;
    assert_eq!(status, StatusCode::OK);
    assert!(!body.contains("<html"));
    assert!(body.contains("Heat") && !body.contains("Older"));
    assert!(body.contains("href=\"/reelpick/2026-09-15\""));
    assert!(body.contains("src=\"/reelpick/poster/2026-09-15.jpg\""));
    assert!(body.contains("Two men. One city."));
    assert!(body.contains("class=\"reelpick-"));
    assert_eq!(headers["x-content-type-options"], "nosniff");
}

#[tokio::test]
async fn the_fragment_escapes_braces_so_an_embedding_caddy_template_cannot_execute_it() {
    let mut p = pick("2026-09-15", "Brace {film}", true);
    p.teaser = "{{.Req.Header}} and {{ .Foo }}".into();
    let (_d, app) = app(&[p]).await;
    let (status, _, body) = get(&app, "/reelpick/today.html").await;
    assert_eq!(status, StatusCode::OK);
    assert!(!body.contains("{{"));
    assert!(!body.contains('{'));
    assert!(!body.contains('}'));
    assert!(body.contains("Brace"));
    assert!(body.contains(".Req.Header"));
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
    assert!(body.contains("Directed by Michael Mann"));
    // The vote count is grouped in the separator of the language: `6,500` in
    // English, `6.500` in German — never the bare `6500` of a debug print.
    assert!(body.contains("7.9/10 from 6,500 votes"), "{body}");
    assert!(body.contains("170 min"));
    assert!(body.contains("Watch in Jellyfin"));
    assert!(body.contains("<title>Heat — reelpick</title>"));
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
    assert!(body.contains("2026-09-15") && body.contains("2026-08-31"));
    // The month is a heading in words, not the `2026-09` the database groups by.
    assert!(body.contains("[ SEPTEMBER 2026 ]") && body.contains("[ AUGUST 2026 ]"));
    assert!(!body.contains(">2026-09<"), "{body}");
}

/// The operator asked for this in so many words: every pick in the history is
/// the same compact card the front page shows, not a bare title in a list.
#[tokio::test]
async fn every_pick_in_the_history_is_the_card_the_front_page_shows() {
    let (_d, app) = app(&[
        pick("2026-09-14", "Older", false),
        pick("2026-09-15", "Heat", true),
    ])
    .await;
    let (status, _, body) = get(&app, "/reelpick/").await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(
        body.matches("class=\"reelpick-field\"").count(),
        2,
        "{body}"
    );
    assert_eq!(body.matches("class=\"reelpick-teaser\"").count(), 2);
    assert!(body.contains("class=\"reelpick-grid\""));
    assert!(body.contains("Two men. One city."));
    // The poster of the pick that has one, and no broken `img` for the one
    // that has not.
    assert!(body.contains("src=\"/reelpick/poster/2026-09-15.jpg\""));
    assert!(!body.contains("src=\"/reelpick/poster/2026-09-14.jpg\""));
    assert_eq!(body.matches("class=\"reelpick-poster\"").count(), 1);
    // The card is the link, so a title is not a second one next to it.
    assert!(body.contains("<a class=\"reelpick-field\" href=\"/reelpick/2026-09-15\""));
    // The clothes of the front page: prompt line, cursor, section heading.
    assert!(body.contains("root@home.example") && body.contains("class=\"kursor\""));
    assert!(body.contains("[ ALL PICKS ]"));
}

#[tokio::test]
async fn an_empty_history_says_so_instead_of_showing_an_empty_grid() {
    let (_d, app) = app(&[]).await;
    let (status, _, body) = get(&app, "/reelpick/").await;
    assert_eq!(status, StatusCode::OK);
    assert!(body.contains("No pick yet."));
    assert!(!body.contains("reelpick-grid"));
}

/// The pages are read by people who configured `language = "de"`; the words
/// around the film follow that, the markup does not change with it.
#[tokio::test]
async fn the_chrome_texts_follow_the_configured_language() {
    let picks = [pick("2026-09-15", "Heat", true)];

    let (_d, de) = app_in("de", &picks).await;
    let (_, _, fragment) = get(&de, "/reelpick/today.html").await;
    assert!(fragment.contains("Alle bisherigen Tipps"), "{fragment}");
    assert!(!fragment.contains("All picks so far"));
    let (_, _, article) = get(&de, "/reelpick/2026-09-15").await;
    assert!(article.contains("<html lang=\"de\""));
    assert!(article.contains("In Jellyfin ansehen"));
    assert!(article.contains("Regie: Michael Mann"));
    assert!(article.contains("170 Min."));
    // German writes 7,9 and groups thousands with a point.
    assert!(article.contains("7,9/10 bei 6.500 Stimmen"), "{article}");
    assert!(article.contains("Tipp vom"));
    assert!(article.contains("← Startseite"));
    let (_, _, history) = get(&de, "/reelpick/").await;
    assert!(history.contains("[ ALLE TIPPS ]"));
    assert!(history.contains("[ SEPTEMBER 2026 ]"));
    assert!(history.contains("Bisher ein Tipp."));
    assert!(!history.contains("All picks"));

    let (_d, en) = app_in("en", &picks).await;
    let (_, _, fragment) = get(&en, "/reelpick/today.html").await;
    assert!(fragment.contains("All picks so far"));
    let (_, _, article) = get(&en, "/reelpick/2026-09-15").await;
    assert!(article.contains("<html lang=\"en\""));
    assert!(article.contains("Watch in Jellyfin"));
    assert!(article.contains("7.9/10 from 6,500 votes"), "{article}");
    let (_, _, history) = get(&en, "/reelpick/").await;
    assert!(history.contains("[ ALL PICKS ]") && history.contains("[ SEPTEMBER 2026 ]"));
    assert!(!history.contains("Alle Tipps"));

    // An empty history is a sentence in the configured language too.
    let (_d, de_empty) = app_in("de", &[]).await;
    let (_, _, body) = get(&de_empty, "/reelpick/today.html").await;
    assert!(body.contains("Noch kein Tipp."));
}

/// The stylesheet is what makes the pages look like the front page; it is
/// served under the base path, as CSS, and carries the palette it copied.
#[tokio::test]
async fn the_stylesheet_is_css_under_the_base_path_and_carries_the_palette() {
    let (_d, app) = app(&[]).await;
    let (status, headers, body) = get(&app, "/reelpick/style.css").await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(headers["content-type"], "text/css; charset=utf-8");
    assert!(body.contains("--auf: #4ef08a"), "{body}");
    assert!(body.contains(".reelpick-field"));
    assert!(!body.contains("prefers-color-scheme: light"));
    // And every page asks for exactly this file, under the base path.
    let (_, _, page) = get(&app, "/reelpick/").await;
    assert!(page.contains("href=\"/reelpick/style.css\""));
    assert!(!page.contains("<script"));
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

#[tokio::test]
async fn an_empty_base_path_serves_every_route_from_the_root() {
    let (_d, app) = app_at_root(&[]).await;
    assert_eq!(get(&app, "/").await.0, StatusCode::OK);
    assert_eq!(get(&app, "/today.html").await.0, StatusCode::OK);
    assert_eq!(
        get(&app, "/healthz").await.0,
        StatusCode::SERVICE_UNAVAILABLE
    );
    assert_eq!(get(&app, "/style.css").await.0, StatusCode::OK);
}
