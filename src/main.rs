use std::env;
use std::path::Path;
use std::process::ExitCode;

use venue_shortlist::load::load_venues;

fn main() -> ExitCode {
    let Some(path) = env::args().nth(1) else {
        eprintln!("usage: venue-shortlist <venues.json>");
        return ExitCode::from(2);
    };
    match load_venues(Path::new(&path)) {
        Ok(loaded) => {
            println!("Loaded {} venues from {path}", loaded.venues.len());
            for warning in &loaded.warnings {
                println!("warning: {warning}");
            }
            ExitCode::SUCCESS
        }
        Err(err) => {
            eprintln!("error: {err}");
            ExitCode::FAILURE
        }
    }
}
