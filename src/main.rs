use std::env;
use std::path::PathBuf;
use std::process::ExitCode;

use venue_shortlist::evaluate::{shortlist, shortlisted_in_all, Evaluation};
use venue_shortlist::load::load_venues;
use venue_shortlist::report::{render, Scenario};
use venue_shortlist::request::{EventRequest, HEADCOUNT_SCENARIOS};

const USAGE: &str = "usage: venue-shortlist [PATH] [--headcount N]\n\
  PATH defaults to venues.json. Without --headcount, both 80 and 120 are run.";

struct Args {
    path: PathBuf,
    headcount: Option<u32>,
}

enum ParsedArgs {
    Run(Args),
    Help,
}

fn parse_args(args: impl Iterator<Item = String>) -> Result<ParsedArgs, String> {
    let mut path = None;
    let mut headcount = None;
    let mut args = args;
    while let Some(arg) = args.next() {
        match arg.as_str() {
            "-h" | "--help" => return Ok(ParsedArgs::Help),
            "--headcount" => {
                let value = args.next().ok_or("--headcount needs a number")?;
                let n: u32 = value.parse().ok().filter(|n| *n > 0).ok_or_else(|| {
                    format!("--headcount must be a positive number, got {value:?}")
                })?;
                headcount = Some(n);
            }
            flag if flag.starts_with('-') => return Err(format!("unknown option {flag}")),
            _ if path.is_some() => return Err(format!("unexpected argument {arg:?}")),
            _ => path = Some(PathBuf::from(arg)),
        }
    }
    Ok(ParsedArgs::Run(Args {
        path: path.unwrap_or_else(|| PathBuf::from("venues.json")),
        headcount,
    }))
}

fn main() -> ExitCode {
    let args = match parse_args(env::args().skip(1)) {
        Ok(ParsedArgs::Run(args)) => args,
        Ok(ParsedArgs::Help) => {
            println!("{USAGE}");
            return ExitCode::SUCCESS;
        }
        Err(message) => {
            eprintln!("error: {message}\n{USAGE}");
            return ExitCode::from(2);
        }
    };
    let loaded = match load_venues(&args.path) {
        Ok(loaded) => loaded,
        Err(err) => {
            eprintln!("error: {err}");
            return ExitCode::FAILURE;
        }
    };
    for warning in &loaded.warnings {
        eprintln!("warning: {warning}");
    }

    let headcounts = match args.headcount {
        Some(n) => vec![n],
        None => HEADCOUNT_SCENARIOS.to_vec(),
    };
    let scenarios: Vec<Scenario> = headcounts
        .into_iter()
        .map(|headcount| {
            let request = EventRequest::partner_day(headcount);
            let evaluations = shortlist(&loaded.venues, &request);
            Scenario {
                request,
                evaluations,
            }
        })
        .collect();
    let rankings: Vec<&[Evaluation]> = scenarios.iter().map(|s| &s.evaluations[..]).collect();
    let works_for_all = shortlisted_in_all(&rankings);
    print!("{}", render(&scenarios, &works_for_all));
    ExitCode::SUCCESS
}

#[cfg(test)]
mod tests {
    use super::*;

    fn parse(args: &[&str]) -> Result<ParsedArgs, String> {
        parse_args(args.iter().map(|a| a.to_string()))
    }

    #[test]
    fn defaults_to_venues_json_and_both_scenarios() {
        let Ok(ParsedArgs::Run(args)) = parse(&[]) else {
            panic!("expected a run");
        };
        assert_eq!(args.path, PathBuf::from("venues.json"));
        assert_eq!(args.headcount, None);
    }

    #[test]
    fn path_and_headcount_in_any_order() {
        let Ok(ParsedArgs::Run(args)) = parse(&["--headcount", "80", "data.json"]) else {
            panic!("expected a run");
        };
        assert_eq!(args.path, PathBuf::from("data.json"));
        assert_eq!(args.headcount, Some(80));
    }

    #[test]
    fn bad_arguments_are_errors() {
        for bad in [
            &["--headcount"][..],
            &["--headcount", "zero"],
            &["--headcount", "0"],
            &["--verbose"],
            &["a.json", "b.json"],
        ] {
            assert!(parse(bad).is_err(), "{bad:?}");
        }
        assert!(matches!(parse(&["--help"]), Ok(ParsedArgs::Help)));
    }
}
