ALTER TABLE peloton_class ADD COLUMN not_served_at TEXT;

DROP INDEX peloton_class_unread;

CREATE INDEX peloton_class_unread
    ON peloton_class (aired_at DESC)
 WHERE detail IS NULL AND not_served_at IS NULL;
