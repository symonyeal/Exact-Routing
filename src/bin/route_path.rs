//! A certified path for the local HTML explorer; no benchmark timing is returned.
//!
//! Usage: route_path FILE SOURCE TARGET SCALE, with SCALE 0 (seconds) or 1000
//! (max(1, round(1000 * seconds))). The node IDs are those in the source binary.

use route::graph::{Graph, check, dijkstra};
use route::road::Road;
use serde_json::{Value, json};
use std::{env, fs};

fn answer(g: &Graph, source: usize, target: usize, scale: usize) -> Result<Value, String> {
    if source >= g.n || target >= g.n {
        return Err("endpoint is outside the network".into());
    }
    let tree = dijkstra(g, source);
    if !check(g, source, &tree, false) {
        return Err("the shortest-path certificate failed".into());
    }
    let distance = tree.d[target];
    let mut path = Vec::new();
    if distance.is_finite() {
        let mut v = target;
        loop {
            path.push(v);
            if v == source {
                break;
            }
            if path.len() > g.n {
                return Err("invalid path parent chain".into());
            }
            v = tree.pi[v] as usize;
        }
        path.reverse();
    }
    Ok(json!({
        "source":source, "target":target, "scale":scale,
        "distance":distance.is_finite().then_some(distance), "path":path,
        "certified":true, "units":if scale == 0 { "seconds" } else { "milliseconds" },
        "nodes":g.n, "arcs":(0..g.n).map(|u|g.arcs(u).count()).sum::<usize>()
    }))
}

fn run() -> Result<Value, String> {
    let args: Vec<_> = env::args().skip(1).collect();
    if args.len() != 4 {
        return Err("usage: route_path FILE SOURCE TARGET SCALE".into());
    }
    let number = |i: usize| {
        args[i]
            .parse::<usize>()
            .map_err(|_| "invalid integer argument".to_owned())
    };
    let (source, target, scale) = (number(1)?, number(2)?, number(3)?);
    if !matches!(scale, 0 | 1000) {
        return Err("scale must be 0 or 1000".into());
    }
    let bytes = fs::read(&args[0]).map_err(|_| "could not read the road binary")?;
    let Road { g, .. } = Road::decode(&bytes)?.with_scale(scale)?;
    answer(&g, source, target, scale)
}

fn main() {
    match run() {
        Ok(value) => println!("{value}"),
        Err(message) => {
            eprintln!("{message}");
            std::process::exit(1);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn returns_certified_directed_paths_and_mathematical_special_cases() {
        let g = Graph::new(
            5,
            [
                (0, 1, 8.0),
                (0, 1, 2.0),
                (1, 2, 0.0),
                (2, 3, 3.0),
                (0, 3, 9.0),
            ],
        );
        let path = answer(&g, 0, 3, 0).unwrap();
        assert_eq!(path["path"], json!([0, 1, 2, 3]));
        assert_eq!(path["distance"], 5.0);
        assert_eq!(path["certified"], true);
        let unreachable = answer(&g, 3, 0, 0).unwrap();
        assert_eq!(unreachable["path"], json!([]));
        assert_eq!(unreachable["distance"], Value::Null);
        assert_eq!(answer(&g, 4, 4, 0).unwrap()["path"], json!([4]));
        assert_eq!(answer(&g, 4, 4, 0).unwrap()["distance"], 0.0);
        assert!(answer(&g, 5, 0, 0).is_err());
    }
}
