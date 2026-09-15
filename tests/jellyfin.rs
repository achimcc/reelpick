mod common;

use reelpick::jellyfin::JellyfinClient;

#[tokio::test]
async fn resolves_the_library_reads_the_films_and_fetches_a_poster() {
    let server = common::jellyfin("k3y", vec![common::movie_json("abc", "Heat", 949, 7.9)]).await;
    let client = JellyfinClient::new(&server.uri(), "k3y").unwrap();
    let lib = client.library_id("Filme").await.unwrap();
    assert_eq!(lib, common::LIBRARY_ID);
    let films = client.movies(&lib).await.unwrap();
    assert_eq!(films.len(), 1);
    let heat = &films[0];
    assert_eq!(heat.tmdb_id, Some(949));
    assert_eq!(heat.director.as_deref(), Some("Michael Mann"));
    assert_eq!(heat.runtime_min, Some(170));
    assert_eq!(heat.genres, ["Crime", "Drama"]);
    let poster = client.poster("abc").await.unwrap().unwrap();
    assert!(poster.starts_with(&[0xFF, 0xD8]));
    assert!(client.poster("zzz").await.unwrap().is_none());
}

#[tokio::test]
async fn an_unknown_library_is_an_error_that_names_it() {
    let server = common::jellyfin("k3y", vec![]).await;
    let client = JellyfinClient::new(&server.uri(), "k3y").unwrap();
    let err = client.library_id("Serien").await.unwrap_err().to_string();
    assert!(err.contains("Serien"), "{err}");
}

#[tokio::test]
async fn a_wrong_key_is_an_error_not_an_empty_library() {
    let server = common::jellyfin("k3y", vec![]).await;
    let client = JellyfinClient::new(&server.uri(), "wrong").unwrap();
    assert!(client.library_id("Filme").await.is_err());
}
