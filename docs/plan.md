# Plan

The task list this crate was built from, and what changed on the way. The
plan itself, and the per-task briefs and reviews, live in the operator's own
repository; this is the record kept alongside the code.

## Tasks

- [x] Task 1 — The skeleton: crate, AGPL-3.0, flake, CI, a binary that knows
      its version
- [x] Task 2 — Configuration: one TOML file, defaults, secrets from files
- [x] Task 3 — The store: SQLite migrations, picks, and a TMDB cache
- [x] Task 4 — The selection (pure functions): threshold, exclusion, sample
- [x] Task 5 — The Jellyfin client
- [x] Task 6 — TMDB and the enrichment step
- [x] Task 7 — The Ollama client
- [x] Task 8 — The `pick` flow
- [x] Task 9 — The pages
- [x] Task 10 — The program (CLI, wiring)
- [x] Task 11 — The NixOS module and the VM test
- [ ] Task 12 — Documentation: README, changelog, the one decision, the
      English design and plan record (this task). Publishing and tagging
      `v0.1.0` are the owner's own steps and are not part of it —
      owner: publish + tag v0.1.0 — pending

## Deviations from the plan

- **Task 2 — anyhow context.** The `.context()` call that used to wrap
  `toml::from_str` was removed so that serde's own error message (which
  names the offending field) stays visible in the error's `to_string()`
  instead of being replaced by a generic wrapper text.
- **Task 3 — `AssertSqlSafe`.** sqlx 0.9 requires internally built SQL
  fragments to be wrapped in `AssertSqlSafe` before they can be used with the
  query macros, even when — as here — the fragment never contains foreign
  input. Accepted as a library requirement, not a safety compromise.
- **Task 8 — a 400-character teaser cap.** The fallback teaser (used when no
  Ollama answer is available) was originally unbounded, taken verbatim from
  the plan. Ruling: cap it at 400 characters, cut at a word boundary with an
  ellipsis, because Ollama's own teasers are validated against a 20–400
  character range and the fragment shown on the embedding page should honor
  the same bound regardless of which path produced it.
- **Task 9 — three related routing/rendering deviations:**
  - Routes are registered with full paths built via `format!("{base}/…")`
    instead of `Router::nest`, because Axum 0.8 changed how `nest` treats a
    trailing slash, and the intended history route (`<base_path>/`) needs
    the older behavior.
  - comrak's unsafe-HTML-rendering option is named `unsafe` in its API,
    which is a Rust keyword — it is referenced as the raw identifier
    `r#unsafe`.
  - `axum::response::Redirect` only offers constructors for 303, 307, and
    308; the `/today` route needs a plain 302 Found, so that response is
    built by hand instead.
- **Task 11 — `systemctl cat` in the VM test.** The test needs to see that
  the Jellyfin key reaches the pick unit as a `LoadCredential`, not as a
  plaintext environment variable. `systemctl show -p LoadCredential` redacts
  the value as `[unprintable]` on the systemd version in the test image;
  `systemctl cat` prints the actual unit file, so the test reads the
  credential spec from there instead.

## Deferred minors

Accepted as out of scope for this crate's first release; each is a
documented gap, not an unnoticed one.

- `Config::load` names the file path only for an I/O error, not for invalid
  TOML content (`src/config.rs`).
- `sample_size == 0` and a `base_path` without a leading slash are validated
  but untested (`src/config.rs`).
- `oldest_picks` treats a negative `n` as "no limit"; this precondition is
  not documented (`src/store.rs`).
- The "nothing found" branches — `pick_for` on a date with no row, an empty
  store — are untested.
- A `NaN` rating silently fails to clear the threshold, since any comparison
  against `NaN` is `false`; this was mandated by the plan as written
  (`src/select.rs`).
- No test covers a Jellyfin item missing `ProviderIds`, `Genres`, `People`,
  or `RunTimeTicks` — structurally handled via `serde`'s defaults and
  `Option`, but not exercised by a test.
- No regression test asserts that a TMDB decode-error message never contains
  the API key; the protection currently rests on the internals of
  `reqwest` 0.12.28 (`src/tmdb.rs`).
- The branches for a candidate with `tmdb_id == None` and for "the TMDB
  client fails while a stale cache entry exists" are untested; there is also
  a redundant `.clone()` in `from_entry`.
- `OllamaClient::choose` called with an empty candidate list still makes two
  round trips (up to 2×600s) before giving up, instead of failing
  immediately; the prompt's `unwrap_or(0)` silently relies on the invariant
  that `eligible` never hands it a candidate without a TMDB ID.
- `candidates.clone()` runs before `eligible` even on the normal (non-empty)
  path; two concurrent `pick` runs on the same day would make the second one
  fail with a generic database error (a `UNIQUE` violation) rather than the
  more informative `AlreadyPicked` outcome — accepted, since the timer that
  drives `pick` runs it sequentially.
- No test covers `base_path == ""` (the root-mount case); this is currently
  backed only by code inspection. `Arc<State>` is unnecessary overhead given
  how the state is used. The `href` construction in `pages.rs` is repeated
  in three places rather than factored out.
- `StateDirectoryMode` is not set on the systemd units, so
  `/var/lib/reelpick` ends up mode `0755` rather than the `0700` the plan's
  comment assumed — systemd's `UMask` setting does not apply to
  `StateDirectory` (`nix/module.nix`).
