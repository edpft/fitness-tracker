-- no-transaction
PRAGMA foreign_keys = OFF;

CREATE TABLE training_slot_rebuilt (
    pattern    INTEGER NOT NULL REFERENCES training_pattern(id) ON DELETE CASCADE,
    weekday    TEXT    NOT NULL
        CHECK (weekday IN ('monday', 'tuesday', 'wednesday', 'thursday',
                           'friday', 'saturday', 'sunday')),
    part       TEXT    NOT NULL
        CHECK (part IN ('morning', 'afternoon', 'evening')),
    discipline TEXT    NOT NULL CHECK (discipline IN ('gym', 'cycling')),
    intensity  TEXT    NOT NULL CHECK (intensity IN ('higher', 'lower')),
    volume     TEXT    NOT NULL CHECK (volume IN ('higher', 'lower')),

    PRIMARY KEY (pattern, weekday, part)
) STRICT, WITHOUT ROWID;

INSERT INTO training_slot_rebuilt (pattern, weekday, part, discipline, intensity, volume)
    SELECT s.pattern, s.weekday, s.part, s.discipline,
           CASE
               WHEN s.discipline = 'gym' THEN
                   CASE (SELECT g.role FROM gym_weekday g
                          WHERE g.weekday = s.weekday
                          ORDER BY g.mesocycle DESC LIMIT 1)
                       WHEN 'heavy' THEN 'higher'
                       WHEN 'light' THEN 'lower'
                   END
               WHEN s.discipline = 'cycling' THEN
                   CASE
                       WHEN (SELECT c.session FROM cycling_weekday c
                              WHERE c.weekday = s.weekday
                              ORDER BY c.mesocycle DESC LIMIT 1) IS NULL THEN NULL
                       WHEN (SELECT c.session FROM cycling_weekday c
                              WHERE c.weekday = s.weekday
                              ORDER BY c.mesocycle DESC LIMIT 1) = 1 THEN 'higher'
                       ELSE 'lower'
                   END
           END,
           CASE
               WHEN s.discipline = 'gym' THEN
                   CASE (SELECT g.role FROM gym_weekday g
                          WHERE g.weekday = s.weekday
                          ORDER BY g.mesocycle DESC LIMIT 1)
                       WHEN 'heavy' THEN 'lower'
                       WHEN 'light' THEN 'higher'
                   END
               WHEN s.discipline = 'cycling' THEN
                   CASE
                       WHEN (SELECT c.session FROM cycling_weekday c
                              WHERE c.weekday = s.weekday
                              ORDER BY c.mesocycle DESC LIMIT 1) IS NULL THEN NULL
                       WHEN (SELECT c.session FROM cycling_weekday c
                              WHERE c.weekday = s.weekday
                              ORDER BY c.mesocycle DESC LIMIT 1) = 1 THEN 'lower'
                       ELSE 'higher'
                   END
           END
    FROM training_slot s;

DROP TABLE training_slot;
ALTER TABLE training_slot_rebuilt RENAME TO training_slot;

CREATE TABLE alteration_slot_rebuilt (
    alteration INTEGER NOT NULL REFERENCES alteration(id) ON DELETE CASCADE,
    weekday    TEXT    NOT NULL
        CHECK (weekday IN ('monday', 'tuesday', 'wednesday', 'thursday',
                           'friday', 'saturday', 'sunday')),
    part       TEXT    NOT NULL
        CHECK (part IN ('morning', 'afternoon', 'evening')),
    discipline TEXT    NOT NULL CHECK (discipline IN ('gym', 'cycling')),
    intensity  TEXT    NOT NULL CHECK (intensity IN ('higher', 'lower')),
    volume     TEXT    NOT NULL CHECK (volume IN ('higher', 'lower')),

    PRIMARY KEY (alteration, weekday, part)
) STRICT, WITHOUT ROWID;

INSERT INTO alteration_slot_rebuilt (alteration, weekday, part, discipline, intensity, volume)
    SELECT a.alteration, a.weekday, a.part, a.discipline,
           (SELECT t.intensity FROM training_slot t
             WHERE t.weekday = a.weekday AND t.discipline = a.discipline
             ORDER BY t.pattern DESC LIMIT 1),
           (SELECT t.volume FROM training_slot t
             WHERE t.weekday = a.weekday AND t.discipline = a.discipline
             ORDER BY t.pattern DESC LIMIT 1)
    FROM alteration_slot a;

DROP TABLE alteration_slot;
ALTER TABLE alteration_slot_rebuilt RENAME TO alteration_slot;

CREATE TABLE cycling_ride_rebuilt (
    mesocycle       INTEGER NOT NULL REFERENCES cycling_mesocycle(id) ON DELETE CASCADE,
    microcycle      INTEGER NOT NULL CHECK (microcycle > 0),
    session         INTEGER NOT NULL CHECK (session > 0),
    published_session INTEGER NOT NULL CHECK (published_session > 0),

    intensity       TEXT    NOT NULL CHECK (intensity IN ('higher', 'lower')),
    volume          TEXT    NOT NULL CHECK (volume IN ('higher', 'lower')),

    warm_up_seconds INTEGER NOT NULL CHECK (warm_up_seconds > 0),
    cool_down_seconds INTEGER CHECK (cool_down_seconds IS NULL OR cool_down_seconds > 0),
    effort_seconds  INTEGER CHECK (effort_seconds IS NULL OR effort_seconds > 0),

    PRIMARY KEY (mesocycle, microcycle, session),
    UNIQUE (mesocycle, microcycle, intensity, volume),
    FOREIGN KEY (mesocycle, microcycle)
        REFERENCES cycling_microcycle(mesocycle, ordinal) ON DELETE CASCADE
) STRICT, WITHOUT ROWID;

INSERT INTO cycling_ride_rebuilt (mesocycle, microcycle, session, published_session,
                          intensity, volume, warm_up_seconds, cool_down_seconds,
                          effort_seconds)
    WITH lengths AS (
        SELECT r.mesocycle, r.microcycle, r.session, r.published_session,
               r.warm_up_seconds, r.cool_down_seconds, r.effort_seconds,
               r.warm_up_seconds
                 + COALESCE(r.cool_down_seconds, 0)
                 + COALESCE(r.effort_seconds, 0)
                 + COALESCE((SELECT SUM(i.seconds) FROM cycling_interval i
                              WHERE i.mesocycle = r.mesocycle
                                AND i.microcycle = r.microcycle
                                AND i.session = r.session), 0) AS total_seconds
        FROM cycling_ride r
    )
    SELECT l.mesocycle, l.microcycle, l.session, l.published_session,
           CASE WHEN l.session = (
               SELECT s.session FROM lengths s
                WHERE s.mesocycle = l.mesocycle AND s.microcycle = l.microcycle
                ORDER BY s.total_seconds ASC, s.session ASC LIMIT 1
           ) THEN 'higher' ELSE 'lower' END,
           CASE WHEN l.session = (
               SELECT s.session FROM lengths s
                WHERE s.mesocycle = l.mesocycle AND s.microcycle = l.microcycle
                ORDER BY s.total_seconds ASC, s.session ASC LIMIT 1
           ) THEN 'lower' ELSE 'higher' END,
           l.warm_up_seconds, l.cool_down_seconds, l.effort_seconds
    FROM lengths l;

DROP TABLE cycling_ride;
ALTER TABLE cycling_ride_rebuilt RENAME TO cycling_ride;

DROP TABLE gym_weekday;
DROP TABLE cycling_weekday;

CREATE TABLE gym_slot_fill_rebuilt (
    mesocycle  INTEGER NOT NULL REFERENCES gym_mesocycle(id) ON DELETE CASCADE,
    slot       TEXT    NOT NULL,
    intensity  TEXT    CHECK (intensity IS NULL OR intensity IN ('higher', 'lower')),

    position   INTEGER NOT NULL DEFAULT 0,
    exercise   TEXT    NOT NULL,

    static_sets INTEGER CHECK (static_sets IS NULL OR static_sets > 0),
    static_reps INTEGER CHECK (static_reps IS NULL OR static_reps > 0),
    CHECK ((static_sets IS NULL) = (static_reps IS NULL)),

    PRIMARY KEY (mesocycle, slot, intensity, position)
);

INSERT INTO gym_slot_fill_rebuilt (mesocycle, slot, intensity, position, exercise,
                           static_sets, static_reps)
    SELECT mesocycle, slot,
           CASE role WHEN 'heavy' THEN 'higher' WHEN 'light' THEN 'lower' END,
           position, exercise, static_sets, static_reps
    FROM gym_slot_fill;

DROP TABLE gym_slot_fill;
ALTER TABLE gym_slot_fill_rebuilt RENAME TO gym_slot_fill;

DROP TRIGGER prescribed_workout_performed_is_not_deletable;

CREATE TABLE prescribed_workout_rebuilt (
    id                     INTEGER PRIMARY KEY AUTOINCREMENT,
    mesocycle              INTEGER NOT NULL REFERENCES gym_mesocycle(id),

    issued_for             TEXT    NOT NULL,
    zone                   TEXT    NOT NULL,
    session_intensity      TEXT    NOT NULL CHECK (session_intensity IN ('higher', 'lower')),
    session_volume         TEXT    NOT NULL CHECK (session_volume IN ('higher', 'lower')),

    week_kind              TEXT    NOT NULL CHECK (week_kind IN ('climbing', 'test')),
    week_index             INTEGER,

    anchor_grams           INTEGER CHECK (anchor_grams IS NULL OR anchor_grams > 0),
    anchor_provenance      TEXT
        CHECK (anchor_provenance IS NULL
               OR anchor_provenance IN ('tested', 'estimated', 'asserted')),
    anchor_from            TEXT,
    anchor_failed_grams    INTEGER
        CHECK (anchor_failed_grams IS NULL OR anchor_failed_grams > anchor_grams),

    target_grams           INTEGER CHECK (target_grams IS NULL OR target_grams > 0),

    parameters_authored_at TEXT    NOT NULL
        REFERENCES generation_parameters(authored_at),
    issued_at              TEXT    NOT NULL,

    UNIQUE (issued_for, issued_at),

    CHECK ((week_kind = 'climbing') = (week_index IS NOT NULL)),
    CHECK ((anchor_grams IS NULL) = (anchor_provenance IS NULL)),
    CHECK ((anchor_grams IS NULL) = (anchor_from IS NULL)),
    CHECK (week_kind = 'test' OR target_grams IS NULL),
    CHECK ((anchor_grams IS NULL) != (target_grams IS NULL))
) STRICT;

INSERT INTO prescribed_workout_rebuilt (id, mesocycle, issued_for, zone, session_intensity,
                                session_volume, week_kind, week_index, anchor_grams,
                                anchor_provenance, anchor_from, anchor_failed_grams,
                                target_grams, parameters_authored_at, issued_at)
    SELECT id, mesocycle, issued_for, zone,
           CASE session_role WHEN 'heavy' THEN 'higher' ELSE 'lower' END,
           CASE session_role WHEN 'heavy' THEN 'lower' ELSE 'higher' END,
           week_kind, week_index, anchor_grams, anchor_provenance, anchor_from,
           anchor_failed_grams, target_grams, parameters_authored_at, issued_at
    FROM prescribed_workout;

DROP TABLE prescribed_workout;
ALTER TABLE prescribed_workout_rebuilt RENAME TO prescribed_workout;

CREATE TRIGGER prescribed_workout_performed_is_not_deletable
BEFORE DELETE ON prescribed_workout
WHEN EXISTS (
    SELECT 1
    FROM prescription_delivery AS d
    JOIN gym_workout AS w ON w.performed_against = d.reference
    WHERE d.prescription = OLD.id
)
BEGIN
    SELECT RAISE(ABORT, 'a performed prescription is not deletable (constitution 12)');
END;

DROP INDEX IF EXISTS gym_mesocycle_start;

CREATE TABLE gym_mesocycle_rebuilt (
    id                 INTEGER PRIMARY KEY AUTOINCREMENT,

    plan               INTEGER NOT NULL REFERENCES plan(id) ON DELETE CASCADE,
    ordinal            INTEGER NOT NULL CHECK (ordinal > 0),

    provider           TEXT    CHECK (provider IS NULL
                                      OR length(trim(provider)) > 0),
    provided_programme TEXT    CHECK (provided_programme IS NULL
                                      OR length(trim(provided_programme)) > 0),
    template           TEXT    NOT NULL
        CHECK (template IN ('linear', 'block', 'sbs', 'test')),

    primary_pattern    TEXT    NOT NULL
        CHECK (primary_pattern IN ('knee_dominant', 'hip_dominant',
                                   'upper_push', 'upper_pull')),
    primary_exercise   TEXT    NOT NULL,

    asserted_grams     INTEGER CHECK (asserted_grams IS NULL OR asserted_grams > 0),
    asserted_provenance TEXT
        CHECK (asserted_provenance IS NULL
               OR asserted_provenance IN ('tested', 'estimated', 'asserted')),
    asserted_from      TEXT,
    asserted_failed_grams INTEGER
        CHECK (asserted_failed_grams IS NULL
               OR asserted_failed_grams > asserted_grams),

    gating_intensity   TEXT    CHECK (gating_intensity IS NULL
                                        OR gating_intensity IN ('higher', 'lower')),
    gating_volume      TEXT    CHECK (gating_volume IS NULL
                                        OR gating_volume IN ('higher', 'lower')),
    start_date         TEXT    NOT NULL,
    duration_weeks     INTEGER NOT NULL,

    test_reps          INTEGER CHECK (test_reps IS NULL OR test_reps > 0),

    entry_test_reps    INTEGER CHECK (entry_test_reps IS NULL OR entry_test_reps > 0),
    entry_test_light_grams INTEGER CHECK (entry_test_light_grams IS NULL
                                          OR entry_test_light_grams > 0),

    CHECK (template = 'test' OR entry_test_reps IS NOT NULL OR asserted_grams IS NULL),
    CHECK ((asserted_grams IS NULL) = (asserted_provenance IS NULL)),
    CHECK ((asserted_grams IS NULL) = (asserted_from IS NULL)),
    CHECK (asserted_grams IS NOT NULL OR asserted_failed_grams IS NULL),

    CHECK ((gating_intensity IS NULL) = (gating_volume IS NULL)),
    CHECK ((template = 'test') = (gating_intensity IS NULL)),
    CHECK ((template = 'test') = (test_reps IS NOT NULL)),

    CHECK (CASE template WHEN 'test' THEN duration_weeks = 1
                         WHEN 'sbs'  THEN duration_weeks = 4
                         ELSE duration_weeks >= 2 END),

    CHECK (template = 'block' OR entry_test_reps IS NULL),
    CHECK (entry_test_reps IS NOT NULL OR entry_test_light_grams IS NULL),

    CHECK ((provider IS NULL) = (provided_programme IS NULL)),
    CHECK (template <> 'sbs' OR provider IS NOT NULL),
    CHECK (template IN ('sbs', 'test') OR provider IS NULL),

    UNIQUE (plan, ordinal)
) STRICT;

INSERT INTO gym_mesocycle_rebuilt (
    id, plan, ordinal, provider, provided_programme, template,
    primary_pattern, primary_exercise,
    asserted_grams, asserted_provenance, asserted_from, asserted_failed_grams,
    gating_intensity, gating_volume, start_date, duration_weeks,
    test_reps, entry_test_reps, entry_test_light_grams
)
SELECT
    id, plan, ordinal, provider, provided_programme, template,
    primary_pattern, primary_exercise,
    asserted_grams, asserted_provenance, asserted_from, asserted_failed_grams,
    CASE gating_role WHEN 'heavy' THEN 'higher' WHEN 'light' THEN 'lower' END,
    CASE gating_role WHEN 'heavy' THEN 'lower' WHEN 'light' THEN 'higher' END,
    start_date, duration_weeks,
    test_reps, entry_test_reps, entry_test_light_grams
FROM gym_mesocycle;

DROP TABLE gym_mesocycle;
ALTER TABLE gym_mesocycle_rebuilt RENAME TO gym_mesocycle;

CREATE INDEX gym_mesocycle_start ON gym_mesocycle(start_date);

PRAGMA foreign_keys = ON;
