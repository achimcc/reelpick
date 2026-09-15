CREATE TABLE picks (
  date         TEXT PRIMARY KEY,
  item_id      TEXT NOT NULL,
  tmdb_id      INTEGER NOT NULL,
  title        TEXT NOT NULL,
  year         INTEGER,
  runtime_min  INTEGER,
  genres       TEXT NOT NULL,
  director     TEXT,
  rating       REAL,
  votes        INTEGER,
  reason       TEXT NOT NULL,
  teaser       TEXT NOT NULL,
  article_md   TEXT NOT NULL,
  generated_by TEXT NOT NULL,
  model        TEXT,
  created_at   TEXT NOT NULL,
  has_poster   INTEGER NOT NULL DEFAULT 0
);
CREATE INDEX picks_tmdb ON picks (tmdb_id);
CREATE INDEX picks_item ON picks (item_id);

CREATE TABLE tmdb_cache (
  tmdb_id    INTEGER PRIMARY KEY,
  rating     REAL,
  votes      INTEGER,
  overview   TEXT,
  runtime    INTEGER,
  fetched_at TEXT NOT NULL
);
