mod common;

use reelpick::jellyfin::Movie;
use reelpick::store::{Store, TmdbEntry};
use reelpick::tmdb::{TmdbClient, enrich};
use wiremock::matchers::{method, path};
use wiremock::{Mock, MockServer, ResponseTemplate};

fn movie(id: i64, rating: f64) -> Movie {
    Movie {
        item_id: format!("item-{id}"),
        title: format!("Film {id}"),
        year: Some(2000),
        tmdb_id: Some(id),
        genres: vec![],
        director: None,
        overview: Some(format!("Jellyfin says: film {id}.")),
        runtime_min: Some(100),
        community_rating: Some(rating),
    }
}

async fn store() -> (tempfile::TempDir, Store) {
    let dir = tempfile::tempdir().unwrap();
    let s = Store::open(&dir.path().join("db")).await.unwrap();
    (dir, s)
}

#[tokio::test]
async fn the_client_reads_rating_votes_overview_and_runtime() {
    let server = common::tmdb("t0k", &[(949, 7.9, 6500)]).await;
    let client = TmdbClient::new("t0k", "de")
        .unwrap()
        .with_base(&server.uri());
    let e = client.movie(949).await.unwrap();
    assert_eq!(
        (e.rating, e.votes, e.runtime),
        (Some(7.9), Some(6500), Some(170))
    );
    assert_eq!(e.overview.as_deref(), Some("TMDB says: film 949."));
    assert!(client.movie(1).await.is_err());
}

#[tokio::test]
async fn enrich_asks_tmdb_once_then_serves_from_the_cache() {
    let server = common::tmdb("t0k", &[(1, 7.0, 400)]).await;
    let client = TmdbClient::new("t0k", "de")
        .unwrap()
        .with_base(&server.uri());
    let (_d, s) = store().await;
    let first = enrich(&s, Some(&client), vec![movie(1, 5.0)], "2026-09-15")
        .await
        .unwrap();
    assert_eq!((first[0].rating, first[0].votes), (Some(7.0), Some(400)));
    assert_eq!(first[0].overview, "TMDB says: film 1.");
    server.reset().await; // TMDB now answers nothing
    let second = enrich(&s, Some(&client), vec![movie(1, 5.0)], "2026-09-20")
        .await
        .unwrap();
    assert_eq!(second[0].votes, Some(400));
}

#[tokio::test]
async fn without_tmdb_a_stale_entry_beats_jellyfin_and_jellyfin_beats_nothing() {
    let (_d, s) = store().await;
    s.put_tmdb(
        &TmdbEntry {
            tmdb_id: 1,
            rating: Some(8.0),
            votes: Some(900),
            overview: Some("old".into()),
            runtime: None,
        },
        "2020-01-01",
    )
    .await
    .unwrap();
    let out = enrich(&s, None, vec![movie(1, 5.0), movie(2, 6.9)], "2026-09-15")
        .await
        .unwrap();
    assert_eq!((out[0].rating, out[0].votes), (Some(8.0), Some(900)));
    assert_eq!((out[1].rating, out[1].votes), (Some(6.9), None));
    assert_eq!(out[1].overview, "Jellyfin says: film 2.");
}

// Regression: the API key travels in the query string, so it must never
// surface in an error's Display (`{:#}`) or Debug (`{:?}`) form, however
// TMDB or the network misbehave.

#[tokio::test]
async fn the_key_never_leaks_into_an_error_on_an_invalid_body() {
    let server = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path("/3/movie/1"))
        .respond_with(ResponseTemplate::new(200).set_body_string("not json"))
        .mount(&server)
        .await;
    let client = TmdbClient::new("t0k", "de")
        .unwrap()
        .with_base(&server.uri());
    let err = client.movie(1).await.unwrap_err();
    assert!(!format!("{err:#}").contains("t0k"));
    assert!(!format!("{err:?}").contains("t0k"));
}

#[tokio::test]
async fn the_key_never_leaks_into_an_error_on_a_server_error() {
    let server = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path("/3/movie/1"))
        .respond_with(ResponseTemplate::new(500))
        .mount(&server)
        .await;
    let client = TmdbClient::new("t0k", "de")
        .unwrap()
        .with_base(&server.uri());
    let err = client.movie(1).await.unwrap_err();
    assert!(!format!("{err:#}").contains("t0k"));
    assert!(!format!("{err:?}").contains("t0k"));
}

#[tokio::test]
async fn the_key_never_leaks_into_an_error_on_a_redirect_to_itself() {
    let server = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path("/3/movie/1"))
        .respond_with(ResponseTemplate::new(302).insert_header("Location", "/3/movie/1"))
        .mount(&server)
        .await;
    let client = TmdbClient::new("t0k", "de")
        .unwrap()
        .with_base(&server.uri());
    let err = client.movie(1).await.unwrap_err();
    assert!(!format!("{err:#}").contains("t0k"));
    assert!(!format!("{err:?}").contains("t0k"));
}

#[tokio::test]
async fn a_failing_tmdb_falls_back_the_same_way() {
    let server = common::tmdb("t0k", &[]).await; // knows nothing: every film is a 404
    let client = TmdbClient::new("t0k", "de")
        .unwrap()
        .with_base(&server.uri());
    let (_d, s) = store().await;
    let out = enrich(&s, Some(&client), vec![movie(3, 7.2)], "2026-09-15")
        .await
        .unwrap();
    assert_eq!((out[0].rating, out[0].votes), (Some(7.2), None));
}
