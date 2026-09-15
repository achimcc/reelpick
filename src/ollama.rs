//! The model: five candidates in, one choice with three texts out — and a
//! schema that makes inventing a sixth film impossible.

use anyhow::{Context, bail};
use serde::Deserialize;
use serde_json::json;

use crate::select::Candidate;

#[derive(Debug, Clone, PartialEq)]
pub struct Choice {
    pub tmdb_id: i64,
    pub reason: String,
    pub teaser: String,
    pub article: String,
}

pub struct OllamaClient {
    url: String,
    model: String,
    http: reqwest::Client,
}

#[derive(Deserialize)]
struct ChatResponse {
    message: Message,
}

#[derive(Deserialize)]
struct Message {
    content: String,
}

#[derive(Deserialize)]
struct RawChoice {
    choice: i64,
    reason: String,
    teaser: String,
    article: String,
}

const TEASER_CHARS: std::ops::RangeInclusive<usize> = 20..=400;
const ARTICLE_WORDS: std::ops::RangeInclusive<usize> = 100..=700;

pub fn schema(candidates: &[Candidate]) -> serde_json::Value {
    let ids: Vec<i64> = candidates.iter().filter_map(|c| c.movie.tmdb_id).collect();
    json!({
        "type": "object",
        "properties": {
            "choice": {"type": "integer", "enum": ids},
            "reason": {"type": "string"},
            "teaser": {"type": "string"},
            "article": {"type": "string"}
        },
        "required": ["choice", "reason", "teaser", "article"]
    })
}

pub fn prompt(candidates: &[Candidate], language: &str) -> String {
    let mut p = String::new();
    p.push_str("Here are the candidates for today's film recommendation, chosen at random from a private library. Pick the one that makes the best evening for a mixed circle of friends and family, and write about it.\n\n");
    for c in candidates {
        let m = &c.movie;
        p.push_str(&format!(
            "- tmdb_id {}: \"{}\" ({}), directed by {}, genres: {}, runtime {} min, rating {} from {} votes.\n  Synopsis: {}\n",
            m.tmdb_id.unwrap_or(0),
            m.title,
            m.year.map(|y| y.to_string()).unwrap_or_else(|| "year unknown".into()),
            m.director.as_deref().unwrap_or("unknown"),
            if m.genres.is_empty() { "unknown".to_string() } else { m.genres.join(", ") },
            m.runtime_min.map(|r| r.to_string()).unwrap_or_else(|| "?".into()),
            c.rating.map(|r| format!("{r:.1}")).unwrap_or_else(|| "?".into()),
            c.votes.map(|v| v.to_string()).unwrap_or_else(|| "?".into()),
            if c.overview.is_empty() { "(none)" } else { &c.overview },
        ));
    }
    p.push_str(&format!(
        "\nAnswer as JSON with the fields choice (one of the tmdb_id values above), reason, teaser and article.\n\
         Write reason, teaser and article in the language with code \"{language}\".\n\
         reason: one paragraph, why this film over the other candidates, today.\n\
         teaser: exactly two sentences that make someone want to watch it, no spoiler, do not quote the rating.\n\
         article: 200 to 400 words in Markdown, headings no larger than ###, no spoiler beyond the first act, \
         no facts that are not in the synopsis or the fields above — do not invent actors, awards or plot points.\n"
    ));
    p
}

pub fn validate(raw: &str, candidates: &[Candidate]) -> anyhow::Result<Choice> {
    let r: RawChoice =
        serde_json::from_str(raw).context("the model did not answer with the expected JSON")?;
    if !candidates.iter().any(|c| c.movie.tmdb_id == Some(r.choice)) {
        bail!(
            "the model chose tmdb_id {}, which is not a candidate",
            r.choice
        );
    }
    let teaser_len = r.teaser.trim().chars().count();
    if !TEASER_CHARS.contains(&teaser_len) {
        bail!("teaser has {teaser_len} characters, wanted {TEASER_CHARS:?}");
    }
    let words = r.article.split_whitespace().count();
    if !ARTICLE_WORDS.contains(&words) {
        bail!("article has {words} words, wanted {ARTICLE_WORDS:?}");
    }
    if r.reason.trim().is_empty() {
        bail!("reason is empty");
    }
    Ok(Choice {
        tmdb_id: r.choice,
        reason: r.reason.trim().into(),
        teaser: r.teaser.trim().into(),
        article: r.article.trim().into(),
    })
}

impl OllamaClient {
    pub fn new(url: &str, model: &str) -> anyhow::Result<OllamaClient> {
        Ok(OllamaClient {
            url: url.trim_end_matches('/').to_string(),
            model: model.to_string(),
            http: reqwest::Client::builder()
                .timeout(std::time::Duration::from_secs(600))
                .build()?,
        })
    }

    pub fn model(&self) -> &str {
        &self.model
    }

    async fn ask(&self, candidates: &[Candidate], language: &str) -> anyhow::Result<String> {
        let body = json!({
            "model": self.model,
            "stream": false,
            "think": false,
            "messages": [
                {"role": "system", "content": "You are a film curator writing for friends. You only know what the user tells you."},
                {"role": "user", "content": prompt(candidates, language)}
            ],
            "format": schema(candidates),
            "options": {"temperature": 0.7},
            "keep_alive": "5m"
        });
        let response = self
            .http
            .post(format!("{}/api/chat", self.url))
            .json(&body)
            .send()
            .await
            .context("Ollama POST /api/chat")?;
        let status = response.status();
        if !status.is_success() {
            bail!("Ollama answered {status}");
        }
        let chat: ChatResponse = response.json().await.context("Ollama: unexpected body")?;
        Ok(chat.message.content)
    }

    /// One try, one retry, then the caller falls back. Two, because a second
    /// sample of the same model is cheap and a third rarely differs.
    pub async fn choose(&self, candidates: &[Candidate], language: &str) -> anyhow::Result<Choice> {
        let mut last = None;
        for attempt in 1..=2 {
            match self
                .ask(candidates, language)
                .await
                .and_then(|raw| validate(&raw, candidates))
            {
                Ok(c) => return Ok(c),
                Err(e) => {
                    eprintln!("reelpick: model attempt {attempt} failed: {e:#}");
                    last = Some(e);
                }
            }
        }
        Err(last.expect("two attempts were made"))
    }
}
