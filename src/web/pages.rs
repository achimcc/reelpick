//! The three views, as Maud. Every link goes through `Config::route`, so the
//! base path is in one place, and every word around the film comes from
//! `Strings`, so the page speaks `config.language`.
//!
//! The history and the article wear the clothes of the page that embeds the
//! fragment (`style.css`, design document section 7): the same head with a
//! prompt line and a block cursor, the same bracketed section headings, and —
//! this is the point of `card` below — literally the same card markup for a
//! pick in both places.

use maud::{DOCTYPE, Markup, PreEscaped, html};

use crate::config::Config;
use crate::store::Pick;
use crate::web::strings::{Strings, host_of};

/// One pick, as the compact card the front page already knows how to draw.
///
/// THE FRAGMENT AND THE HISTORY CALL THE SAME FUNCTION, and that is what keeps
/// them identical: the embedding page styles `reelpick-field`, `-poster`,
/// `-text`, `-title`, `-teaser` and `-date` itself, this application ships the
/// same rules in `style.css`, and neither can drift from the other as long as
/// there is one place that writes the markup.
///
/// Whatever this renders can end up inside a Caddy template (the fragment
/// route escapes every brace afterwards), so nothing here may emit `{` or `}`.
pub fn card(config: &Config, p: &Pick) -> Markup {
    html! {
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
    }
}

pub fn fragment(config: &Config, pick: Option<&Pick>) -> Markup {
    let s = Strings::for_language(&config.language);
    match pick {
        None => html! {
            p.reelpick-empty { (s.no_pick) }
        },
        Some(p) => html! {
            (card(config, p))
            p.reelpick-more { a href=(config.route("/")) { (s.all_picks_so_far) } }
        },
    }
}

/// The page around a body: head with the prompt line, the name and the cursor,
/// the crumbs back out of here, and a foot that says what happens tomorrow.
fn frame(
    config: &Config,
    title: &str,
    prompt_cmd: &str,
    lead: &str,
    crumbs: Markup,
    body: Markup,
) -> Markup {
    let s = Strings::for_language(&config.language);
    html! {
        (DOCTYPE)
        html lang=(config.language) {
            head {
                meta charset="utf-8";
                meta name="viewport" content="width=device-width, initial-scale=1";
                title { (title) " — reelpick" }
                link rel="stylesheet" href=(config.route("/style.css"));
            }
            body {
                header.kopf {
                    div.bahn {
                        p.reelpick-crumbs { (crumbs) }
                        p.prompt { b { "root@" (host_of(&config.home_url)) } ":~# reelpick " (prompt_cmd) }
                        h1 { "REELPICK" span.kursor aria-hidden="true" {} }
                        p.vorwort { (lead) }
                    }
                }
                main { div.bahn { (body) } }
                footer {
                    div.bahn {
                        p.prompt { b { "$" } " reelpick pick --tomorrow" }
                        p { (s.footer_note) }
                    }
                }
            }
        }
    }
}

/// `← Startseite`, the one link every page here owes the page that sent the
/// reader in.
fn crumb_home(config: &Config, s: &Strings) -> Markup {
    html! { a href=(config.home_url) { "← " (s.home) } }
}

pub fn article(
    config: &Config,
    p: &Pick,
    article_html: &str,
    prev: Option<&str>,
    next: Option<&str>,
) -> Markup {
    let s = Strings::for_language(&config.language);
    let jellyfin = format!(
        "{}/web/index.html#/details?id={}",
        config.jellyfin.public_url.trim_end_matches('/'),
        p.item_id
    );
    // Built as a list and joined, so a missing field never leaves a dangling
    // separator behind: half of these are `Option` on a good day.
    let mut facts: Vec<String> = Vec::new();
    if let Some(d) = &p.director {
        facts.push(format!("{}{}", s.director, d));
    }
    if !p.genres.is_empty() {
        facts.push(p.genres.join(", "));
    }
    if let Some(minutes) = p.runtime_min {
        facts.push(format!("{} {}", s.integer(minutes), s.minutes));
    }
    if let Some(rating) = p.rating {
        let mut line = format!("{}/10", s.decimal(rating));
        if let Some(votes) = p.votes {
            line.push_str(&format!(
                " {}{}{}",
                s.votes_before,
                s.integer(votes),
                s.votes_after
            ));
        }
        facts.push(line);
    }
    let facts = facts.join(" · ");
    frame(
        config,
        &p.title,
        &format!("pick --date {}", p.date),
        &p.teaser,
        html! {
            (crumb_home(config, &s))
            " · "
            a href=(config.route("/")) { (s.all_picks) }
        },
        html! {
            header.reelpick-head {
                @if p.has_poster { img src=(config.route(&format!("/poster/{}.jpg", p.date))) alt=""; }
                div {
                    h1 { (p.title) @if let Some(y) = p.year { " (" (y) ")" } }
                    @if !facts.is_empty() { p.reelpick-meta { (facts) } }
                    p.reelpick-meta { (s.pick_of) time datetime=(p.date) { (p.date) } }
                    p { a.reelpick-button href=(jellyfin) { (s.watch) } }
                }
            }
            p.reelpick-reason { (p.reason) }
            section.reelpick-article { (PreEscaped(article_html)) }
            nav.reelpick-nav {
                span {
                    @if let Some(d) = prev {
                        a href=(config.route(&format!("/{d}"))) { "← " (s.previous) " · " (d) }
                    }
                }
                span {
                    @if let Some(d) = next {
                        a href=(config.route(&format!("/{d}"))) { (s.next) " · " (d) " →" }
                    }
                }
            }
        },
    )
}

pub fn history(config: &Config, picks: &[Pick]) -> Markup {
    let s = Strings::for_language(&config.language);
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
        s.all_picks,
        "picks --all",
        s.history_lead,
        crumb_home(config, &s),
        html! {
            div.abschnitt-kopf {
                h2 { (s.all_picks_heading) }
                p { (s.picks_so_far(picks.len())) }
            }
            @if picks.is_empty() {
                p.reelpick-empty { (s.no_pick) }
            } @else {
                @for (month, list) in &months {
                    div.abschnitt-kopf { h2 { (s.month_heading(month)) } }
                    div.reelpick-grid {
                        @for p in list { (card(config, p)) }
                    }
                }
            }
        },
    )
}
