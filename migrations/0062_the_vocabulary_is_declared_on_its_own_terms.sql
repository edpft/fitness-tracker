UPDATE gym_mesocycle SET primary_exercise = CASE primary_exercise
    WHEN 'squat-barbell' THEN 'back-squat-barbell'
    WHEN 'front-squat' THEN 'front-squat-barbell'
    WHEN 'triceps-extension-cable' THEN 'single-arm-triceps-extension-cable'
    WHEN 'single-arm-tricep-extension-dumbbell' THEN 'single-arm-triceps-extension-dumbbell'
    WHEN 'lu-raise' THEN 'overhead-lateral-raise'
    ELSE primary_exercise END;

UPDATE gym_slot_fill SET exercise = CASE exercise
    WHEN 'squat-barbell' THEN 'back-squat-barbell'
    WHEN 'front-squat' THEN 'front-squat-barbell'
    WHEN 'triceps-extension-cable' THEN 'single-arm-triceps-extension-cable'
    WHEN 'single-arm-tricep-extension-dumbbell' THEN 'single-arm-triceps-extension-dumbbell'
    WHEN 'lu-raise' THEN 'overhead-lateral-raise'
    ELSE exercise END;

UPDATE prescribed_exercise SET exercise = CASE exercise
    WHEN 'squat-barbell' THEN 'back-squat-barbell'
    WHEN 'front-squat' THEN 'front-squat-barbell'
    WHEN 'triceps-extension-cable' THEN 'single-arm-triceps-extension-cable'
    WHEN 'single-arm-tricep-extension-dumbbell' THEN 'single-arm-triceps-extension-dumbbell'
    WHEN 'lu-raise' THEN 'overhead-lateral-raise'
    ELSE exercise END;

UPDATE performed_exercise SET exercise = CASE exercise
    WHEN 'squat-barbell' THEN 'back-squat-barbell'
    WHEN 'front-squat' THEN 'front-squat-barbell'
    WHEN 'triceps-extension-cable' THEN 'single-arm-triceps-extension-cable'
    WHEN 'single-arm-tricep-extension-dumbbell' THEN 'single-arm-triceps-extension-dumbbell'
    WHEN 'lu-raise' THEN 'overhead-lateral-raise'
    ELSE exercise END;

UPDATE normalisation_refusal SET exercise = CASE exercise
    WHEN 'squat-barbell' THEN 'back-squat-barbell'
    WHEN 'front-squat' THEN 'front-squat-barbell'
    WHEN 'triceps-extension-cable' THEN 'single-arm-triceps-extension-cable'
    WHEN 'single-arm-tricep-extension-dumbbell' THEN 'single-arm-triceps-extension-dumbbell'
    WHEN 'lu-raise' THEN 'overhead-lateral-raise'
    ELSE exercise END;

UPDATE measured_set SET guess_exercise = CASE guess_exercise
    WHEN 'squat-barbell' THEN 'back-squat-barbell'
    WHEN 'front-squat' THEN 'front-squat-barbell'
    WHEN 'triceps-extension-cable' THEN 'single-arm-triceps-extension-cable'
    WHEN 'single-arm-tricep-extension-dumbbell' THEN 'single-arm-triceps-extension-dumbbell'
    WHEN 'lu-raise' THEN 'overhead-lateral-raise'
    ELSE guess_exercise END;
