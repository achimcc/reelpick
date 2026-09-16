# Changelog

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
