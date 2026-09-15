//! SQLite: the picks and a cache of what TMDB said.

use std::path::Path;
use std::str::FromStr;

use anyhow::Context;
use sqlx::sqlite::{SqliteConnectOptions, SqliteJournalMode, SqlitePoolOptions};
use sqlx::{Row, SqlitePool};

use crate::select::Excluded;

#[derive(Debug, Clone, PartialEq)]
pub struct Pick {
    pub date: String,
    pub item_id: String,
    pub tmdb_id: i64,
    pub title: String,
    pub year: Option<i64>,
    pub runtime_min: Option<i64>,
    pub genres: Vec<String>,
    pub director: Option<String>,
    pub rating: Option<f64>,
    pub votes: Option<i64>,
    pub reason: String,
    pub teaser: String,
    pub article_md: String,
    pub generated_by: String,
    pub model: Option<String>,
    pub has_poster: bool,
}

#[derive(Debug, Clone, PartialEq)]
pub struct TmdbEntry {
    pub tmdb_id: i64,
    pub rating: Option<f64>,
    pub votes: Option<i64>,
    pub overview: Option<String>,
    pub runtime: Option<i64>,
}

#[derive(Clone)]
pub struct Store {
    pool: SqlitePool,
}

const PICK_COLUMNS: &str = "date, item_id, tmdb_id, title, year, runtime_min, genres, director, \
    rating, votes, reason, teaser, article_md, generated_by, model, has_poster";

impl Store {
    pub async fn open(path: &Path) -> anyhow::Result<Store> {
        let options = SqliteConnectOptions::from_str(&format!("sqlite://{}", path.display()))
            .with_context(|| format!("database path {}", path.display()))?
            .create_if_missing(true)
            .journal_mode(SqliteJournalMode::Wal)
            .busy_timeout(std::time::Duration::from_secs(5));
        let pool = SqlitePoolOptions::new()
            .max_connections(4)
            .connect_with(options)
            .await?;
        sqlx::migrate!("./migrations")
            .run(&pool)
            .await
            .context("migrating")?;
        Ok(Store { pool })
    }

    pub async fn insert_pick(&self, p: &Pick) -> anyhow::Result<()> {
        let genres = serde_json::to_string(&p.genres)?;
        sqlx::query(sqlx::AssertSqlSafe(format!(
            "INSERT INTO picks ({PICK_COLUMNS}, created_at) \
             VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)"
        )))
        .bind(&p.date)
        .bind(&p.item_id)
        .bind(p.tmdb_id)
        .bind(&p.title)
        .bind(p.year)
        .bind(p.runtime_min)
        .bind(genres)
        .bind(&p.director)
        .bind(p.rating)
        .bind(p.votes)
        .bind(&p.reason)
        .bind(&p.teaser)
        .bind(&p.article_md)
        .bind(&p.generated_by)
        .bind(&p.model)
        .bind(p.has_poster as i64)
        .bind(jiff::Timestamp::now().to_string())
        .execute(&self.pool)
        .await
        .context("inserting the pick")?;
        Ok(())
    }

    fn row_to_pick(row: &sqlx::sqlite::SqliteRow) -> anyhow::Result<Pick> {
        let genres: String = row.try_get("genres")?;
        Ok(Pick {
            date: row.try_get("date")?,
            item_id: row.try_get("item_id")?,
            tmdb_id: row.try_get("tmdb_id")?,
            title: row.try_get("title")?,
            year: row.try_get("year")?,
            runtime_min: row.try_get("runtime_min")?,
            genres: serde_json::from_str(&genres)?,
            director: row.try_get("director")?,
            rating: row.try_get("rating")?,
            votes: row.try_get("votes")?,
            reason: row.try_get("reason")?,
            teaser: row.try_get("teaser")?,
            article_md: row.try_get("article_md")?,
            generated_by: row.try_get("generated_by")?,
            model: row.try_get("model")?,
            has_poster: row.try_get::<i64, _>("has_poster")? != 0,
        })
    }

    async fn picks_where(&self, tail: &str, bind: Option<&str>) -> anyhow::Result<Vec<Pick>> {
        let sql = format!("SELECT {PICK_COLUMNS} FROM picks {tail}");
        let mut q = sqlx::query(sqlx::AssertSqlSafe(sql));
        if let Some(b) = bind {
            q = q.bind(b);
        }
        let rows = q.fetch_all(&self.pool).await?;
        rows.iter().map(Self::row_to_pick).collect()
    }

    pub async fn pick_for(&self, date: &str) -> anyhow::Result<Option<Pick>> {
        Ok(self.picks_where("WHERE date = ?", Some(date)).await?.pop())
    }

    pub async fn latest_pick(&self) -> anyhow::Result<Option<Pick>> {
        Ok(self
            .picks_where("ORDER BY date DESC LIMIT 1", None)
            .await?
            .pop())
    }

    pub async fn all_picks(&self) -> anyhow::Result<Vec<Pick>> {
        self.picks_where("ORDER BY date DESC", None).await
    }

    pub async fn oldest_picks(&self, n: i64) -> anyhow::Result<Vec<Pick>> {
        self.picks_where(&format!("ORDER BY date ASC LIMIT {n}"), None)
            .await
    }

    pub async fn excluded(&self) -> anyhow::Result<Excluded> {
        let rows = sqlx::query("SELECT tmdb_id, item_id FROM picks")
            .fetch_all(&self.pool)
            .await?;
        let mut ex = Excluded::default();
        for r in rows {
            ex.tmdb_ids.insert(r.try_get("tmdb_id")?);
            ex.item_ids.insert(r.try_get("item_id")?);
        }
        Ok(ex)
    }

    pub async fn neighbours(&self, date: &str) -> anyhow::Result<(Option<String>, Option<String>)> {
        let prev: Option<String> =
            sqlx::query_scalar("SELECT date FROM picks WHERE date < ? ORDER BY date DESC LIMIT 1")
                .bind(date)
                .fetch_optional(&self.pool)
                .await?;
        let next: Option<String> =
            sqlx::query_scalar("SELECT date FROM picks WHERE date > ? ORDER BY date ASC LIMIT 1")
                .bind(date)
                .fetch_optional(&self.pool)
                .await?;
        Ok((prev, next))
    }

    fn row_to_entry(row: &sqlx::sqlite::SqliteRow) -> anyhow::Result<TmdbEntry> {
        Ok(TmdbEntry {
            tmdb_id: row.try_get("tmdb_id")?,
            rating: row.try_get("rating")?,
            votes: row.try_get("votes")?,
            overview: row.try_get("overview")?,
            runtime: row.try_get("runtime")?,
        })
    }

    /// The cache entry if it is younger than `max_age_days` at `now` (a date).
    pub async fn cached_tmdb(
        &self,
        tmdb_id: i64,
        max_age_days: i64,
        now: &str,
    ) -> anyhow::Result<Option<TmdbEntry>> {
        let row = sqlx::query(
            "SELECT tmdb_id, rating, votes, overview, runtime FROM tmdb_cache \
             WHERE tmdb_id = ? AND julianday(?) - julianday(fetched_at) <= ?",
        )
        .bind(tmdb_id)
        .bind(now)
        .bind(max_age_days)
        .fetch_optional(&self.pool)
        .await?;
        row.as_ref().map(Self::row_to_entry).transpose()
    }

    /// The cache entry regardless of age — for the day TMDB does not answer.
    pub async fn stale_tmdb(&self, tmdb_id: i64) -> anyhow::Result<Option<TmdbEntry>> {
        let row = sqlx::query(
            "SELECT tmdb_id, rating, votes, overview, runtime FROM tmdb_cache WHERE tmdb_id = ?",
        )
        .bind(tmdb_id)
        .fetch_optional(&self.pool)
        .await?;
        row.as_ref().map(Self::row_to_entry).transpose()
    }

    pub async fn put_tmdb(&self, e: &TmdbEntry, now: &str) -> anyhow::Result<()> {
        sqlx::query(
            "INSERT INTO tmdb_cache (tmdb_id, rating, votes, overview, runtime, fetched_at) \
             VALUES (?, ?, ?, ?, ?, ?) ON CONFLICT(tmdb_id) DO UPDATE SET \
             rating = excluded.rating, votes = excluded.votes, overview = excluded.overview, \
             runtime = excluded.runtime, fetched_at = excluded.fetched_at",
        )
        .bind(e.tmdb_id)
        .bind(e.rating)
        .bind(e.votes)
        .bind(&e.overview)
        .bind(e.runtime)
        .bind(now)
        .execute(&self.pool)
        .await?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn pick(date: &str, tmdb_id: i64, item: &str) -> Pick {
        Pick {
            date: date.into(),
            item_id: item.into(),
            tmdb_id,
            title: format!("Film {tmdb_id}"),
            year: Some(1999),
            runtime_min: Some(120),
            genres: vec!["Drama".into()],
            director: Some("Someone".into()),
            rating: Some(7.5),
            votes: Some(1000),
            reason: "because".into(),
            teaser: "Two sentences.".into(),
            article_md: "# Hi\n\nText.".into(),
            generated_by: "ollama".into(),
            model: Some("qwen3:8b".into()),
            has_poster: false,
        }
    }

    async fn store() -> (tempfile::TempDir, Store) {
        let dir = tempfile::tempdir().unwrap();
        let s = Store::open(&dir.path().join("reelpick.db")).await.unwrap();
        (dir, s)
    }

    #[tokio::test]
    async fn a_pick_round_trips_and_a_second_for_the_same_day_is_refused() {
        let (_d, s) = store().await;
        s.insert_pick(&pick("2026-09-15", 1, "a")).await.unwrap();
        let back = s.pick_for("2026-09-15").await.unwrap().unwrap();
        assert_eq!(back.title, "Film 1");
        assert_eq!(back.genres, vec!["Drama".to_string()]);
        assert!(s.insert_pick(&pick("2026-09-15", 2, "b")).await.is_err());
    }

    #[tokio::test]
    async fn latest_all_oldest_and_neighbours_agree_on_the_order() {
        let (_d, s) = store().await;
        for (d, id) in [("2026-09-13", 1), ("2026-09-15", 3), ("2026-09-14", 2)] {
            s.insert_pick(&pick(d, id, &id.to_string())).await.unwrap();
        }
        assert_eq!(s.latest_pick().await.unwrap().unwrap().date, "2026-09-15");
        let all: Vec<_> = s
            .all_picks()
            .await
            .unwrap()
            .into_iter()
            .map(|p| p.date)
            .collect();
        assert_eq!(all, ["2026-09-15", "2026-09-14", "2026-09-13"]);
        let old: Vec<_> = s
            .oldest_picks(2)
            .await
            .unwrap()
            .into_iter()
            .map(|p| p.tmdb_id)
            .collect();
        assert_eq!(old, [1, 2]);
        assert_eq!(
            s.neighbours("2026-09-14").await.unwrap(),
            (Some("2026-09-13".into()), Some("2026-09-15".into()))
        );
        assert_eq!(
            s.neighbours("2026-09-15").await.unwrap(),
            (Some("2026-09-14".into()), None)
        );
    }

    #[tokio::test]
    async fn excluded_collects_both_ids() {
        let (_d, s) = store().await;
        s.insert_pick(&pick("2026-09-15", 7, "item-7"))
            .await
            .unwrap();
        let ex = s.excluded().await.unwrap();
        assert!(ex.tmdb_ids.contains(&7));
        assert!(ex.item_ids.contains("item-7"));
    }

    #[tokio::test]
    async fn tmdb_cache_expires_but_stays_readable_as_stale() {
        let (_d, s) = store().await;
        let e = TmdbEntry {
            tmdb_id: 5,
            rating: Some(8.0),
            votes: Some(300),
            overview: Some("x".into()),
            runtime: Some(90),
        };
        s.put_tmdb(&e, "2026-08-01").await.unwrap();
        assert!(s.cached_tmdb(5, 30, "2026-08-20").await.unwrap().is_some());
        assert!(s.cached_tmdb(5, 30, "2026-09-15").await.unwrap().is_none());
        assert_eq!(s.stale_tmdb(5).await.unwrap().unwrap().votes, Some(300));
        assert!(s.stale_tmdb(6).await.unwrap().is_none());
    }
}
