# Changelog

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
