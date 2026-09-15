# reelpick

reelpick picks one film a day from a Jellyfin library and writes something
about it. TMDB supplies the rating and vote count that filter the
candidates (cached, and entirely optional), and a local Ollama model picks
the film from a random shortlist and writes a short teaser and a longer
article about it. It serves the result as a handful of read-only pages: a
fragment meant for embedding elsewhere, one article per day, and a history
of every past pick.

## How it picks

Run once a day (`reelpick pick`), idempotently — a second run on the same
day does nothing:

1. Read the configured Jellyfin library; movies without a TMDB ID are
   dropped, since there is neither a rating nor a vote count for them.
2. Exclude films already picked before (by TMDB ID and by Jellyfin item ID).
3. Look up rating, vote count, and overview on TMDB, cached for 30 days.
   Without TMDB (no key, or TMDB unreachable), Jellyfin's own community
   rating is used instead, against the rating threshold only.
4. If nothing is left, the oldest picks become eligible again (everything
   has had its day); if that is still empty, `pick` fails with an error.
5. Draw a uniformly random sample (`sample_size` candidates, no weighting
   by rating).
6. Ask Ollama to pick one from the sample and write a `reason`, a `teaser`,
   and an `article`, constrained to a JSON schema so it cannot invent a
   film; one retry on a bad answer, then fall back to the best-rated
   candidate with the TMDB overview standing in for the texts.
7. Copy the poster into the local data directory (never served from a
   foreign host), and save everything in one transaction.

`reelpick serve` only ever reads what `pick` wrote; it makes no outbound
call of its own at runtime. If Jellyfin, TMDB, or Ollama is unreachable when
`pick` runs, `serve` keeps showing the last successful pick.

See [`docs/design.md`](docs/design.md) for the fuller design summary, and
[`docs/decisions/0001-no-login.md`](docs/decisions/0001-no-login.md) for why
there is no login (see also the note below).

## No login

reelpick has **no authentication of its own**. Every page shows the same
thing to everyone who reaches it — there is no per-user data, no role, and
no endpoint that changes anything at runtime. It is meant to sit behind a
reverse proxy that already authenticates (forward-auth, HTTP basic auth, or
similar); do not expose it directly to an untrusted network.

## Configuration

One TOML file, its path given by the `REELPICK_CONFIG` environment
variable. Secrets are read from files named in the configuration, never
from environment variables.

| Key | Default | Meaning |
|---|---|---|
| `listen` | `"127.0.0.1:8080"` | address `serve` binds to |
| `base_path` | `""` | path prefix for every route (e.g. `"/reelpick"`); a trailing slash is stripped; `""` serves from the root |
| `data_dir` | `"/var/lib/reelpick"` | where the SQLite database and posters live |
| `language` | `"en"` | language code passed to Ollama for the generated texts |
| `timezone` | `"UTC"` | IANA time zone name; determines what "today" means |
| `home_url` | `"/"` | link back to the site the pages are embedded into |
| `sample_size` | `5` | how many candidates are offered to the model each day |
| `[jellyfin] url` | *(required)* | Jellyfin base URL, used server-side |
| `[jellyfin] public_url` | *(required)* | Jellyfin base URL used for the browser-facing "view in Jellyfin" link |
| `[jellyfin] library` | *(required)* | the library's name, as shown in Jellyfin |
| `[jellyfin] api_key_file` | `None` | path to a file holding the Jellyfin API key; required in practice, since `pick` cannot read the library without it |
| `[tmdb] api_key_file` | `None` | path to a file holding the TMDB v3 API key; without it, ratings fall back to Jellyfin's own |
| `[tmdb] min_rating` | `6.5` | minimum rating a candidate must clear |
| `[tmdb] min_votes` | `200` | minimum vote count required (TMDB only; ignored on the Jellyfin fallback) |
| `[ollama] url` | *(required)* | Ollama base URL |
| `[ollama] model` | *(required)* | the model name to ask, e.g. `"qwen3:8b"` |

Example:

```toml
listen    = "0.0.0.0:8080"
base_path = "/reelpick"
data_dir  = "/var/lib/reelpick"
language  = "en"
timezone  = "Europe/Berlin"
home_url  = "https://example.org/"
sample_size = 5

[jellyfin]
url          = "http://jellyfin.internal:8096"
public_url   = "https://jellyfin.example.org"
library      = "Movies"
api_key_file = "/run/credentials/reelpick-pick.service/jellyfin"

[tmdb]
api_key_file = "/run/credentials/reelpick-pick.service/tmdb"   # optional
min_rating   = 6.5
min_votes    = 200

[ollama]
url   = "http://ollama.internal:11434"
model = "qwen3:8b"
```

## Routes

All routes live under `base_path`.

| Route | Response |
|---|---|
| `GET <base>/` | the history: every past pick, newest first |
| `GET <base>/today.html` | the embeddable fragment: the latest pick, as a link with poster, title, year, teaser, and date (not necessarily *today's* pick — if `pick` failed, this shows yesterday's, dated) |
| `GET <base>/today` | `302 Found` to the latest article |
| `GET <base>/<date>` | the article for that date (`YYYY-MM-DD`): poster, title, year, director, genres, runtime, rating and vote count, the reason, the rendered article, a link into Jellyfin, and links to the previous and next day |
| `GET <base>/healthz` | `200` with the latest pick's date, or `503 no picks yet` |
| `GET <base>/style.css` | the embedded stylesheet |
| `GET <base>/poster/<date>.jpg` | the stored poster for that date, cached for a day |

## NixOS module

`nixosModules.default` provides `services.reelpick`:

| Option | Default | Meaning |
|---|---|---|
| `enable` | `false` | enables the service |
| `package` | *(required)* | the reelpick package to run |
| `settings` | `{ }` | the configuration above, as a Nix attribute set (freeform TOML); `data_dir` and both `api_key_file` entries are set by the module itself |
| `jellyfinApiKeyFile` | *(required)* | where the Jellyfin API key comes from — an absolute path, or a bare systemd credential name |
| `tmdbApiKeyFile` | `null` | same, for the TMDB key; without it, ratings come from Jellyfin |
| `pickTime` | `"05:00"` | when the daily pick runs, local time, as a systemd `OnCalendar` hour:minute |

Example, adapted from the module's own VM test (`nix/test.nix`):

```nix
{
  services.reelpick = {
    enable = true;
    package = reelpick.packages.${pkgs.system}.default;
    jellyfinApiKeyFile = "/run/secrets/jellyfin-api-key";
    tmdbApiKeyFile = "/run/secrets/tmdb-api-key"; # optional
    pickTime = "05:00";
    settings = {
      base_path = "/reelpick";
      language  = "en";
      timezone  = "Europe/Berlin";
      jellyfin = {
        url        = "http://jellyfin.internal:8096";
        public_url = "https://jellyfin.example.org";
        library    = "Movies";
      };
      ollama = {
        url   = "http://ollama.internal:11434";
        model = "qwen3:8b";
      };
    };
  };
}
```

This creates `reelpick.service` (the server, `wantedBy multi-user.target`),
`reelpick-pick.service` (the oneshot, with the API keys loaded as systemd
credentials), and `reelpick-pick.timer` (fires `reelpick-pick.service`
daily at `pickTime`, `Persistent = true`). Both services run as the same
static `reelpick` user, since they share the data directory and its SQLite
database.

## Embedding the fragment

`GET <base>/today.html` returns a self-contained HTML fragment (no
`<html>`, no stylesheet) meant to be included into another page. One way to
do that, with Caddy's `templates` directive:

```
handle /reelpick/* {
    reverse_proxy reelpick:8080
}

handle {
    root * /srv/site
    templates
    file_server
}
```

and, at the point in the page where the pick should appear:

```
{{httpInclude "/reelpick/today.html"}}
```

Caddy's `httpInclude` forwards the original request's headers to the
sub-request, so a session cookie checked by an authenticating proxy in front
of the site reaches reelpick's fragment route as well. If reelpick does not
answer, the include renders as empty text rather than breaking the page —
keep any heading or intro text for that section outside the `httpInclude`
call so the page still makes sense when the fragment is empty.

## Development

```
cargo test          # unit and integration tests
nix flake check      # package build, and (on Linux) the NixOS VM test
```

A `devShell` is provided (`nix develop`) with `cargo`, `rustc`, `rustfmt`,
`clippy`, `rust-analyzer`, and `sqlite`.

## License

AGPL-3.0-only.
