//! One result's description, read as the sets it records.
//!
//! **A format is the gym's prescription, not what was done** (operator,
//! 2026-09-28). An AMRAP, an EMOM, a set of rounds for time: what derives is
//! the sets performed, and the score is not kept. Each layout below was read
//! off the operator's export of 2026-09-28, and a description matching none of
//! them is refused with the reason, not guessed at.
//!
//! **What the description says is what derives.** A `modified` result's
//! description is what he did. A line with no count is a set whose count is
//! unstated; a calorie count is the prescription for a machine he rode for a
//! time nobody wrote down, so it is dropped; a rep exercise given a time
//! (`Dumbbell Floor Press, 45 secs`) derives with its reps unstated. A load in
//! pounds or poods is converted as stated, a pood being a 16 kg kettlebell.

use domain::measure::Kg;

/// One set, as the description gives it, before its name is mapped.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) struct Done {
    pub(super) name: String,
    /// Repetitions, or calories, or skips: whatever number the line leads
    /// with. Which of them the exercise is counted in is decided on mapping.
    pub(super) count: Option<u32>,
    pub(super) seconds: Option<u64>,
    pub(super) millimetres: Option<u64>,
    pub(super) load: Option<Kg>,
    pub(super) rest_after: Option<u64>,
}

/// The sets of one result, grouped as they were performed: each group is one
/// exercise, or a superset of the exercises done in rounds.
pub(super) type Groups = Vec<Vec<Done>>;

/// What the export says about one result, besides its description.
pub(super) struct Score<'a> {
    /// `Formatted Result`: `3 rounds + 40 Single Unders | 412 reps`.
    pub(super) formatted: &'a str,
    /// `Result`: `3.133`.
    pub(super) result: &'a str,
}

/// The sets a description records, or why it cannot be read.
pub(super) fn read(description: &str, score: &Score<'_>) -> Result<Groups, String> {
    let mut lines = description
        .lines()
        .map(str::trim)
        .filter(|line| !line.is_empty());
    let Some(header) = lines.next() else {
        return Err("an empty description".to_owned());
    };
    let body: Vec<&str> = lines.collect();

    if header == "Sets" || header.starts_with("Sets :") {
        let rest = header
            .strip_prefix("Sets : rest ")
            .map(seconds_in)
            .transpose()?;
        let mut sets = parsed(&body)?;
        let last = sets.len().saturating_sub(1);
        for (index, set) in sets.iter_mut().enumerate() {
            if index < last {
                set.rest_after = rest;
            }
        }
        return Ok(vec![sets]);
    }
    if let Some(rounds) = header
        .strip_suffix(" rounds of:")
        .or_else(|| header.strip_suffix(" rounds, each round for time, of:"))
    {
        let rounds = number(rounds)?;
        return in_rounds(&body, rounds);
    }
    if let Some(scheme) = header.strip_suffix(" reps of:") {
        let counts = scheme
            .split('-')
            .map(number)
            .collect::<Result<Vec<_>, _>>()?;
        return by_scheme(&body, &counts);
    }
    if let Some(every) = header.strip_prefix("Every ") {
        return every_interval(every, &body);
    }
    if header.ends_with(" AMRAP:") {
        return amrap(header, &body, score);
    }
    if header.starts_with("AMReps") {
        return Err(format!(
            "{header:?}, whose score ({}) the lines do not account for",
            score.formatted
        ));
    }
    let mut all = vec![header];
    all.extend(body);
    Ok(vec![parsed(&all)?])
}

/// Every line a set, in the order written.
fn parsed(lines: &[&str]) -> Result<Vec<Done>, String> {
    let mut sets = Vec::new();
    for line in lines {
        match line_of(line)? {
            Line::Sets(done) => sets.extend(done),
            Line::Rest(_) | Line::Nothing => {}
        }
    }
    Ok(sets)
}

/// The lines of a round, repeated, with any rest after each round but the
/// last.
fn in_rounds(body: &[&str], rounds: u32) -> Result<Groups, String> {
    let (lines, rest) = round_of(body)?;
    let mut sets = Vec::new();
    for round in 0..rounds {
        let mut this = lines.clone();
        if round.saturating_add(1) < rounds
            && let Some(last) = this.last_mut()
        {
            last.rest_after = rest;
        }
        sets.extend(this);
    }
    Ok(vec![sets])
}

/// One round's sets, and the rest after it.
fn round_of(body: &[&str]) -> Result<(Vec<Done>, Option<u64>), String> {
    let mut lines = Vec::new();
    let mut rest = None;
    for line in body {
        match line_of(line)? {
            Line::Sets(done) => lines.extend(done),
            Line::Rest(seconds) => rest = Some(seconds),
            Line::Nothing => {}
        }
    }
    Ok((lines, rest))
}

/// `21-15-9 reps of:`: each round the scheme's count of every line that does
/// not state its own.
fn by_scheme(body: &[&str], counts: &[u32]) -> Result<Groups, String> {
    let (lines, rest) = round_of(body)?;
    let mut sets = Vec::new();
    let last = counts.len().saturating_sub(1);
    for (round, count) in counts.iter().enumerate() {
        let mut this: Vec<Done> = lines
            .iter()
            .map(|line| {
                let mut set = line.clone();
                if set.count.is_none() && set.millimetres.is_none() && set.seconds.is_none() {
                    set.count = Some(*count);
                }
                set
            })
            .collect();
        if round < last
            && let Some(last) = this.last_mut()
        {
            last.rest_after = rest;
        }
        sets.extend(this);
    }
    Ok(vec![sets])
}

/// `Every 1 min for 12 mins, alternating between:` and its kin.
///
/// Alternating takes one line an interval, in turn, and `Rest 1 min` is an
/// interval with nothing in it. Otherwise a line an interval where there are
/// as many lines as intervals (six pull-downs over six minutes), and every line
/// every interval where there are not (`Every 3:30 for 14 mins`, four rounds).
fn every_interval(every: &str, body: &[&str]) -> Result<Groups, String> {
    let (every, alternating) = every.strip_suffix(", alternating between:").map_or_else(
        || (every.strip_suffix(':').unwrap_or(every), false),
        |every| (every, true),
    );
    let Some((interval, total)) = every.split_once(" for ") else {
        return Err(format!("every {every:?}, which names no length"));
    };
    let interval = seconds_in(interval)?;
    let total = seconds_in(total)?;
    let intervals = total
        .checked_div(interval)
        .filter(|count| count.checked_mul(interval) == Some(total) && *count > 0)
        .ok_or_else(|| format!("{total} s is not a whole number of {interval} s intervals"))?;
    let intervals = usize::try_from(intervals).map_err(|error| error.to_string())?;

    let mut slots = Vec::new();
    for line in body {
        slots.push(line_of(line)?);
    }
    let slots: Vec<Line> = slots
        .into_iter()
        .filter(|slot| !matches!(slot, Line::Nothing))
        .collect();
    if slots.is_empty() {
        return Err("an interval with nothing in it".to_owned());
    }

    let mut sets = Vec::new();
    if alternating || slots.len() == intervals {
        for index in 0..intervals {
            if let Some(Line::Sets(done)) = slots.get(index.checked_rem(slots.len()).unwrap_or(0)) {
                sets.extend(done.iter().cloned());
            }
        }
    } else {
        for _ in 0..intervals {
            for slot in &slots {
                if let Line::Sets(done) = slot {
                    sets.extend(done.iter().cloned());
                }
            }
        }
    }
    Ok(vec![sets])
}

/// `12:00 AMRAP:` or `3x 4:00 AMRAP:`, whose rounds are the score's.
///
/// The score is the whole rounds, plus whatever it names of the next one:
/// `4 rounds + 20 Deadlifts + 40 Push-ups`. Several AMRAPs are scored
/// together. A fraction the score does not name (`2.167 rounds`) is not read,
/// because nothing says which movement it was.
fn amrap(header: &str, body: &[&str], score: &Score<'_>) -> Result<Groups, String> {
    let (lines, _) = round_of(body)?;
    let rounds = whole_rounds(score)?;
    let mut sets = Vec::new();
    for _ in 0..rounds {
        sets.extend(lines.iter().cloned());
    }

    let named = score.formatted.split(" | ").next().unwrap_or_default();
    for part in named.split(" + ").skip(1) {
        let Some((count, name)) = part.split_once(' ') else {
            return Err(format!("{part:?} in the score of {header:?}"));
        };
        let count = number(count)?;
        let Some(line) = lines.iter().find(|line| line.name == name) else {
            return Err(format!(
                "the score of {header:?} names {name:?}, which is none of its lines"
            ));
        };
        let mut set = line.clone();
        set.count = Some(count);
        set.seconds = None;
        sets.push(set);
    }
    Ok(vec![sets])
}

/// The whole rounds a score records.
fn whole_rounds(score: &Score<'_>) -> Result<u32, String> {
    let written = score
        .formatted
        .split_once(" rounds")
        .map_or(score.result, |(rounds, _)| rounds);
    let whole = written.split('.').next().unwrap_or_default();
    number(whole).map_err(|_| format!("a score of {:?}, which is not rounds", score.formatted))
}

/// What one line of a description is.
enum Line {
    Sets(Vec<Done>),
    /// `Rest 1 min`, `Resting 2 mins between each round.`
    Rest(u64),
    /// A time cap, or a line recording that nothing was done.
    Nothing,
}

fn line_of(line: &str) -> Result<Line, String> {
    if line.starts_with("Time cap") {
        return Ok(Line::Nothing);
    }
    if let Some(rest) = line.strip_prefix("Resting ") {
        let length = rest.split(" between").next().unwrap_or(rest);
        return seconds_in(length).map(Line::Rest);
    }
    if let Some(rest) = line.strip_prefix("Rest ") {
        return seconds_in(rest).map(Line::Rest);
    }

    let (left, right) = line
        .split_once('|')
        .map_or((line, ""), |(left, right)| (left, right.trim()));
    let left = left
        .trim()
        .trim_start_matches('[')
        .trim_end_matches(']')
        .trim();
    let (tabata, left) = left
        .strip_prefix("Tabata ")
        .map_or((false, left), |left| (true, left));

    let (count, rest) = match left.split_once(' ') {
        Some((first, rest)) if first.starts_with(|c: char| c.is_ascii_digit()) => {
            let first = first.split('/').next().unwrap_or(first);
            (Some(number(first)?), rest)
        }
        _ => (None, left),
    };
    let mut parts = rest.split(", ");
    let name = parts.next().unwrap_or_default().trim().to_owned();
    if name.is_empty() {
        return Err(format!("{line:?}, which names no movement"));
    }
    let mut set = Done {
        name,
        count,
        seconds: None,
        millimetres: None,
        load: None,
        rest_after: None,
    };
    for qualifier in parts {
        qualify(&mut set, qualifier.trim()).map_err(|detail| format!("{line:?}: {detail}"))?;
    }

    if tabata {
        let counts = right
            .split(',')
            .map(|count| number(count.trim()))
            .collect::<Result<Vec<_>, _>>()
            .map_err(|detail| format!("{line:?}: {detail}"))?;
        return Ok(Line::Sets(
            counts
                .into_iter()
                .map(|count| Done {
                    count: Some(count),
                    ..set.clone()
                })
                .collect(),
        ));
    }

    if let Some(done) = right
        .strip_suffix(" reps")
        .or_else(|| right.strip_suffix(" rep"))
    {
        set.count = Some(number(done)?);
    } else if !right.is_empty() && right != "kg" {
        qualify(&mut set, right).map_err(|detail| format!("{line:?}: {detail}"))?;
    }
    if set.count == Some(0) {
        return Ok(Line::Nothing);
    }
    Ok(Line::Sets(vec![set]))
}

/// One qualifier after the movement's name: a load, a time, a distance, or a
/// percentage of a maximum, which is the prescription and is not kept.
fn qualify(set: &mut Done, qualifier: &str) -> Result<(), String> {
    let Some((value, unit)) = qualifier.split_once(' ') else {
        return Err(format!("{qualifier:?} is not a quantity"));
    };
    match unit {
        "kg" => set.load = Some(value.parse::<Kg>().map_err(|error| error.to_string())?),
        "lbs" | "lb" => set.load = Some(Kg::from_pounds(value).map_err(|error| error.to_string())?),
        "pood" => {
            let poods = value.parse::<Kg>().map_err(|error| error.to_string())?;
            set.load = Some(Kg::from_grams(poods.as_grams().saturating_mul(16)));
        }
        "secs" | "sec" | "min" | "mins" => set.seconds = Some(seconds_in(qualifier)?),
        "m" => {
            set.millimetres = Some(u64::from(number(value)?).saturating_mul(1000));
        }
        // A box's height: nothing in the model holds it.
        "in" => {}
        "1RM" if value.ends_with('%') => {}
        _ => return Err(format!("{qualifier:?} is not a quantity this reads")),
    }
    Ok(())
}

/// `45 secs`, `3 mins`, `1 min`, `2:30`, `3:30`.
fn seconds_in(text: &str) -> Result<u64, String> {
    let text = text.trim();
    if let Some((minutes, seconds)) = text.split_once(':') {
        let minutes = u64::from(number(minutes)?);
        let seconds = u64::from(number(seconds)?);
        return Ok(minutes.saturating_mul(60).saturating_add(seconds));
    }
    let (value, unit) = text
        .split_once(' ')
        .ok_or_else(|| format!("{text:?} is not a length of time"))?;
    let value = u64::from(number(value)?);
    match unit {
        "sec" | "secs" => Ok(value),
        "min" | "mins" => Ok(value.saturating_mul(60)),
        _ => Err(format!("{text:?} is not a length of time")),
    }
}

fn number(text: &str) -> Result<u32, String> {
    text.trim()
        .parse::<u32>()
        .map_err(|_| format!("{text:?} is not a whole number"))
}

#[cfg(test)]
mod tests {
    use domain::measure::Kg;

    use super::{Done, Score, read};

    fn score<'a>(formatted: &'a str, result: &'a str) -> Score<'a> {
        Score { formatted, result }
    }

    /// Each set as `count name load time`, `?` where it is unstated.
    fn shown(groups: &[Vec<Done>]) -> Vec<Vec<String>> {
        groups
            .iter()
            .map(|group| {
                group
                    .iter()
                    .map(|set| {
                        let count = set.count.map_or_else(|| "?".to_owned(), |c| c.to_string());
                        let load = set.load.map(|kg| format!(" @{kg} kg")).unwrap_or_default();
                        let time = set.seconds.map(|s| format!(" {s}s")).unwrap_or_default();
                        let far = set
                            .millimetres
                            .map(|mm| format!(" {}m", mm / 1000))
                            .unwrap_or_default();
                        let rest = set
                            .rest_after
                            .map(|s| format!(" rest {s}s"))
                            .unwrap_or_default();
                        format!("{count} {}{load}{time}{far}{rest}", set.name)
                    })
                    .collect()
            })
            .collect()
    }

    #[test]
    fn sets_carry_their_load_after_the_bar() {
        let groups = read(
            "Sets\n5 Deadlifts | 90 kg\n5 Deadlifts, 75% 1RM | 100 kg",
            &score("", ""),
        )
        .expect("read");
        assert_eq!(
            shown(&groups),
            vec![vec!["5 Deadlifts @90 kg", "5 Deadlifts @100 kg"]]
        );
    }

    #[test]
    fn a_load_left_blank_is_unstated_and_reps_after_the_bar_are_the_count() {
        let groups = read(
            "Sets\n6 Self Assisted Pull-ups |  kg\nStraight Arm Lat Pull Down, 41 kg | 12 reps",
            &score("", ""),
        )
        .expect("read");
        assert_eq!(
            shown(&groups),
            vec![vec![
                "6 Self Assisted Pull-ups",
                "12 Straight Arm Lat Pull Down @41 kg"
            ]]
        );
    }

    #[test]
    fn rounds_repeat_their_lines_and_rest_between_them() {
        let groups = read(
            "3 rounds of:\n10 Toes-to-bars\nBurpee\nRest 1 min",
            &score("Completed", ""),
        )
        .expect("read");
        assert_eq!(
            shown(&groups),
            vec![vec![
                "10 Toes-to-bars",
                "? Burpee rest 60s",
                "10 Toes-to-bars",
                "? Burpee rest 60s",
                "10 Toes-to-bars",
                "? Burpee",
            ]]
        );
    }

    #[test]
    fn an_amrap_is_its_whole_rounds_and_the_partial_its_score_names() {
        let groups = read(
            "12:00 AMRAP:\n100 Single Unders\n16 Dumbbell Snatches\n8 Toes-to-bars",
            &score("3 rounds + 40 Single Unders | 412 reps", "3.133"),
        )
        .expect("read");
        let sets = shown(&groups).concat();
        assert_eq!(sets.len(), 10);
        assert_eq!(sets.last().map(String::as_str), Some("40 Single Unders"));
    }

    #[test]
    fn a_fraction_the_score_does_not_name_is_not_read() {
        let groups = read(
            "10:00 AMRAP:\n10 Kettlebell Rows\n50 Double Unders",
            &score("2.167 rounds | 2.167 rounds", "2.167"),
        )
        .expect("read");
        assert_eq!(shown(&groups).concat().len(), 4);
    }

    #[test]
    fn alternating_intervals_take_a_line_each_and_rest_takes_one_too() {
        let groups = read(
            "Every 1 min for 12 mins, alternating between:\nBurpee\nAny Machine Calorie\nDumbbell Thruster, 40 secs\nRest 1 min",
            &score("Completed", ""),
        )
        .expect("read");
        assert_eq!(shown(&groups).concat().len(), 9);
    }

    #[test]
    fn every_line_every_interval_where_the_lines_are_not_one_each() {
        let groups = read(
            "Every 3:30 for 14 mins:\nAny Machine Calorie\nBurpee\n12 Thrusters",
            &score("4.0 rounds", "4.0"),
        )
        .expect("read");
        assert_eq!(shown(&groups).concat().len(), 12);
    }

    #[test]
    fn a_scheme_gives_each_round_its_count() {
        let groups = read(
            "21-15-9 reps of:\nThruster, 20 kg\n[ Run, 200 m ]",
            &score("4 mins 47 secs", "287000"),
        )
        .expect("read");
        assert_eq!(
            shown(&groups),
            vec![vec![
                "21 Thruster @20 kg",
                "? Run 200m",
                "15 Thruster @20 kg",
                "? Run 200m",
                "9 Thruster @20 kg",
                "? Run 200m",
            ]]
        );
    }

    #[test]
    fn pounds_and_poods_are_converted_as_stated() {
        let groups = read(
            "Run, 400 m\n10 Push Press, 115 lbs\n21 Kettlebell Swings, 1.5 pood\n10 Box Jumps, 24 in",
            &score("10 mins", ""),
        )
        .expect("read");
        let loads: Vec<Option<Kg>> = groups.concat().iter().map(|set| set.load).collect();
        assert_eq!(
            loads,
            vec![
                None,
                Some(Kg::from_grams(52_163)),
                Some(Kg::from_grams(24_000)),
                None
            ]
        );
    }

    #[test]
    fn a_line_recorded_as_not_done_derives_nothing() {
        let groups = read(
            "30 Bicep Curls\n[ 10/7 Any Machine Calories ] | 0 reps\n\nTime cap: 15 mins",
            &score("1 rep", ""),
        )
        .expect("read");
        assert_eq!(shown(&groups), vec![vec!["30 Bicep Curls"]]);
    }

    #[test]
    fn tabata_counts_are_its_sets() {
        let groups = read(
            "Tabata Any Machine Calorie | 16,15,15,14\nTabata Burpee | 15,14,14,13",
            &score("27 reps | 14 + 13", "27"),
        )
        .expect("read");
        assert_eq!(shown(&groups).concat().len(), 8);
    }

    #[test]
    fn a_result_whose_lines_do_not_account_for_its_score_is_refused() {
        assert!(
            read(
                "AMReps in 10 mins:\n4 Wall Balls\n4x [ 6 Push-ups ]",
                &score("168 reps | 24's", "168"),
            )
            .is_err()
        );
    }
}
