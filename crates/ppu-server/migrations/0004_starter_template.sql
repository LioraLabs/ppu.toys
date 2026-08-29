CREATE TABLE settings (
  key   TEXT PRIMARY KEY,
  value TEXT NOT NULL
);

INSERT INTO settings(key, value) VALUES (
  'starter_template',
  '{"name":"untitled toy","files":[{"name":"main.lua","source":"function frame(t, f)\n  apply_pokes()\n  brightness = 15\n  cgram[0] = rgb(80 + 60 * math.sin(t), 40, 140)\nend\n"}]}'
);
