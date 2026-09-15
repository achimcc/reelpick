//! TMDB: rating, vote count and a description in the reader's language —
//! cached, because the library barely changes and TMDB is a neighbour.

use std::sync::Arc;

use anyhow::{Context, bail};
use serde::Deserialize;
use tokio::sync::Semaphore;

use crate::jellyfin::Movie;
use crate::select::Candidate;
use crate::store::{Store, TmdbEntry};

pub const CACHE_DAYS: i64 = 30;
const PARALLEL: usize = 4;

pub struct TmdbClient {
    api_key: String,
    language: String,
    base: String,
    http: reqwest::Client,
}

#[derive(Deserialize)]
struct Body {
    vote_average: Option<f64>,
    vote_count: Option<i64>,
    overview: Option<String>,
    runtime: Option<i64>,
}

impl TmdbClient {
    pub fn new(api_key: &str, language: &str) -> anyhow::Result<TmdbClient> {
        Ok(TmdbClient {
            api_key: api_key.to_string(),
            language: language.to_string(),
            base: "https://api.themoviedb.org".to_string(),
            http: reqwest::Client::builder()
                .timeout(std::time::Duration::from_secs(30))
                .build()?,
        })
    }

    pub fn with_base(mut self, base_url: &str) -> TmdbClient {
        self.base = base_url.trim_end_matches('/').to_string();
        self
    }

    pub async fn movie(&self, tmdb_id: i64) -> anyhow::Result<TmdbEntry> {
        // The key travels in the query, as TMDB v3 demands. It is never part
        // of an error message: only the path is named.
        let path = format!("/3/movie/{tmdb_id}");
        let response = self
            .http
            .get(format!("{}{}", self.base, path))
            .query(&[
                ("api_key", self.api_key.as_str()),
                ("language", self.language.as_str()),
            ])
            .send()
            .await
            .map_err(|e| anyhow::anyhow!("TMDB GET {path}: {}", e.without_url()))?;
        let status = response.status();
        if !status.is_success() {
            bail!("TMDB GET {path} answered {status}");
        }
        let body: Body = response
            .json()
            .await
            .with_context(|| format!("TMDB GET {path}: unexpected body"))?;
        Ok(TmdbEntry {
            tmdb_id,
            rating: body.vote_average,
            votes: body.vote_count,
            overview: body.overview.filter(|o| !o.trim().is_empty()),
            runtime: body.runtime,
        })
    }
}

/// Turns films into candidates: fresh cache, else TMDB, else stale cache,
/// else what Jellyfin knows. Every step down is a warning, none is an error.
pub async fn enrich(
    store: &Store,
    tmdb: Option<&TmdbClient>,
    movies: Vec<Movie>,
    today: &str,
) -> anyhow::Result<Vec<Candidate>> {
    let limit = Arc::new(Semaphore::new(PARALLEL));
    let mut tasks = Vec::with_capacity(movies.len());
    for movie in movies {
        let store = store.clone();
        let limit = limit.clone();
        let today = today.to_string();
        tasks.push(async move {
            let Some(id) = movie.tmdb_id else {
                return Ok::<Candidate, anyhow::Error>(from_jellyfin(movie));
            };
            if let Some(e) = store.cached_tmdb(id, CACHE_DAYS, &today).await? {
                return Ok(from_entry(movie, e));
            }
            if let Some(client) = tmdb {
                let _permit = limit.acquire().await.expect("semaphore open");
                match client.movie(id).await {
                    Ok(e) => {
                        store.put_tmdb(&e, &today).await?;
                        return Ok(from_entry(movie, e));
                    }
                    Err(err) => eprintln!("reelpick: {err:#}; using what is cached or known"),
                }
            }
            if let Some(e) = store.stale_tmdb(id).await? {
                return Ok(from_entry(movie, e));
            }
            Ok(from_jellyfin(movie))
        });
    }
    let mut out = Vec::with_capacity(tasks.len());
    for t in tasks {
        out.push(t.await?);
    }
    Ok(out)
}

fn from_entry(movie: Movie, e: TmdbEntry) -> Candidate {
    let overview = e
        .overview
        .clone()
        .or_else(|| movie.overview.clone())
        .unwrap_or_default();
    Candidate {
        rating: e.rating,
        votes: e.votes,
        overview,
        movie,
    }
}

fn from_jellyfin(movie: Movie) -> Candidate {
    let overview = movie.overview.clone().unwrap_or_default();
    Candidate {
        rating: movie.community_rating,
        votes: None,
        overview,
        movie,
    }
}
