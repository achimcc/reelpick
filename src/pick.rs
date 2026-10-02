//! The daily run: read, filter, sample, ask, store. Idempotent per day.

use anyhow::{Context, bail};

use crate::config::Config;
use crate::jellyfin::JellyfinClient;
use crate::ollama::{Choice, OllamaClient};
use crate::select::{Candidate, Excluded, Thresholds, best_rated, eligible, sample};
use crate::store::{Pick, Store};
use crate::tmdb::{TmdbClient, enrich};

pub const EXHAUSTION_WINDOW: i64 = 20;

/// How often the poster of a new pick is asked for before the run stores the
/// pick without one, and how long it waits in between. A Jellyfin that is busy
/// with a library scan answers the image request late or not at all; the pick
/// itself has been made by then and must not be lost over a picture.
pub const POSTER_ATTEMPTS: u32 = 3;
pub const POSTER_PAUSE: std::time::Duration = std::time::Duration::from_secs(30);

/// How many of the newest picks without a poster every run tries again. A
/// pick that got none on its day (2026-10-02: the image request timed out
/// while Jellyfin removed fifteen thousand items) used to stay without one
/// for good, because the poster was fetched exactly once.
pub const BACKFILL_WINDOW: i64 = 7;

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

/// Fetch the poster of `item_id` and write it as the poster of `date`.
/// `false` when Jellyfin has none or did not deliver one in `attempts` tries.
async fn store_poster(
    config: &Config,
    jellyfin: &JellyfinClient,
    item_id: &str,
    date: &str,
    attempts: u32,
    pause: std::time::Duration,
) -> anyhow::Result<bool> {
    for attempt in 1..=attempts {
        match jellyfin.poster(item_id).await {
            Ok(Some(bytes)) => {
                let dir = config.data_dir.join("posters");
                tokio::fs::create_dir_all(&dir).await?;
                tokio::fs::write(dir.join(format!("{date}.jpg")), bytes).await?;
                return Ok(true);
            }
            Ok(None) => return Ok(false),
            Err(e) => {
                eprintln!(
                    "reelpick: no poster for {date} (attempt {attempt} of {attempts}): {e:#}"
                );
                if attempt < attempts {
                    tokio::time::sleep(pause).await;
                }
            }
        }
    }
    Ok(false)
}

/// Try once more for every recent pick that has no poster. Returns the dates
/// that have one now. Nothing here fails the run: a poster is decoration.
pub async fn backfill_posters(
    config: &Config,
    store: &Store,
    jellyfin: &JellyfinClient,
) -> Vec<String> {
    let missing = match store.picks_without_poster(BACKFILL_WINDOW).await {
        Ok(m) => m,
        Err(e) => {
            eprintln!("reelpick: cannot list the picks without a poster: {e:#}");
            return Vec::new();
        }
    };
    let mut filled = Vec::new();
    for p in missing {
        let stored = store_poster(
            config,
            jellyfin,
            &p.item_id,
            &p.date,
            1,
            std::time::Duration::ZERO,
        )
        .await;
        match stored {
            Ok(true) => match store.set_has_poster(&p.date).await {
                Ok(()) => {
                    eprintln!("reelpick: {} has its poster now", p.date);
                    filled.push(p.date);
                }
                Err(e) => eprintln!("reelpick: {e:#}"),
            },
            Ok(false) => {}
            Err(e) => eprintln!("reelpick: writing the poster for {}: {e:#}", p.date),
        }
    }
    filled
}

pub async fn run<R: rand::Rng>(
    config: &Config,
    store: &Store,
    clients: Clients<'_>,
    today: &str,
    rng: &mut R,
) -> anyhow::Result<Outcome> {
    run_with_pause(config, store, clients, today, rng, POSTER_PAUSE).await
}

/// `run`, with the pause between two poster attempts given — the tests do
/// not wait thirty seconds.
pub async fn run_with_pause<R: rand::Rng>(
    config: &Config,
    store: &Store,
    clients: Clients<'_>,
    today: &str,
    rng: &mut R,
    poster_pause: std::time::Duration,
) -> anyhow::Result<Outcome> {
    // Before anything else, and on every run: also on the second run of a day,
    // which otherwise does nothing.
    backfill_posters(config, store, clients.jellyfin).await;
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

    let has_poster = store_poster(
        config,
        clients.jellyfin,
        &chosen.movie.item_id,
        today,
        POSTER_ATTEMPTS,
        poster_pause,
    )
    .await?;

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
        imdb_rating: chosen.imdb_rating(),
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

/// The upper bound `ollama::validate` enforces on a model's teaser. The
/// fallback path bypasses `validate` entirely (there is no model answer to
/// check), so it must cap itself: an overview with no `.`/`!`/`?` would
/// otherwise hand back the whole, unbounded text.
const TEASER_MAX_CHARS: usize = 400;

/// No model: the best rating, and the description stands in for the texts.
pub fn fallback(candidates: &[Candidate]) -> anyhow::Result<Choice> {
    let best = best_rated(candidates).context("no candidate to fall back to")?;
    let teaser = cap_teaser(&first_sentences(&best.overview, 2), TEASER_MAX_CHARS);
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

/// `text` as-is when it already fits in `max_chars` (counting characters,
/// not bytes); otherwise cut at the last word boundary that still fits,
/// with an ellipsis added, so the result never exceeds `max_chars`.
fn cap_teaser(text: &str, max_chars: usize) -> String {
    if text.chars().count() <= max_chars {
        return text.to_string();
    }
    let budget = max_chars.saturating_sub(1); // room for the ellipsis
    let truncated: String = text.chars().take(budget).collect();
    let cut = match truncated.rfind(char::is_whitespace) {
        Some(i) => &truncated[..i],
        None => &truncated[..],
    };
    format!("{}…", cut.trim_end())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::jellyfin::Movie;

    fn movie(tmdb_id: i64) -> Movie {
        Movie {
            item_id: format!("item-{tmdb_id}"),
            title: format!("Film {tmdb_id}"),
            year: Some(2000),
            tmdb_id: Some(tmdb_id),
            genres: vec![],
            director: None,
            overview: None,
            runtime_min: None,
            community_rating: None,
        }
    }

    fn cand(tmdb_id: i64, overview: &str) -> Candidate {
        Candidate {
            movie: movie(tmdb_id),
            rating: Some(8.0),
            votes: Some(1000),
            overview: overview.to_string(),
        }
    }

    #[test]
    fn a_punctuation_free_overview_is_cut_at_400_chars_with_an_ellipsis() {
        let overview: String = "lorem ".repeat(200); // 1200 chars, no '.', '!' or '?'
        assert!(overview.chars().count() > 1000);
        let choice = fallback(&[cand(1, &overview)]).unwrap();
        let len = choice.teaser.chars().count();
        assert!(len <= TEASER_MAX_CHARS, "{len}");
        assert!(choice.teaser.ends_with('…'), "{}", choice.teaser);
        // The cut lands on a word boundary: no trailing partial "lore" before the ellipsis.
        assert!(choice.teaser.trim_end_matches('…').ends_with("lorem"));
    }

    #[test]
    fn a_normal_two_sentence_overview_is_left_unchanged() {
        let overview = "Two men on opposite sides of the law. One city that cannot hold both. \
                         A third sentence that must not appear.";
        let choice = fallback(&[cand(1, overview)]).unwrap();
        assert_eq!(
            choice.teaser,
            "Two men on opposite sides of the law. One city that cannot hold both."
        );
    }
}
