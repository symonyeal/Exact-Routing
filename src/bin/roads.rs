//! Exact point-to-point queries on a road graph written by `scripts/roads.py`: plain Dijkstra
//! against two-level overlays from graph and hypergraph partitions.
//!
//! Usage: roads FILE [QUERIES [U...]]   (defaults: 1000 queries; U = 256, 1024, 4096)
//!
//! Prints JSON lines: the graph; plain Dijkstra; then for each model and cell bound U the cells,
//! boundary nodes, overlay arcs, build seconds, per query the mean microseconds, nodes settled and
//! arcs scanned, and the queries whose distance differs from plain Dijkstra's by more than 1e-9
//! relative. Full Dijkstra trees from three sources are certified with `check`. Queries are
//! uniform node pairs, seeded; every method answers each query in turn, so times are paired.
//! Exits 1 if a tree is not certified or a query is wrong.

use route::graph::{check, dijkstra};
use route::overlay::{Model, Search, build};
use route::road::Road;
use std::time::Instant;
use std::{env, fs, process::ExitCode};

fn main() -> ExitCode {
    let args: Vec<String> = env::args().skip(1).collect();
    let file = args.first().expect("usage: roads FILE [QUERIES [U...]]");
    let queries: usize = args.get(1).map_or(1000, |x| x.parse().unwrap());
    let bounds: Vec<usize> = if args.len() > 2 {
        args[2..].iter().map(|x| x.parse().unwrap()).collect()
    } else {
        vec![256, 1024, 4096]
    };
    let b = fs::read(file).unwrap_or_else(|e| panic!("{file}: {e}"));
    let Road { g, xy } = Road::decode(&b).expect("invalid road binary");
    let (n, m) = (g.n, (b.len() - 16 - 16 * g.n) / 16);
    println!(r#"{{"graph":"{file}","nodes":{n},"arcs":{m}}}"#);

    let mut z = 2026u64;
    let mut mix = || {
        z = z.wrapping_add(0x9e3779b97f4a7c15);
        let mut x = z;
        x = (x ^ x >> 30).wrapping_mul(0xbf58476d1ce4e5b9);
        x = (x ^ x >> 27).wrapping_mul(0x94d049bb133111eb);
        ((x ^ x >> 31) % n as u64) as usize
    };
    let certified = (0..3)
        .map(|_| mix())
        .filter(|&s| check(&g, s, &dijkstra(&g, s), false))
        .count();
    let pairs: Vec<(usize, usize)> = (0..queries).map(|_| (mix(), mix())).collect();

    let mut built = Vec::new();
    for model in [Model::Graph, Model::Hypergraph] {
        for &u in &bounds {
            let t0 = Instant::now();
            let o = build(&g, &xy, u, model);
            built.push((model, u, o, t0.elapsed().as_secs_f64()));
        }
    }

    // Method 0 is plain Dijkstra, method j > 0 the overlay built[j - 1]. Each query runs every
    // method, starting from a different one each time, so drift in clock speed is shared.
    let k = built.len() + 1;
    let mut search = Search::new(n);
    let (mut secs, mut settled, mut scanned) = (vec![0.0; k], vec![0; k], vec![0; k]);
    let (mut d, mut wrong, mut reached) = (vec![0.0; k], vec![0; k], 0);
    for (i, &(s, t)) in pairs.iter().enumerate() {
        for j in (0..k).map(|j| (i + j) % k) {
            let t0 = Instant::now();
            d[j] = match j {
                0 => search.run(&g, s, Some(t), |_| true, None),
                _ => search.query(&g, &built[j - 1].2, s, t),
            };
            secs[j] += t0.elapsed().as_secs_f64();
            (settled[j], scanned[j]) = (settled[j] + search.settled, scanned[j] + search.scanned);
        }
        for j in 1..k {
            wrong[j] += !(d[j] == d[0] || (d[j] - d[0]).abs() <= 1e-9 * d[0]) as usize;
        }
        reached += d[0].is_finite() as usize;
    }
    let us = |j: usize| 1e6 * secs[j] / queries as f64;
    println!(
        r#"{{"method":"dijkstra","queries":{queries},"reached":{reached},"query_us":{:.1},"settled":{},"scanned":{},"certified_trees":"{certified}/3"}}"#,
        us(0),
        settled[0] / queries,
        scanned[0] / queries
    );
    for (j, (model, u, o, build_s)) in (1..).zip(&built) {
        println!(
            r#"{{"method":"overlay","model":"{model:?}","u":{u},"cells":{},"boundary":{},"overlay_arcs":{},"build_s":{build_s:.2},"query_us":{:.1},"settled":{},"scanned":{},"wrong":{}}}"#,
            o.cells,
            o.boundary,
            o.arcs(),
            us(j),
            settled[j] / queries,
            scanned[j] / queries,
            wrong[j]
        );
    }
    if certified == 3 && wrong.iter().all(|&x| x == 0) {
        ExitCode::SUCCESS
    } else {
        ExitCode::FAILURE
    }
}
