//! The daily run: read, filter, sample, ask, store. Idempotent per day.

use anyhow::{Context, bail};

use crate::config::Config;
use crate::jellyfin::JellyfinClient;
use crate::ollama::{Choice, OllamaClient};
use crate::select::{Candidate, Excluded, Thresholds, best_rated, eligible, sample};
use crate::store::{Pick, Store};
use crate::tmdb::{TmdbClient, enrich};

pub const EXHAUSTION_WINDOW: i64 = 20;

pub struct Clients<'a> {
    pub jellyfin: &'a JellyfinClient,
    pub tmdb: Option<&'a TmdbClient>,
    pub ollama: &'a OllamaClient,
}

#[derive(Debug)]
// `Pick` is the larger variant by design: it is the whole record, returned
// once per call and never stored in a long-lived collection, so boxing it
// would only add indirection without a real benefit.
#[allow(clippy::large_enum_variant)]
pub enum Outcome {
    AlreadyPicked(String),
    Picked(Pick),
}

pub async fn run<R: rand::Rng>(
    config: &Config,
    store: &Store,
    clients: Clients<'_>,
    today: &str,
    rng: &mut R,
) -> anyhow::Result<Outcome> {
    if store.pick_for(today).await?.is_some() {
        return Ok(Outcome::AlreadyPicked(today.to_string()));
    }

    let library = clients
        .jellyfin
        .library_id(&config.jellyfin.library)
        .await?;
    let movies = clients.jellyfin.movies(&library).await?;
    let candidates = enrich(store, clients.tmdb, movies, today).await?;
    let thresholds = Thresholds {
        min_rating: config.tmdb.min_rating,
        min_votes: config.tmdb.min_votes,
    };

    let mut pool = eligible(candidates.clone(), &thresholds, &store.excluded().await?);
    if pool.is_empty() {
        // Everything has had its day. The ones longest ago come back first.
        let oldest = store.oldest_picks(EXHAUSTION_WINDOW).await?;
        let mut still_excluded = Excluded::default();
        for p in store.all_picks().await? {
            if !oldest.iter().any(|o| o.date == p.date) {
                still_excluded.tmdb_ids.insert(p.tmdb_id);
                still_excluded.item_ids.insert(p.item_id.clone());
            }
        }
        pool = eligible(candidates, &thresholds, &still_excluded);
    }
    if pool.is_empty() {
        bail!("no eligible film: the library is empty or nothing clears the threshold");
    }

    let shortlist = sample(rng, pool, config.sample_size);
    let (choice, generated_by, model) =
        match clients.ollama.choose(&shortlist, &config.language).await {
            Ok(c) => (c, "ollama", Some(clients.ollama.model().to_string())),
            Err(e) => {
                eprintln!("reelpick: {e:#}; falling back to the rating");
                (fallback(&shortlist)?, "fallback", None)
            }
        };
    let chosen = shortlist
        .iter()
        .find(|c| c.movie.tmdb_id == Some(choice.tmdb_id))
        .context("the choice is not in the shortlist")?;

    let has_poster = match clients.jellyfin.poster(&chosen.movie.item_id).await {
        Ok(Some(bytes)) => {
            let dir = config.data_dir.join("posters");
            tokio::fs::create_dir_all(&dir).await?;
            tokio::fs::write(dir.join(format!("{today}.jpg")), bytes).await?;
            true
        }
        Ok(None) => false,
        Err(e) => {
            eprintln!("reelpick: no poster: {e:#}");
            false
        }
    };

    let pick = Pick {
        date: today.to_string(),
        item_id: chosen.movie.item_id.clone(),
        tmdb_id: choice.tmdb_id,
        title: chosen.movie.title.clone(),
        year: chosen.movie.year,
        runtime_min: chosen.movie.runtime_min,
        genres: chosen.movie.genres.clone(),
        director: chosen.movie.director.clone(),
        rating: chosen.rating,
        votes: chosen.votes,
        reason: choice.reason,
        teaser: choice.teaser,
        article_md: choice.article,
        generated_by: generated_by.to_string(),
        model,
        has_poster,
    };
    store.insert_pick(&pick).await?;
    Ok(Outcome::Picked(pick))
}

/// No model: the best rating, and the description stands in for the texts.
pub fn fallback(candidates: &[Candidate]) -> anyhow::Result<Choice> {
    let best = best_rated(candidates).context("no candidate to fall back to")?;
    let teaser = first_sentences(&best.overview, 2);
    Ok(Choice {
        tmdb_id: best.movie.tmdb_id.context("candidate without tmdb_id")?,
        reason: "chosen by rating".to_string(),
        teaser: if teaser.is_empty() {
            best.movie.title.clone()
        } else {
            teaser
        },
        article: if best.overview.is_empty() {
            best.movie.title.clone()
        } else {
            best.overview.clone()
        },
    })
}

fn first_sentences(text: &str, n: usize) -> String {
    let mut out = String::new();
    let mut count = 0;
    for ch in text.chars() {
        out.push(ch);
        if matches!(ch, '.' | '!' | '?') {
            count += 1;
            if count == n {
                break;
            }
        }
    }
    out.trim().to_string()
}
