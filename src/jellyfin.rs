//! Jellyfin: the library, the films in it, their posters.

use anyhow::{Context, bail};
use serde::Deserialize;

#[derive(Debug, Clone, PartialEq)]
pub struct Movie {
    pub item_id: String,
    pub title: String,
    pub year: Option<i64>,
    pub tmdb_id: Option<i64>,
    pub genres: Vec<String>,
    pub director: Option<String>,
    pub overview: Option<String>,
    pub runtime_min: Option<i64>,
    pub community_rating: Option<f64>,
}

pub struct JellyfinClient {
    base: String,
    auth: String,
    http: reqwest::Client,
}

#[derive(Deserialize)]
struct Page<T> {
    #[serde(rename = "Items")]
    items: Vec<T>,
}

#[derive(Deserialize)]
struct Folder {
    #[serde(rename = "Name")]
    name: String,
    #[serde(rename = "Id")]
    id: String,
}

#[derive(Deserialize)]
struct Item {
    #[serde(rename = "Name")]
    name: String,
    #[serde(rename = "Id")]
    id: String,
    #[serde(rename = "ProductionYear")]
    year: Option<i64>,
    #[serde(rename = "ProviderIds", default)]
    provider_ids: std::collections::HashMap<String, String>,
    #[serde(rename = "Genres", default)]
    genres: Vec<String>,
    #[serde(rename = "CommunityRating")]
    community_rating: Option<f64>,
    #[serde(rename = "Overview")]
    overview: Option<String>,
    #[serde(rename = "RunTimeTicks")]
    run_time_ticks: Option<i64>,
    #[serde(rename = "People", default)]
    people: Vec<Person>,
}

#[derive(Deserialize)]
struct Person {
    #[serde(rename = "Name")]
    name: String,
    #[serde(rename = "Type")]
    kind: String,
}

const TICKS_PER_MINUTE: i64 = 600_000_000;

impl JellyfinClient {
    pub fn new(base_url: &str, api_key: &str) -> anyhow::Result<JellyfinClient> {
        Ok(JellyfinClient {
            base: base_url.trim_end_matches('/').to_string(),
            auth: format!("MediaBrowser Token=\"{api_key}\""),
            http: reqwest::Client::builder()
                .timeout(std::time::Duration::from_secs(60))
                .build()?,
        })
    }

    async fn get_json<T: serde::de::DeserializeOwned>(
        &self,
        path: &str,
        query: &[(&str, &str)],
    ) -> anyhow::Result<T> {
        let response = self
            .http
            .get(format!("{}{}", self.base, path))
            .query(query)
            .header("Authorization", &self.auth)
            .send()
            .await
            .with_context(|| format!("Jellyfin GET {path}"))?;
        let status = response.status();
        if !status.is_success() {
            bail!("Jellyfin GET {path} answered {status}");
        }
        response
            .json()
            .await
            .with_context(|| format!("Jellyfin GET {path}: unexpected body"))
    }

    pub async fn library_id(&self, name: &str) -> anyhow::Result<String> {
        let page: Page<Folder> = self.get_json("/Library/MediaFolders", &[]).await?;
        page.items
            .into_iter()
            .find(|f| f.name == name)
            .map(|f| f.id)
            .with_context(|| format!("Jellyfin has no library named {name:?}"))
    }

    pub async fn movies(&self, library_id: &str) -> anyhow::Result<Vec<Movie>> {
        let page: Page<Item> = self
            .get_json(
                "/Items",
                &[
                    ("ParentId", library_id),
                    ("IncludeItemTypes", "Movie"),
                    ("Recursive", "true"),
                    ("Fields", "ProviderIds,Overview,Genres,People,CommunityRating,ProductionYear,RunTimeTicks"),
                ],
            )
            .await?;
        Ok(page.items.into_iter().map(Movie::from).collect())
    }

    /// `None` when Jellyfin has no image for the item — a film without a
    /// poster is still a film.
    pub async fn poster(&self, item_id: &str) -> anyhow::Result<Option<Vec<u8>>> {
        let path = format!("/Items/{item_id}/Images/Primary");
        let response = self
            .http
            .get(format!("{}{}", self.base, path))
            .query(&[("maxWidth", "600"), ("quality", "85")])
            .header("Authorization", &self.auth)
            .send()
            .await
            .with_context(|| format!("Jellyfin GET {path}"))?;
        if response.status() == reqwest::StatusCode::NOT_FOUND {
            return Ok(None);
        }
        if !response.status().is_success() {
            bail!("Jellyfin GET {path} answered {}", response.status());
        }
        Ok(Some(response.bytes().await?.to_vec()))
    }
}

impl From<Item> for Movie {
    fn from(i: Item) -> Movie {
        Movie {
            tmdb_id: i.provider_ids.get("Tmdb").and_then(|s| s.parse().ok()),
            director: i
                .people
                .iter()
                .find(|p| p.kind == "Director")
                .map(|p| p.name.clone()),
            runtime_min: i.run_time_ticks.map(|t| t / TICKS_PER_MINUTE),
            item_id: i.id,
            title: i.name,
            year: i.year,
            genres: i.genres,
            overview: i.overview,
            community_rating: i.community_rating,
        }
    }
}
