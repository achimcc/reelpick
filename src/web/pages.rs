//! The three views, as Maud. Every link goes through `Config::route`, so the
//! base path is in one place.

use maud::{DOCTYPE, Markup, PreEscaped, html};

use crate::config::Config;
use crate::store::Pick;

pub fn fragment(config: &Config, pick: Option<&Pick>) -> Markup {
    match pick {
        None => html! {
            p.reelpick-empty { "No pick yet." }
        },
        Some(p) => html! {
            a.reelpick-field href=(config.route(&format!("/{}", p.date))) {
                @if p.has_poster {
                    img.reelpick-poster src=(config.route(&format!("/poster/{}.jpg", p.date))) alt="" loading="lazy";
                }
                div.reelpick-text {
                    h3.reelpick-title { (p.title) @if let Some(y) = p.year { " (" (y) ")" } }
                    p.reelpick-teaser { (p.teaser) }
                    p.reelpick-date { time datetime=(p.date) { (p.date) } }
                }
            }
            p.reelpick-more { a href=(config.route("/")) { "All picks so far" } }
        },
    }
}

fn frame(config: &Config, title: &str, body: Markup) -> Markup {
    html! {
        (DOCTYPE)
        html lang=(config.language) {
            head {
                meta charset="utf-8";
                meta name="viewport" content="width=device-width, initial-scale=1";
                title { (title) }
                link rel="stylesheet" href=(config.route("/style.css"));
            }
            body {
                main { (body) }
            }
        }
    }
}

pub fn article(
    config: &Config,
    p: &Pick,
    article_html: &str,
    prev: Option<&str>,
    next: Option<&str>,
) -> Markup {
    let jellyfin = format!(
        "{}/web/index.html#/details?id={}",
        config.jellyfin.public_url.trim_end_matches('/'),
        p.item_id
    );
    frame(
        config,
        &p.title,
        html! {
            p.reelpick-crumbs { a href=(config.home_url) { "Home" } " · " a href=(config.route("/")) { "All picks" } }
            header.reelpick-head {
                @if p.has_poster { img src=(config.route(&format!("/poster/{}.jpg", p.date))) alt=""; }
                div {
                    h1 { (p.title) @if let Some(y) = p.year { " (" (y) ")" } }
                    p.reelpick-meta {
                        @if let Some(d) = &p.director { "Directed by " (d) " · " }
                        @if !p.genres.is_empty() { (p.genres.join(", ")) " · " }
                        @if let Some(r) = p.runtime_min { (r) " min · " }
                        @if let Some(r) = p.rating {
                            (format!("{r:.1}")) "/10"
                            @if let Some(v) = p.votes { " from " (v) " votes" }
                        }
                    }
                    p.reelpick-meta { "Pick of " time datetime=(p.date) { (p.date) } }
                    p { a.reelpick-button href=(jellyfin) { "Watch in Jellyfin" } }
                }
            }
            p.reelpick-reason { (p.reason) }
            section.reelpick-article { (PreEscaped(article_html)) }
            nav.reelpick-nav {
                span { @if let Some(d) = prev { a href=(config.route(&format!("/{d}"))) { "← " (d) } } }
                span { @if let Some(d) = next { a href=(config.route(&format!("/{d}"))) { (d) " →" } } }
            }
        },
    )
}

pub fn history(config: &Config, picks: &[Pick]) -> Markup {
    // Grouped before the template, in the order the picks arrive (newest first).
    let mut months: Vec<(String, Vec<&Pick>)> = Vec::new();
    for p in picks {
        let month = p.date.get(..7).unwrap_or("").to_string();
        match months.last_mut() {
            Some((last, list)) if *last == month => list.push(p),
            _ => months.push((month, vec![p])),
        }
    }
    frame(
        config,
        "All picks",
        html! {
            p.reelpick-crumbs { a href=(config.home_url) { "Home" } }
            h1 { "All picks" }
            div.reelpick-history {
                @for (month, list) in &months {
                    h2 { (month) }
                    ul {
                        @for p in list {
                            li {
                                time datetime=(p.date) { (p.date) }
                                a href=(config.route(&format!("/{}", p.date))) { (p.title) @if let Some(y) = p.year { " (" (y) ")" } }
                            }
                        }
                    }
                }
            }
        },
    )
}
