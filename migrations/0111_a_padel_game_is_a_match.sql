ALTER TABLE padel_game RENAME TO padel_match;

DROP INDEX padel_game_by_start;

CREATE INDEX padel_match_by_start ON padel_match (started_at_utc);
