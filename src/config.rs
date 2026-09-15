//! The configuration: one TOML file, secrets read from the files it names.

use std::path::{Path, PathBuf};

use anyhow::{Context, bail};
use jiff::tz::TimeZone;
use serde::Deserialize;

#[derive(Debug, Clone)]
pub struct Config {
    pub listen: String,
    pub base_path: String,
    pub data_dir: PathBuf,
    pub language: String,
    pub timezone: TimeZone,
    pub home_url: String,
    pub sample_size: usize,
    pub jellyfin: Jellyfin,
    pub tmdb: Tmdb,
    pub ollama: Ollama,
}

#[derive(Debug, Clone, Deserialize)]
pub struct Jellyfin {
    pub url: String,
    pub public_url: String,
    pub library: String,
    pub api_key_file: Option<PathBuf>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct Tmdb {
    pub api_key_file: Option<PathBuf>,
    #[serde(default = "default_min_rating")]
    pub min_rating: f64,
    #[serde(default = "default_min_votes")]
    pub min_votes: i64,
}

#[derive(Debug, Clone, Deserialize)]
pub struct Ollama {
    pub url: String,
    pub model: String,
}

#[derive(Deserialize)]
struct Raw {
    #[serde(default = "default_listen")]
    listen: String,
    #[serde(default)]
    base_path: String,
    #[serde(default = "default_data_dir")]
    data_dir: PathBuf,
    #[serde(default = "default_language")]
    language: String,
    #[serde(default = "default_timezone")]
    timezone: String,
    #[serde(default = "default_home_url")]
    home_url: String,
    #[serde(default = "default_sample_size")]
    sample_size: usize,
    jellyfin: Jellyfin,
    #[serde(default = "default_tmdb")]
    tmdb: Tmdb,
    ollama: Ollama,
}

fn default_listen() -> String {
    "127.0.0.1:8080".into()
}
fn default_data_dir() -> PathBuf {
    "/var/lib/reelpick".into()
}
fn default_language() -> String {
    "en".into()
}
fn default_timezone() -> String {
    "UTC".into()
}
fn default_home_url() -> String {
    "/".into()
}
fn default_sample_size() -> usize {
    5
}
fn default_min_rating() -> f64 {
    6.5
}
fn default_min_votes() -> i64 {
    200
}
fn default_tmdb() -> Tmdb {
    Tmdb {
        api_key_file: None,
        min_rating: default_min_rating(),
        min_votes: default_min_votes(),
    }
}

impl Config {
    pub fn from_toml(text: &str) -> anyhow::Result<Config> {
        let raw: Raw = toml::from_str(text)?;
        let timezone = TimeZone::get(&raw.timezone)
            .with_context(|| format!("unknown timezone {:?}", raw.timezone))?;
        if raw.sample_size == 0 {
            bail!("sample_size must be at least 1");
        }
        let base_path = raw.base_path.trim_end_matches('/').to_string();
        if !base_path.is_empty() && !base_path.starts_with('/') {
            bail!("base_path must start with a slash, got {base_path:?}");
        }
        Ok(Config {
            listen: raw.listen,
            base_path,
            data_dir: raw.data_dir,
            language: raw.language,
            timezone,
            home_url: raw.home_url,
            sample_size: raw.sample_size,
            jellyfin: raw.jellyfin,
            tmdb: raw.tmdb,
            ollama: raw.ollama,
        })
    }

    pub fn load(path: &Path) -> anyhow::Result<Config> {
        let text =
            std::fs::read_to_string(path).with_context(|| format!("reading {}", path.display()))?;
        Config::from_toml(&text)
    }

    /// A route under the base path. `tail` starts with a slash.
    pub fn route(&self, tail: &str) -> String {
        format!("{}{}", self.base_path, tail)
    }

    pub fn date_at(&self, at: jiff::Timestamp) -> jiff::civil::Date {
        at.to_zoned(self.timezone.clone()).date()
    }

    pub fn today(&self) -> jiff::civil::Date {
        self.date_at(jiff::Timestamp::now())
    }
}

/// Reads a secret from a file, without the trailing newline every editor adds.
pub fn read_secret(path: &Path) -> anyhow::Result<String> {
    let text = std::fs::read_to_string(path)
        .with_context(|| format!("reading the secret at {}", path.display()))?;
    let trimmed = text.trim();
    if trimmed.is_empty() {
        bail!("the secret at {} is empty", path.display());
    }
    Ok(trimmed.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    const MINIMAL: &str = r#"
        [jellyfin]
        url = "http://jelly:8096"
        public_url = "https://jellyfin.example.org"
        library = "Movies"
        api_key_file = "/run/credentials/reelpick-pick.service/jellyfin"
        [ollama]
        url = "http://llm:11434"
        model = "qwen3:8b"
    "#;

    #[test]
    fn defaults_fill_what_the_file_leaves_out() {
        let c = Config::from_toml(MINIMAL).unwrap();
        assert_eq!(c.listen, "127.0.0.1:8080");
        assert_eq!(c.base_path, "");
        assert_eq!(c.language, "en");
        assert_eq!(c.sample_size, 5);
        assert_eq!(c.tmdb.min_rating, 6.5);
        assert_eq!(c.tmdb.min_votes, 200);
        assert!(c.tmdb.api_key_file.is_none());
        assert_eq!(c.timezone.iana_name(), Some("UTC"));
    }

    #[test]
    fn base_path_loses_its_trailing_slash_and_routes_join_cleanly() {
        let text = format!("base_path = \"/reelpick/\"\n{MINIMAL}");
        let c = Config::from_toml(&text).unwrap();
        assert_eq!(c.base_path, "/reelpick");
        assert_eq!(c.route("/today.html"), "/reelpick/today.html");
        let root = Config::from_toml(MINIMAL).unwrap();
        assert_eq!(root.route("/today.html"), "/today.html");
    }

    #[test]
    fn an_unknown_timezone_is_an_error_not_utc() {
        let text = format!("timezone = \"Mars/Olympus\"\n{MINIMAL}");
        let err = Config::from_toml(&text).unwrap_err().to_string();
        assert!(err.contains("Mars/Olympus"), "{err}");
    }

    #[test]
    fn berlin_is_resolved_and_today_uses_it() {
        let text = format!("timezone = \"Europe/Berlin\"\n{MINIMAL}");
        let c = Config::from_toml(&text).unwrap();
        assert_eq!(c.timezone.iana_name(), Some("Europe/Berlin"));
        // 2026-09-15 23:30 UTC is already the 16th in Berlin.
        let at = jiff::Timestamp::from_second(1_789_515_000).unwrap(); // 2026-09-15T23:30:00Z
        assert_eq!(c.date_at(at).to_string(), "2026-09-16");
    }

    #[test]
    fn a_missing_jellyfin_section_is_an_error() {
        let err = Config::from_toml("[ollama]\nurl='x'\nmodel='m'\n")
            .unwrap_err()
            .to_string();
        assert!(err.contains("jellyfin"), "{err}");
    }

    #[test]
    fn read_secret_trims_the_newline_and_names_the_file_on_failure() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("key");
        std::fs::write(&path, "s3cret\n").unwrap();
        assert_eq!(read_secret(&path).unwrap(), "s3cret");
        let missing = dir.path().join("nope");
        let err = read_secret(&missing).unwrap_err().to_string();
        assert!(err.contains("nope"), "{err}");
    }
}
