# Design

This is an English summary of the design this crate was built from. It
covers what the application is for, the stack, the selection flow, the
routes, the configuration, and the failure modes. It does not translate the
source document word for word — the full German derivation, including the
site-specific reasoning (why this and not an existing blog, the guest and
network placement, the embedding into the front page), lives in the
operator's own repository: `docs/2026-09-15-reelpick-design.md`.

## 1. What this is

A different film from a Jellyfin library, picked once a day, shown as a
short teaser that links to a longer article; a history page lists every past
pick. The choice is not left to chance alone: TMDB (when configured) filters
candidates by rating and vote count, and a local Ollama model picks one from
a random shortlist and writes the teaser and the article itself.

There is nothing personal about a pick — no per-user recommendation, no
watch history, no rating by the reader. The same pick is shown to everyone,
which is the point: it is meant to be a shared, small piece of daily
conversation, not a recommendation engine.

## 4. The stack, and the two subcommands

One binary, two subcommands:

| Command | What it does | How it runs |
|---|---|---|
| `reelpick pick` | picks today's film and writes it to the database | a systemd oneshot from a daily timer, `Persistent=true` |
| `reelpick serve` | renders the fragment, the article, and the history from the database | a long-running service; it calls no other service at runtime |

The split is the fault tolerance: if Jellyfin, TMDB, or Ollama is down,
`pick` fails loudly and `serve` keeps showing the last pick. The pages
depend on nothing but SQLite.

`pick` is idempotent per day: if today's date already has a row, it does
nothing and exits successfully, so the timer may fire any number of times
and a restart of the service never produces a second pick for the same day.

Crate layout, one responsibility per module: `config` (TOML plus secret
files), `jellyfin` (a client for three endpoints), `tmdb` (client plus a
30-day cache), `ollama` (chat with a JSON schema), `select` (pure filtering
and sampling, no I/O), `store` (SQLite), `web` (routes and templates), and
`pick` (the flow that wires the others together).

## 6. The selection — `pick`, step by step

1. **Read the library.** The configured library name is resolved to an ID;
   an unknown name is an error, not an empty pool. Movies are then read with
   their provider IDs, overview, genres, people, rating, year, and runtime.
2. **Movies without a TMDB ID are dropped** — without one there is neither a
   rating nor a vote count, and Jellyfin's own identification is usually
   unreliable for such films anyway.
3. **Already-picked films are excluded**, by both TMDB ID and Jellyfin item
   ID — a film re-added to Jellyfin gets a new item ID but is the same film.
4. **TMDB is queried** for rating, vote count, overview, and runtime.
   Results are cached for 30 days; only expired or missing entries are
   fetched. The threshold (default `min_rating = 6.5`, `min_votes = 200`)
   comes from the configuration.
5. **Fallback without TMDB:** if TMDB does not answer, or no key is
   configured, Jellyfin's own community rating is compared against
   `min_rating` alone, with no vote threshold. This is logged as a warning;
   the pick still happens.
6. **Exhaustion:** if nothing is left after filtering, every film has had
   its day — the oldest picks become candidates again, filtered the same
   way. If that pool is also empty (an empty library, or nothing clears the
   threshold), `pick` fails with an error that says so.
7. **A random sample** (`sample_size`, default 5) is drawn uniformly, with
   no weighting by rating — the threshold has already done the sorting, and
   a 6.6 should have the same chance as an 8.4.
8. **Ollama** receives the shortlist (title, year, genres, director,
   runtime, overview, rating, vote count) and the configured language, and
   is asked to answer as JSON against a schema with exactly four fields:
   `choice` (a TMDB ID, constrained to the candidates — the model cannot
   invent a film), `reason` (one paragraph), `teaser` (two spoiler-free
   sentences, no rating number), and `article` (200 to 400 words of
   Markdown, no heading larger than `###`, no fact beyond what was given).
   The answer is checked against the schema, the candidate list, and length
   bounds (teaser 20–400 characters, article 100–700 words). A failure gets
   one retry, and then falls back.
9. **Fallback without Ollama:** the best-rated candidate is chosen; the
   teaser is the first two sentences of the TMDB overview (capped at 400
   characters at a word boundary), the article is the whole overview,
   `reason` is `"chosen by rating"`, and the row records
   `generated_by = "fallback"`. The pages show no visible difference; only
   the history in the database says so to whoever looks.
10. **The poster** is copied from Jellyfin into the data directory. If it is
    missing, the row simply has no poster. No image is ever loaded from a
    foreign host on the served pages — posters are always served from the
    application's own data directory.
11. **Everything is saved** in one transaction.

"Today" is the date in the configured time zone. A NixOS deployment would
typically run the timer early in the local morning, so the pick is ready
before anyone wakes up and does not change under a reader who watches a film
around midnight.

## 7. The pages

All routes live under `base_path`, so the application can be served from its
own host or from any path. See the README for the exact route table. In
short: a fragment meant for embedding elsewhere, a history of every past
pick, one article page per date with a link back into Jellyfin, a redirect
to today's article, the stored poster, and a health check. No JavaScript
anywhere.

The Jellyfin link on an article page points at
`<jellyfin.public_url>/web/index.html#/details?id=<item_id>`. If the film
later disappears from Jellyfin, the article stays and the link leads to
Jellyfin's own "not found" — the history is a log, not an inventory.

## 9. Configuration

One TOML file, referenced by the `REELPICK_CONFIG` environment variable.
Secrets are read from files named in the configuration, never from
environment variables — see the README for the full key table and defaults.

Only `pick` ever needs a secret. `serve` reads none, is given none, and
could not do anything with one if it had it.

The TMDB v3 key is passed as an `api_key` query parameter, because that is
the only form TMDB accepts. It therefore appears in the outgoing request URL
— but never in anything reelpick logs: the client logs host and path, never
the query string.

Losing the Jellyfin key makes `pick` abort — without a library there is
nothing to choose from. Losing the TMDB key or Ollama only degrades the
pick, with a warning; every failure other than Jellyfin makes the pick worse
but never makes it disappear.

## 11. Failure modes

| Failure | `pick` | `serve` | Embedding page |
|---|---|---|---|
| Ollama unreachable | falls back to the best rating and TMDB's text, `generated_by = fallback` | unaffected | unaffected |
| TMDB unreachable | falls back to the cache, then to Jellyfin's rating; warning logged | unaffected | unaffected |
| Jellyfin unreachable | fails loudly (the unit is reported failed) | shows the last pick | shows the last pick, with its date |
| reelpick itself unreachable | — | — | the embedded section is simply empty; the rest of the page is intact |
| Candidate pool empty | falls back to the exhaustion rule; if that is also empty, an error naming the cause | shows the last pick | shows the last pick |
| Model answers outside the schema | one retry, then the fallback | unaffected | unaffected |
