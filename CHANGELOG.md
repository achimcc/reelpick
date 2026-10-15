# Changelog

## 0.2.5 — 2026-10-15

The pick no longer depends on Jellyfin being idle. `GET /Items` over the
whole library asked for `People` on every film, to print one director; that
single field turned a 0.07 s request into 19 s on a quiet server and into
a timeout whenever a library scan, trickplay or tag-cache run held
Jellyfin's database — which on the server this was written for happened at
every one of the run's three attempts for five nights in a row. The library
is now read without `People`, and the directors of the five shortlisted
films come in one small request afterwards; when even that fails, the pick
goes on without the name. `[jellyfin] timeout_secs` (default `60`) makes the
request timeout a setting instead of a constant.

The NixOS module's `pickTime` also takes a list. The pick is idempotent per
day, so a second slot later in the morning only does something when the
first failed — or when a restart of the machine stopped the chain of
retries, which `Persistent` does not catch up on (2026-10-15: three
attempts into Jellyfin's busy hours, then a deploy at 04:32 restarted the
container, and the day had no pick).

## 0.2.4 — 2026-10-14

Dependency updates only, no change in behaviour: rustls 0.23.46
(RUSTSEC-2026-0346 to 0349), quinn-proto 0.11.19 (RUSTSEC-2026-0338 to
0343; compiled in through reqwest, never used) and ammonia 4.2.3
(RUSTSEC-2021-0157 and 0158; `style` attributes and SVG animation tags,
which the default allowlist never lets through). Found by the homeserver's
daily `cargo audit bin` over the deployed binaries; the pinned advisory
database moves to 2026-10-14.

## 0.2.3 — 2026-10-03

Baseline hardening from the homeserver's security audit 3 (B149), no change in behaviour: `unsafe` code denied crate-wide, flake checks `audit` (cargo-audit against a pinned advisory database) and `deny` (cargo-deny: bans, sources, licenses), `SECURITY.md`, Renovate.

## 0.2.2 — 2026-10-02

A pick no longer stays without its poster because Jellyfin was busy for a
minute. The poster used to be fetched exactly once, when the pick was made;
on 2026-10-02 that request timed out while Jellyfin removed fifteen thousand
items, and the pick of the day was shown without a cover for good. Now the
run asks up to three times, thirty seconds apart, and every run — also the
second one of a day, which otherwise does nothing — tries once more for the
seven newest picks that have none. A film Jellyfin has no image for (404)
stays without, quietly. None of this can fail a run.

## 0.2.1 — 2026-09-27

The brace escaping of the fragment moves from the HTTP route into
`pages::fragment` itself, the one function that writes that markup (audit 3,
B69). Caddy's `httpInclude` parses the included body as a Go template, and
title and teaser come from Jellyfin, TMDB's community-edited overview and a
model's answer; the route already escaped every `{` and `}`, but a caller of
the public function got raw braces. Now nothing that calls it can forget:
no `{` or `}` leaves it (text and attributes, as `&#123;` / `&#125;`), and
since maud escapes every `<` and `>` from the data, no `<!--{{` / `}}-->`
either, for an operator who moves Caddy's delimiters there. New tests plant
`{{env ...}}`, `{{printf ...}}`, `}}-->` and `<!--{{` in the title, the
model's teaser and the fallback's overview and check the fragment.

## 0.2.0 — 2026-09-20

The review names the film's IMDb rating, and the audience is friends rather
than a family. The rating comes from Jellyfin's `CommunityRating`, which is
IMDb's own number wherever the IMDb Ratings plugin refreshes it; it is the
only rating the model is shown and the only one the article page prints,
because a model that can see two ratings quotes the wrong one sooner or
later. TMDB's rating and vote count keep doing what they always did — decide
who gets onto the shortlist. The prompt now writes for an evening among
grown-up friends: no film is passed over for being unfit for children, and
none is preferred for being hard. Picks made before this release have no
IMDb rating stored and keep showing the TMDB line they were written with.

## 0.1.1 — 2026-09-16

The history and the article now wear the same clothes as the page that
embeds the fragment: one stylesheet with that page's palette and its two
font stacks, a head with a prompt line, a block cursor and a way back, and
bracketed section headings. Every pick in the history is the same compact
card the fragment renders — poster, title, teaser, date — because both come
out of one function now. The words around the film follow `language`
instead of being English regardless: German or English chrome, month names,
and ratings and vote counts in the separators of the language.

## 0.1.0 — 2026-09-15

The first release. One film a day: Jellyfin supplies the library, TMDB the
rating and vote count (cached, optional), an Ollama model picks from a random
shortlist and writes a teaser and an article. Pages under a base path, a
fragment for embedding, a history. NixOS module with a timer, VM-tested.
