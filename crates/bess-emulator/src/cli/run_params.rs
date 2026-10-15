//! Which seed, start and speed a run uses when both a scenario file and the
//! command line may state them (design D5): the file wins, an explicit flag
//! that disagrees is an error naming both values, and a flag the file does
//! not state keeps its meaning. The defaults live here, not in clap, so a
//! default can never collide with a file.

use std::fmt;

use bess_scenario::{RunParams, Scenario};

use super::Args;

/// The seed when neither a flag nor a file states one.
pub const DEFAULT_SEED: u64 = 42;
/// 2026-01-01 00:00:00 UTC.
pub const DEFAULT_START_UNIX_S: i64 = 1_767_225_600;
/// One simulated day in 24 wall-clock minutes.
pub const DEFAULT_SPEED: f64 = 60.0;

/// A flag and the scenario disagree.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PrecedenceError {
    /// The flag as typed.
    pub flag: &'static str,
    /// Its value.
    pub given: String,
    /// The file's field.
    pub field: &'static str,
    /// The file's value.
    pub stated: String,
}

impl fmt::Display for PrecedenceError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "{} {} conflicts with the scenario's {}, {}; drop the flag or change the file",
            self.flag, self.given, self.field, self.stated
        )
    }
}

impl std::error::Error for PrecedenceError {}

/// The run's parameters from the flags and the file, if any.
pub fn resolve(args: &Args, scenario: Option<&Scenario>) -> Result<RunParams, PrecedenceError> {
    Ok(RunParams {
        seed: pick(
            ("--seed", args.seed),
            ("seed", scenario.and_then(|s| s.seed)),
            DEFAULT_SEED,
        )?,
        start_unix_s: pick(
            ("--start-unix", args.start_unix),
            ("start", scenario.and_then(|s| s.start_unix_s)),
            DEFAULT_START_UNIX_S,
        )?,
        speed: pick(
            ("--speed", args.speed),
            ("speed", scenario.and_then(|s| s.speed)),
            DEFAULT_SPEED,
        )?,
    })
}

/// One parameter: the file's value when it states one, else the flag's,
/// else the default; a flag that disagrees with the file is refused.
fn pick<T: PartialEq + fmt::Display>(
    (flag, given): (&'static str, Option<T>),
    (field, stated): (&'static str, Option<T>),
    default: T,
) -> Result<T, PrecedenceError> {
    match (given, stated) {
        (Some(given), Some(stated)) if given != stated => Err(PrecedenceError {
            flag,
            given: given.to_string(),
            field,
            stated: stated.to_string(),
        }),
        (_, Some(stated)) => Ok(stated),
        (Some(given), None) => Ok(given),
        (None, None) => Ok(default),
    }
}

#[cfg(test)]
mod tests {
    use bess_core::PlantConfig;
    use bess_scenario::load;
    use clap::Parser;

    use super::*;

    fn args(flags: &[&str]) -> Args {
        Args::parse_from(std::iter::once("bess-emulator").chain(flags.iter().copied()))
    }

    fn scenario(head: &str) -> Scenario {
        let text = format!("schema: 1\nname: t\n{head}events: []\n");
        load(text.as_bytes(), &PlantConfig::gw01()).unwrap()
    }

    /// The design's table, cell by cell: file only, flag only, both equal,
    /// both different, neither.
    #[test]
    fn the_file_wins_and_a_flag_that_disagrees_is_refused() {
        let file = scenario("seed: 42\nstart: 2024-07-08T00:00:00Z\nspeed: 60\n");
        let from_file = RunParams {
            seed: 42,
            start_unix_s: 1_720_396_800,
            speed: 60.0,
        };
        assert_eq!(resolve(&args(&[]), Some(&file)), Ok(from_file));

        let flags = ["--seed", "7", "--start-unix", "100", "--speed", "3600"];
        assert_eq!(
            resolve(&args(&flags), None),
            Ok(RunParams {
                seed: 7,
                start_unix_s: 100,
                speed: 3600.0,
            })
        );

        let same = [
            "--seed",
            "42",
            "--start-unix",
            "1720396800",
            "--speed",
            "60",
        ];
        assert_eq!(resolve(&args(&same), Some(&file)), Ok(from_file));

        let err = resolve(&args(&["--seed", "7"]), Some(&file)).unwrap_err();
        assert_eq!(
            err.to_string(),
            "--seed 7 conflicts with the scenario's seed, 42; drop the flag or change the file"
        );
        let err = resolve(&args(&["--speed", "120"]), Some(&file)).unwrap_err();
        assert_eq!((err.flag, err.field), ("--speed", "speed"));
        let err = resolve(&args(&["--start-unix", "0"]), Some(&file)).unwrap_err();
        assert_eq!(
            (err.given.as_str(), err.stated.as_str()),
            ("0", "1720396800")
        );

        assert_eq!(
            resolve(&args(&[]), None),
            Ok(RunParams {
                seed: DEFAULT_SEED,
                start_unix_s: DEFAULT_START_UNIX_S,
                speed: DEFAULT_SPEED,
            })
        );
    }

    /// A file that states only a seed leaves the other flags their meaning,
    /// and a default never collides with a file.
    #[test]
    fn what_the_file_leaves_out_keeps_its_flag_or_default() {
        let partial = scenario("seed: 9\n");
        assert_eq!(
            resolve(&args(&["--speed", "600"]), Some(&partial)),
            Ok(RunParams {
                seed: 9,
                start_unix_s: DEFAULT_START_UNIX_S,
                speed: 600.0,
            })
        );
        let speed_only = scenario("speed: 1\n");
        assert_eq!(
            resolve(&args(&["--seed", "42"]), Some(&speed_only)),
            Ok(RunParams {
                seed: 42,
                start_unix_s: DEFAULT_START_UNIX_S,
                speed: 1.0,
            })
        );
    }
}
