mod common;

use rand::SeedableRng;
use reelpick::config::Config;
use reelpick::jellyfin::JellyfinClient;
use reelpick::ollama::OllamaClient;
use reelpick::pick::{Clients, Outcome, run};
use reelpick::store::Store;
use reelpick::tmdb::TmdbClient;

struct World {
    _dir: tempfile::TempDir,
    config: Config,
    store: Store,
    jellyfin: JellyfinClient,
    tmdb: Option<TmdbClient>,
    ollama: OllamaClient,
}

async fn world(
    jelly: &wiremock::MockServer,
    tmdb: Option<&wiremock::MockServer>,
    ollama: &wiremock::MockServer,
) -> World {
    let dir = tempfile::tempdir().unwrap();
    let text = format!(
        "data_dir = {:?}\nlanguage = \"de\"\n[jellyfin]\nurl = {:?}\npublic_url = \"https://j.example\"\nlibrary = \"Filme\"\n[ollama]\nurl = {:?}\nmodel = \"qwen3:8b\"\n",
        dir.path().to_str().unwrap(),
        jelly.uri(),
        ollama.uri()
    );
    let config = Config::from_toml(&text).unwrap();
    let store = Store::open(&dir.path().join("reelpick.db")).await.unwrap();
    World {
        jellyfin: JellyfinClient::new(&jelly.uri(), "k3y").unwrap(),
        tmdb: tmdb.map(|t| TmdbClient::new("t0k", "de").unwrap().with_base(&t.uri())),
        ollama: OllamaClient::new(&ollama.uri(), "qwen3:8b").unwrap(),
        _dir: dir,
        config,
        store,
    }
}

impl World {
    async fn pick(&self, day: &str) -> Outcome {
        let clients = Clients {
            jellyfin: &self.jellyfin,
            tmdb: self.tmdb.as_ref(),
            ollama: &self.ollama,
        };
        let mut rng = rand::rngs::StdRng::seed_from_u64(1);
        run(&self.config, &self.store, clients, day, &mut rng)
            .await
            .unwrap()
    }
}

fn three_films() -> Vec<serde_json::Value> {
    vec![
        common::movie_json("abc", "Heat", 949, 7.9),
        common::movie_json("def", "Dud", 2, 4.0),
        common::movie_json("ghi", "Obscure Gem", 3, 9.0),
    ]
}

#[tokio::test]
async fn a_full_run_picks_writes_a_row_and_stores_the_poster() {
    let jelly = common::jellyfin("k3y", three_films()).await;
    let tmdb = common::tmdb("t0k", &[(949, 7.9, 6500), (2, 4.0, 5000), (3, 9.0, 12)]).await;
    let ollama = common::ollama(common::good_choice(949)).await;
    let w = world(&jelly, Some(&tmdb), &ollama).await;
    let Outcome::Picked(p) = w.pick("2026-09-15").await else {
        panic!("expected a pick")
    };
    assert_eq!(
        (p.tmdb_id, p.title.as_str(), p.generated_by.as_str()),
        (949, "Heat", "ollama")
    );
    assert_eq!(p.votes, Some(6500));
    assert!(p.has_poster);
    assert!(w.config.data_dir.join("posters/2026-09-15.jpg").exists());
    assert_eq!(
        w.store
            .pick_for("2026-09-15")
            .await
            .unwrap()
            .unwrap()
            .teaser,
        p.teaser
    );
    // Only Heat was eligible: Dud is rated too low, Obscure Gem has too few votes.
    let sent = &ollama.received_requests().await.unwrap()[0];
    let body: serde_json::Value = serde_json::from_slice(&sent.body).unwrap();
    assert_eq!(
        body["format"]["properties"]["choice"]["enum"],
        serde_json::json!([949])
    );
}

#[tokio::test]
async fn the_second_run_on_the_same_day_does_nothing() {
    let jelly = common::jellyfin("k3y", three_films()).await;
    let tmdb = common::tmdb("t0k", &[(949, 7.9, 6500)]).await;
    let ollama = common::ollama(common::good_choice(949)).await;
    let w = world(&jelly, Some(&tmdb), &ollama).await;
    w.pick("2026-09-15").await;
    assert!(matches!(w.pick("2026-09-15").await, Outcome::AlreadyPicked(d) if d == "2026-09-15"));
    assert_eq!(ollama.received_requests().await.unwrap().len(), 1);
}

#[tokio::test]
async fn without_ollama_the_best_rated_wins_and_is_marked_fallback() {
    let jelly = common::jellyfin(
        "k3y",
        vec![
            common::movie_json("abc", "Heat", 949, 7.9),
            common::movie_json("xyz", "Better", 5, 8.5),
        ],
    )
    .await;
    let tmdb = common::tmdb("t0k", &[(949, 7.9, 6500), (5, 8.5, 3000)]).await;
    let ollama = wiremock::MockServer::start().await; // answers 404 to everything
    let w = world(&jelly, Some(&tmdb), &ollama).await;
    let Outcome::Picked(p) = w.pick("2026-09-15").await else {
        panic!()
    };
    assert_eq!((p.tmdb_id, p.generated_by.as_str()), (5, "fallback"));
    assert!(p.model.is_none());
    assert!(p.teaser.starts_with("TMDB says"));
}

#[tokio::test]
async fn without_tmdb_jellyfins_rating_carries_the_threshold() {
    let jelly = common::jellyfin("k3y", three_films()).await;
    let ollama = common::ollama(common::good_choice(949)).await;
    let w = world(&jelly, None, &ollama).await;
    let Outcome::Picked(p) = w.pick("2026-09-15").await else {
        panic!()
    };
    assert!(p.votes.is_none());
    // Dud (4.0) is out; Heat (7.9) and Obscure Gem (9.0, no vote count known) are in.
    let sent = &ollama.received_requests().await.unwrap()[0];
    let body: serde_json::Value = serde_json::from_slice(&sent.body).unwrap();
    let ids = body["format"]["properties"]["choice"]["enum"]
        .as_array()
        .unwrap();
    assert_eq!(ids.len(), 2);
}

#[tokio::test]
async fn when_every_film_was_picked_the_oldest_come_back() {
    let jelly = common::jellyfin("k3y", vec![common::movie_json("abc", "Heat", 949, 7.9)]).await;
    let tmdb = common::tmdb("t0k", &[(949, 7.9, 6500)]).await;
    let ollama = common::ollama(common::good_choice(949)).await;
    let w = world(&jelly, Some(&tmdb), &ollama).await;
    w.pick("2026-09-15").await;
    let Outcome::Picked(p) = w.pick("2026-09-16").await else {
        panic!("the pool is exhausted, the oldest returns")
    };
    assert_eq!(p.tmdb_id, 949);
}

#[tokio::test]
async fn an_empty_library_is_an_error_that_says_so() {
    let jelly = common::jellyfin("k3y", vec![]).await;
    let ollama = common::ollama(common::good_choice(1)).await;
    let w = world(&jelly, None, &ollama).await;
    let clients = Clients {
        jellyfin: &w.jellyfin,
        tmdb: None,
        ollama: &w.ollama,
    };
    let mut rng = rand::rngs::StdRng::seed_from_u64(1);
    let err = run(&w.config, &w.store, clients, "2026-09-15", &mut rng)
        .await
        .unwrap_err()
        .to_string();
    assert!(err.contains("no eligible film"), "{err}");
}
