mod common;

use reelpick::jellyfin::Movie;
use reelpick::ollama::{OllamaClient, prompt, schema, validate};
use reelpick::select::Candidate;
use wiremock::matchers::{method, path};
use wiremock::{Mock, MockServer, ResponseTemplate};

fn cand(id: i64) -> Candidate {
    Candidate {
        movie: Movie {
            item_id: format!("i{id}"),
            title: format!("Film {id}"),
            year: Some(1995),
            tmdb_id: Some(id),
            genres: vec!["Crime".into()],
            director: Some("Someone".into()),
            overview: None,
            runtime_min: Some(120),
            // IMDb's rating, as Jellyfin holds it — deliberately a different
            // number from TMDB's below.
            community_rating: Some(6.9),
        },
        rating: Some(7.5),
        votes: Some(1000),
        overview: format!("About film {id}."),
    }
}

#[test]
fn the_schema_pins_the_choice_to_the_candidates() {
    let s = schema(&[cand(1), cand(2)]);
    assert_eq!(s["properties"]["choice"]["enum"], serde_json::json!([1, 2]));
    let required = s["required"].as_array().unwrap();
    for f in ["choice", "reason", "teaser", "article"] {
        assert!(required.iter().any(|r| r == f), "missing {f}");
    }
}

#[test]
fn the_prompt_names_every_candidate_the_language_and_the_rules() {
    let p = prompt(&[cand(1), cand(2)], "de");
    assert!(p.contains("Film 1") && p.contains("Film 2"));
    assert!(p.contains("About film 2."));
    assert!(p.to_lowercase().contains("spoiler"));
    assert!(p.contains("de"));
}

#[test]
fn the_prompt_shows_imdbs_rating_and_asks_for_it_in_the_article() {
    let p = prompt(&[cand(1)], "de");
    assert!(p.contains("IMDb rating 6.9 out of 10"), "{p}");
    assert!(p.contains("Name the film's IMDb rating"), "{p}");
    // TMDB decides who is on the shortlist and says nothing to the reader:
    // a model shown two ratings will quote the wrong one sooner or later.
    assert!(!p.contains("7.5"), "{p}");
    assert!(!p.contains("1000"), "{p}");
}

#[test]
fn a_candidate_without_an_imdb_rating_says_so_rather_than_inventing_one() {
    let mut c = cand(1);
    c.movie.community_rating = None;
    let p = prompt(&[c], "de");
    assert!(p.contains("IMDb rating unknown"), "{p}");
}

#[test]
fn the_prompt_writes_for_friends_and_does_not_ask_for_a_family_film() {
    let p = prompt(&[cand(1)], "de").to_lowercase();
    assert!(p.contains("circle of friends"), "{p}");
    assert!(p.contains("grown-up friends, not a family"), "{p}");
    assert!(p.contains("never rule a film out because children"), "{p}");
    // The old wording asked for an evening for "friends and family"; nothing
    // in the prompt may ask for family suitability again.
    assert!(!p.contains("friends and family"), "{p}");
}

#[test]
fn validate_rejects_a_foreign_choice_and_wrong_lengths() {
    let c = [cand(1), cand(2)];
    let ok = common::good_choice(2).to_string();
    assert_eq!(validate(&ok, &c).unwrap().tmdb_id, 2);
    let foreign = common::good_choice(9).to_string();
    assert!(validate(&foreign, &c).is_err());
    let mut short = common::good_choice(1);
    short["teaser"] = "Hm.".into();
    assert!(validate(&short.to_string(), &c).is_err());
    let mut thin = common::good_choice(1);
    thin["article"] = "Too few words.".into();
    assert!(validate(&thin.to_string(), &c).is_err());
    assert!(validate("not json", &c).is_err());
}

#[tokio::test]
async fn choose_sends_the_schema_and_returns_the_parsed_choice() {
    let server = common::ollama(common::good_choice(1)).await;
    let client = OllamaClient::new(&server.uri(), "qwen3:8b").unwrap();
    let choice = client.choose(&[cand(1), cand(2)], "de").await.unwrap();
    assert_eq!(choice.tmdb_id, 1);
    let sent = &server.received_requests().await.unwrap()[0];
    let body: serde_json::Value = serde_json::from_slice(&sent.body).unwrap();
    assert_eq!(body["model"], "qwen3:8b");
    assert_eq!(body["think"], false);
    assert_eq!(body["stream"], false);
    assert_eq!(
        body["format"]["properties"]["choice"]["enum"],
        serde_json::json!([1, 2])
    );
}

#[tokio::test]
async fn choose_retries_once_then_gives_up() {
    let server = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path("/api/chat"))
        .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
            "done": true, "message": {"role": "assistant", "content": "{\"choice\": 77}"}
        })))
        .expect(2)
        .mount(&server)
        .await;
    let client = OllamaClient::new(&server.uri(), "qwen3:8b").unwrap();
    assert!(client.choose(&[cand(1)], "de").await.is_err());
}
