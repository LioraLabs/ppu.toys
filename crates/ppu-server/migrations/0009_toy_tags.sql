ALTER TABLE toys ADD COLUMN tags_json TEXT NOT NULL DEFAULT '[]'
    CHECK(json_valid(tags_json) AND json_type(tags_json) = 'array');
