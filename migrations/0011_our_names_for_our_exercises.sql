-- Two exercises get our name instead of Hevy's.
--
-- The vocabulary in `domain` is supposed to be ours (§ 8), and in these two
-- places it was the source's label copied inward. The mapping in
-- `infrastructure/src/hevy/mapping.rs` is where a Hevy title belongs, and it
-- still names each of these in a comment beside the template id — what changes
-- is that the name the domain holds is now the movement as the operator means
-- it, not as Hevy prints it.
--
--   seated-palms-up-wrist-curl  -> wrist-flexion-dumbbell
--   reverse-wrist-curl-dumbbell -> wrist-extension-dumbbell
--
-- These two are the clearest case: `forearms` was one slot pairing "seated
-- palms up wrist curl" with "reverse wrist curl", which reads as two unrelated
-- curls. They are wrist flexion and wrist extension — antagonists — and the
-- template now says so in the slot names too.
--
-- **The placeholders are not renamed.** `cable-twist-up-to-down` and
-- `stretching` stay exactly what they are. The operator has been logging a bent
-- over cable chop and a squatting groin stretch under them because Hevy has no
-- exercise for either, but the record really does say what it says — rewriting
-- it here would put a claim into history that the source never made. Both
-- movements are now in the vocabulary with no Hevy template behind them, and
-- correcting the workouts that used a stand-in is the edit overlay's job.
--
-- `pull-up` is the same case and needs no row at all: `neutral-grip-pull-up` is
-- a new member of the vocabulary and nothing was ever stored under it.

UPDATE performed_exercise
SET exercise = CASE exercise
    WHEN 'seated-palms-up-wrist-curl'  THEN 'wrist-flexion-dumbbell'
    WHEN 'reverse-wrist-curl-dumbbell' THEN 'wrist-extension-dumbbell'
    ELSE exercise
END
WHERE exercise IN (
    'seated-palms-up-wrist-curl',
    'reverse-wrist-curl-dumbbell'
);

-- The refusal log names our exercise where it knew one, so it carries the same
-- keys and takes the same rewrite.
UPDATE normalisation_refusal
SET exercise = CASE exercise
    WHEN 'seated-palms-up-wrist-curl'  THEN 'wrist-flexion-dumbbell'
    WHEN 'reverse-wrist-curl-dumbbell' THEN 'wrist-extension-dumbbell'
    ELSE exercise
END
WHERE exercise IN (
    'seated-palms-up-wrist-curl',
    'reverse-wrist-curl-dumbbell'
);

-- The prescribed side is on this branch and unreleased, but a store written by
-- an earlier commit of it is a real thing on the operator's machine.
UPDATE prescribed_exercise
SET exercise = CASE exercise
    WHEN 'seated-palms-up-wrist-curl'  THEN 'wrist-flexion-dumbbell'
    WHEN 'reverse-wrist-curl-dumbbell' THEN 'wrist-extension-dumbbell'
    ELSE exercise
END;

UPDATE programme_slot_fill
SET exercise = CASE exercise
    WHEN 'seated-palms-up-wrist-curl'  THEN 'wrist-flexion-dumbbell'
    WHEN 'reverse-wrist-curl-dumbbell' THEN 'wrist-extension-dumbbell'
    ELSE exercise
END;

UPDATE programme
SET primary_exercise = CASE primary_exercise
    WHEN 'seated-palms-up-wrist-curl'  THEN 'wrist-flexion-dumbbell'
    WHEN 'reverse-wrist-curl-dumbbell' THEN 'wrist-extension-dumbbell'
    ELSE primary_exercise
END;
