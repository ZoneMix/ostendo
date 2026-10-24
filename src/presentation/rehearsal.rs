//! Rehearsal timings, kept in `.ostendo-rehearsals.{stem}.json` next to the
//! presentation, and the pacing report built from them.

use std::fmt::Write as _;
use std::io::Write as _;
use std::path::{Path, PathBuf};
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use anyhow::Result;
use serde::{Deserialize, Serialize};

/// Runs kept per presentation; older ones are dropped.
const MAX_RUNS: usize = 20;

/// Seconds spent on each slide during one talk or rehearsal.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Run {
    /// Unix time the run ended.
    pub ended: u64,
    /// (slide title, seconds on it), in deck order.
    pub slides: Vec<(String, f64)>,
}

impl Run {
    pub fn new(slides: Vec<(String, f64)>) -> Self {
        let ended = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map_or(0, |d| d.as_secs());
        Self { ended, slides }
    }

    pub fn total(&self) -> f64 {
        self.slides.iter().map(|s| s.1).sum()
    }
}

pub fn path_for(presentation: &Path) -> PathBuf {
    let stem = presentation
        .file_stem()
        .and_then(|s| s.to_str())
        .unwrap_or("default");
    presentation
        .parent()
        .unwrap_or(Path::new("."))
        .join(format!(".ostendo-rehearsals.{stem}.json"))
}

/// Past runs; a missing or unreadable file means none.
pub fn load(presentation: &Path) -> Vec<Run> {
    std::fs::read_to_string(path_for(presentation))
        .ok()
        .and_then(|text| serde_json::from_str(&text).ok())
        .unwrap_or_default()
}

/// Appends `run`, keeping the newest runs, through a rename so a crash never
/// truncates the history.
pub fn record(presentation: &Path, run: Run) -> Result<()> {
    let mut runs = load(presentation);
    runs.push(run);
    let excess = runs.len().saturating_sub(MAX_RUNS);
    runs.drain(..excess);
    let path = path_for(presentation);
    let dir = path.parent().filter(|d| !d.as_os_str().is_empty());
    let mut tmp = tempfile::NamedTempFile::new_in(dir.unwrap_or(Path::new(".")))?;
    tmp.write_all(serde_json::to_string_pretty(&runs)?.as_bytes())?;
    tmp.persist(&path)?;
    Ok(())
}

/// A table of the latest run against the average of the earlier ones and,
/// with a planned `duration`, an even share of it per slide.
pub fn report(titles: &[String], runs: &[Run], plan: Option<Duration>) -> String {
    let Some((latest, earlier)) = runs.split_last() else {
        return "No rehearsals yet: present with the timer running (it starts on the \
                first slide change), and runs of a minute or more are recorded on quit.\n"
            .to_string();
    };
    let per_slide = plan.map(|d| d.as_secs_f64() / titles.len().max(1) as f64);
    // By position, or by title once slides have moved since that run.
    let time_in = |run: &Run, i: usize| {
        let title = &titles[i];
        run.slides
            .get(i)
            .filter(|s| s.0 == *title)
            .or_else(|| run.slides.iter().find(|s| s.0 == *title))
            .map(|s| s.1)
    };
    let average = |i: usize| {
        let times: Vec<f64> = earlier.iter().filter_map(|r| time_in(r, i)).collect();
        (!times.is_empty()).then(|| times.iter().sum::<f64>() / times.len() as f64)
    };
    let cell = |secs: Option<f64>| secs.map_or_else(|| "—".to_string(), seconds);
    let title_w = titles
        .iter()
        .map(|t| t.chars().count())
        .max()
        .unwrap_or(5)
        .clamp(5, 36);

    let mut out = String::new();
    let _ = writeln!(
        out,
        "{} run(s); latest {}, {}{}\n",
        runs.len(),
        date(latest.ended),
        seconds(latest.total()),
        plan.map_or(String::new(), |d| format!(" of {} planned", clock(d)))
    );
    let _ = writeln!(
        out,
        "  {:>3}  {:<title_w$}  {:>7}  {:>7}  {:>7}",
        "#", "Slide", "Latest", "Average", "Plan"
    );
    for (i, title) in titles.iter().enumerate() {
        let latest_secs = time_in(latest, i);
        let flag = match (latest_secs, per_slide) {
            (Some(took), Some(plan)) if took > plan + 30.0 => {
                format!("  +{}", seconds(took - plan))
            }
            _ => String::new(),
        };
        let title: String = title.chars().take(title_w).collect();
        let _ = writeln!(
            out,
            "  {:>3}  {title:<title_w$}  {:>7}  {:>7}  {:>7}{flag}",
            i + 1,
            cell(latest_secs),
            cell(average(i)),
            cell(per_slide),
        );
    }
    let earlier_total = (!earlier.is_empty())
        .then(|| earlier.iter().map(Run::total).sum::<f64>() / earlier.len() as f64);
    let _ = writeln!(
        out,
        "  {:>3}  {:<title_w$}  {:>7}  {:>7}  {:>7}",
        "",
        "Total",
        seconds(latest.total()),
        cell(earlier_total),
        cell(plan.map(|d| d.as_secs_f64())),
    );
    out
}

/// `m:ss`, or `h:mm:ss` past an hour.
pub fn clock(d: Duration) -> String {
    let secs = d.as_secs();
    let (h, m, s) = (secs / 3600, secs / 60 % 60, secs % 60);
    if h > 0 {
        format!("{h}:{m:02}:{s:02}")
    } else {
        format!("{m}:{s:02}")
    }
}

/// `clock` for a stored time; a hand-edited negative or huge one reads as 0.
fn seconds(secs: f64) -> String {
    clock(Duration::try_from_secs_f64(secs.round()).unwrap_or_default())
}

/// `YYYY-MM-DD` (UTC) for a Unix time.
fn date(unix: u64) -> String {
    // Civil-from-days (Howard Hinnant's algorithm).
    let z = (unix / 86_400) as i64 + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z.rem_euclid(146_097);
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let day = doy - (153 * mp + 2) / 5 + 1;
    let month = if mp < 10 { mp + 3 } else { mp - 9 };
    let year = yoe + era * 400 + i64::from(month <= 2);
    format!("{year:04}-{month:02}-{day:02}")
}

#[cfg(test)]
mod tests {
    use super::*;

    fn run(ended: u64, secs: &[f64]) -> Run {
        let titles = ["Intro", "Deep dive"];
        Run {
            ended,
            slides: titles
                .iter()
                .map(|t| t.to_string())
                .zip(secs.to_vec())
                .collect(),
        }
    }

    #[test]
    fn runs_round_trip_and_only_the_newest_are_kept() {
        let dir = tempfile::tempdir().unwrap();
        let deck = dir.path().join("talk.md");
        for i in 0..MAX_RUNS as u64 + 3 {
            record(&deck, run(i, &[1.0, 2.5])).unwrap();
        }
        let runs = load(&deck);
        assert_eq!(runs.len(), MAX_RUNS);
        assert_eq!(runs[0].ended, 3);
        assert_eq!(runs.last(), Some(&run(MAX_RUNS as u64 + 2, &[1.0, 2.5])));
        assert!(dir.path().join(".ostendo-rehearsals.talk.json").exists());
    }

    #[test]
    fn the_report_compares_the_latest_run_with_earlier_ones_and_the_plan() {
        let titles = vec!["Intro".to_string(), "Deep dive".to_string()];
        let mut moved = run(0, &[70.0, 140.0]);
        moved.slides.reverse();
        let runs = [
            run(0, &[50.0, 120.0]),
            // Recorded before the slides swapped places: matched by title.
            moved,
            run(1_758_931_200, &[65.0, 245.0]),
        ];
        let text = report(&titles, &runs, Some(Duration::from_secs(240)));
        assert!(
            text.starts_with("3 run(s); latest 2025-09-27, 5:10 of 4:00 planned"),
            "{text}"
        );
        let row = text.lines().find(|l| l.contains("Deep dive")).unwrap();
        let cells: Vec<&str> = row.split_whitespace().skip(3).collect();
        // Latest, average of the earlier two, plan, and how far over it.
        assert_eq!(cells, ["4:05", "2:10", "2:00", "+2:05"], "{row}");
        let intro = text.lines().find(|l| l.contains("Intro")).unwrap();
        assert!(
            !intro.contains('+'),
            "within 30 s of the plan is not flagged: {intro}"
        );
        assert!(report(&titles, &[], None).starts_with("No rehearsals yet"));
    }
}
