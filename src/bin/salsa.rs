//! Score breadth-first search or Dijkstra on SALSA-CLRS test sets written by `scripts/salsa.py`.
//!
//! Usage: salsa bfs|dijkstra FILE...
//!
//! Prints one JSON line per file: `graphs`; `node`, the share of nodes whose parent equals the
//! reference parent; `graph`, the share of graphs with every parent equal; `certified`, trees
//! accepted by `check`; `us`, mean microseconds per search (graph construction excluded). Exits 1
//! unless every tree is certified.

use route::graph::{Graph, bfs, check, dijkstra};
use std::time::Instant;
use std::{env, fs, path::Path, process::ExitCode};

fn main() -> ExitCode {
    let mut args = env::args().skip(1);
    let algo = args.next().expect("usage: salsa bfs|dijkstra FILE...");
    let unit = match algo.as_str() {
        "bfs" => true,
        "dijkstra" => false,
        _ => panic!("algorithm {algo:?}: expected bfs or dijkstra"),
    };
    let mut ok = true;
    for f in args {
        let text = fs::read_to_string(&f).unwrap_or_else(|e| panic!("{f}: {e}"));
        let mut lines = text.lines();
        let (mut graphs, mut nodes, mut equal, mut whole, mut certified, mut secs) =
            (0, 0, 0, 0, 0, 0.0);
        while let Some(head) = lines.next() {
            let num = |x: &str| {
                x.parse::<usize>()
                    .unwrap_or_else(|e| panic!("{f}: {x:?}: {e}"))
            };
            let [n, m, s] = head.split(' ').map(num).collect::<Vec<_>>()[..] else {
                panic!("{f}: graph {graphs}: bad header {head:?}");
            };
            let arcs: Vec<(u32, u32, f64)> = (0..m)
                .map(|_| {
                    let x: Vec<&str> = lines.next().unwrap().split(' ').collect();
                    // Weights are 32-bit floats; breadth-first search files carry none.
                    let w = x.get(2).map_or(1.0, |w| w.parse::<f32>().unwrap() as f64);
                    (num(x[0]) as u32, num(x[1]) as u32, w)
                })
                .collect();
            let reference: Vec<u32> = lines
                .next()
                .unwrap()
                .split(' ')
                .map(|x| num(x) as u32)
                .collect();
            let g = Graph::new(n, arcs);
            let r = g.reverse();
            let t0 = Instant::now();
            let t = if unit {
                bfs(&g, &r, s)
            } else {
                dijkstra(&g, s)
            };
            secs += t0.elapsed().as_secs_f64();
            let hit = t.pi.iter().zip(&reference).filter(|(a, b)| a == b).count();
            (nodes, equal, whole) = (nodes + n, equal + hit, whole + (hit == n) as usize);
            certified += check(&g, s, &t, unit) as usize;
            graphs += 1;
        }
        let name = Path::new(&f).file_stem().unwrap().to_string_lossy();
        println!(
            r#"{{"algo":"{algo}","set":"{name}","graphs":{graphs},"node":{:.4},"graph":{:.4},"certified":{certified},"us":{:.2}}}"#,
            equal as f64 / nodes as f64,
            whole as f64 / graphs as f64,
            1e6 * secs / graphs as f64
        );
        ok &= certified == graphs;
    }
    if ok {
        ExitCode::SUCCESS
    } else {
        ExitCode::FAILURE
    }
}
