CREATE TABLE users (
  id          TEXT PRIMARY KEY,
  handle      TEXT NOT NULL,
  avatar_hash TEXT,
  is_admin    INTEGER NOT NULL DEFAULT 0,
  created_at  INTEGER NOT NULL
);

CREATE TABLE sessions (
  id         TEXT PRIMARY KEY,
  user_id    TEXT NOT NULL REFERENCES users(id),
  created_at INTEGER NOT NULL,
  expires_at INTEGER NOT NULL
);

CREATE TABLE toys (
  id           TEXT PRIMARY KEY,
  author_id    TEXT NOT NULL REFERENCES users(id),
  title        TEXT NOT NULL,
  description  TEXT NOT NULL DEFAULT '',
  files_json   TEXT NOT NULL,
  state        TEXT NOT NULL DEFAULT 'draft',
  forked_from  TEXT REFERENCES toys(id),
  heart_count  INTEGER NOT NULL DEFAULT 0,
  clip         BLOB,
  thumb        BLOB,
  created_at   INTEGER NOT NULL,
  published_at INTEGER
);

CREATE TABLE toy_sources (
  toy_id       TEXT NOT NULL REFERENCES toys(id) ON DELETE CASCADE,
  name         TEXT NOT NULL,
  kind         TEXT NOT NULL,
  builtin_id   TEXT,
  options_json TEXT,
  payload      BLOB,
  meta_json    TEXT,
  PRIMARY KEY (toy_id, name)
);

CREATE TABLE toy_revisions (
  toy_id     TEXT NOT NULL REFERENCES toys(id) ON DELETE CASCADE,
  rev        INTEGER NOT NULL,
  files_json TEXT NOT NULL,
  saved_at   INTEGER NOT NULL,
  PRIMARY KEY (toy_id, rev)
);

CREATE TABLE hearts (
  user_id    TEXT NOT NULL REFERENCES users(id),
  toy_id     TEXT NOT NULL REFERENCES toys(id) ON DELETE CASCADE,
  created_at INTEGER NOT NULL,
  PRIMARY KEY (user_id, toy_id)
);

CREATE TRIGGER hearts_ai AFTER INSERT ON hearts BEGIN
  UPDATE toys SET heart_count = heart_count + 1 WHERE id = NEW.toy_id;
END;

CREATE TRIGGER hearts_ad AFTER DELETE ON hearts BEGIN
  UPDATE toys SET heart_count = heart_count - 1 WHERE id = OLD.toy_id;
END;

-- storage for ban-by-discord-id (moderation)
CREATE TABLE bans (
  discord_id TEXT PRIMARY KEY,
  created_at INTEGER NOT NULL
);

CREATE INDEX toys_state_created ON toys(state, created_at DESC);
CREATE INDEX toys_state_hearts  ON toys(state, heart_count DESC);
CREATE INDEX toys_author        ON toys(author_id);
