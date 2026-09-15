# 1. No login of its own

## Status

Accepted.

## Context

The operator's site policy requires every application behind the forward-auth
proxy to keep a login of its own, as defense in depth against a fail-open of
that proxy (an incident on the operator's infrastructure is the reason the
rule exists). reelpick does not have one. That is a deliberate exception, not
an oversight.

## Decision

reelpick ships with no authentication or authorization of its own.

## Rationale

- **There is nothing to authorize.** Every page shows the same thing to
  everyone who reaches it. There is no role, no form, no write path, and no
  per-user data — the service is a read-only view onto a table that a timer
  fills once a day.
- **The page it sits inside has none either.** The site's front page — a
  static file behind the same forward-auth — carries no login of its own for
  the same reason. reelpick is embedded into that page, under its path, and
  inherits the same posture rather than adding a second, inconsistent one.
- **What a fail-open would expose is not a secret.** If the proxy in front of
  it ever let a stranger through, they would see the title of a film the
  library owns and a text a model wrote about it. That does not justify a
  second gate.

## What the application deliberately does not do, so the decision holds

- It never hands a Jellyfin (or any other) API key to the browser.
- It never loads an image from a foreign host — posters are copied into the
  application's own data directory and served from there.
- It exposes no endpoint that changes state. The only path that could ever
  be considered a write is the `pick` subcommand, and that is a command run
  by a timer, not a route a client can reach.

## Consequences

reelpick must keep being read-only and free of user data for this decision
to remain sound. If a future version adds anything per-user — a rating, a
watchlist, a comment — this decision needs to be revisited before that
feature ships, not after.

It also means reelpick must always run behind something that does
authenticate (a reverse proxy with forward-auth, or an equivalent). It is not
safe to expose on its own to an untrusted network.

## See also

The German original of this reasoning, including the site-wide policy it is
an exception to, lives in the operator's own repository:
`docs/2026-09-15-reelpick-design.md`, section 5.
