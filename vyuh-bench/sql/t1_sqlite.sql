CREATE TABLE IF NOT EXISTS items (
  id INTEGER PRIMARY KEY,
  name TEXT NOT NULL
);

INSERT OR IGNORE INTO items (id, name) VALUES
  (1, 'alpha'),
  (2, 'beta'),
  (3, 'gamma');
