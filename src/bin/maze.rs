//! Score the solver on Maze-Hard CSV files (`source,question,answer,rating`, square grids).
//!
//! Prints one JSON line per file: `rows`; `certified`, outcomes proved by `check`; `exact`, rendered
//! paths equal to the reference answer; `rated`, rows with rating = D + 1. Exits 1 unless every
//! outcome is certified.

use route::grid::{Maze, check, potential, render, solve};
use std::{env, fs, path::Path, process::ExitCode};

fn main() -> ExitCode {
    let mut ok = true;
    for f in env::args().skip(1) {
        let text = fs::read_to_string(&f).unwrap_or_else(|e| panic!("{f}: {e}"));
        let (mut rows, mut certified, mut exact, mut rated) = (0, 0, 0, 0);
        for line in text.lines().skip(1) {
            let [_, q, a, rating] = line.split(',').collect::<Vec<_>>()[..] else {
                panic!("{f}: row {rows} does not have four fields");
            };
            let n = q.len().isqrt();
            let m =
                Maze::parse(q.as_bytes(), n, n).unwrap_or_else(|e| panic!("{f}: row {rows}: {e}"));
            let x = solve(&m);
            certified += check(&m, x.as_deref(), &potential(&m)) as usize;
            exact += x
                .as_ref()
                .is_some_and(|x| render(q.as_bytes(), x) == a.as_bytes())
                as usize;
            rated += x.as_ref().is_some_and(|x| rating.parse() == Ok(x.len())) as usize;
            rows += 1;
        }
        let name = Path::new(&f).file_name().unwrap().to_string_lossy();
        println!(
            r#"{{"file":"{name}","rows":{rows},"certified":{certified},"exact":{exact},"rated":{rated}}}"#
        );
        ok &= certified == rows;
    }
    if ok {
        ExitCode::SUCCESS
    } else {
        ExitCode::FAILURE
    }
}
