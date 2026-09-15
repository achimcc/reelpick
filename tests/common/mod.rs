#![allow(dead_code)]

use wiremock::matchers::{header, method, path, query_param};
use wiremock::{Mock, MockServer, ResponseTemplate};

pub const LIBRARY_ID: &str = "f137a2dd21bbc1b99aa5c0f6bf02a805";

pub fn movie_json(id: &str, title: &str, tmdb: i64, rating: f64) -> serde_json::Value {
    serde_json::json!({
        "Name": title, "Id": id, "ProductionYear": 1995,
        "ProviderIds": {"Tmdb": tmdb.to_string()},
        "Genres": ["Crime", "Drama"], "CommunityRating": rating,
        "Overview": format!("Overview of {title}."), "RunTimeTicks": 102000000000i64,
        "People": [{"Name": "Michael Mann", "Type": "Director"}, {"Name": "Al Pacino", "Type": "Actor"}]
    })
}

/// A Jellyfin with one library and the given films, answering only to the key.
pub async fn jellyfin(api_key: &str, movies: Vec<serde_json::Value>) -> MockServer {
    let server = MockServer::start().await;
    let auth = format!("MediaBrowser Token=\"{api_key}\"");
    Mock::given(method("GET")).and(path("/Library/MediaFolders")).and(header("Authorization", auth.as_str()))
        .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
            "Items": [{"Name": "Filme", "Id": LIBRARY_ID, "CollectionType": "movies"}], "TotalRecordCount": 1
        })))
        .mount(&server).await;
    let count = movies.len();
    Mock::given(method("GET"))
        .and(path("/Items"))
        .and(query_param("ParentId", LIBRARY_ID))
        .and(query_param("IncludeItemTypes", "Movie"))
        .and(header("Authorization", auth.as_str()))
        .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
            "Items": movies, "TotalRecordCount": count
        })))
        .mount(&server)
        .await;
    Mock::given(method("GET"))
        .and(path("/Items/abc/Images/Primary"))
        .respond_with(ResponseTemplate::new(200).set_body_bytes(b"\xFF\xD8\xFF poster".to_vec()))
        .mount(&server)
        .await;
    server
}

/// A TMDB that knows the given films: (tmdb_id, rating, votes).
pub async fn tmdb(api_key: &str, films: &[(i64, f64, i64)]) -> MockServer {
    let server = MockServer::start().await;
    for (id, rating, votes) in films {
        Mock::given(method("GET"))
            .and(path(format!("/3/movie/{id}")))
            .and(query_param("api_key", api_key))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
                "id": id, "vote_average": rating, "vote_count": votes,
                "overview": format!("TMDB says: film {id}."), "runtime": 170
            })))
            .mount(&server)
            .await;
    }
    server
}

/// An Ollama that answers every chat with the given choice.
pub async fn ollama(choice: serde_json::Value) -> MockServer {
    let server = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path("/api/chat"))
        .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
            "model": "qwen3:8b", "done": true,
            "message": {"role": "assistant", "content": choice.to_string()}
        })))
        .mount(&server)
        .await;
    server
}

pub fn good_choice(tmdb_id: i64) -> serde_json::Value {
    serde_json::json!({
        "choice": tmdb_id,
        "reason": "Of the five, this one is the evening.",
        "teaser": "Two men on opposite sides of the law. One city that cannot hold both.",
        "article": "### Why tonight\n\n".to_string() + &"A sentence about the film. ".repeat(40)
    })
}
