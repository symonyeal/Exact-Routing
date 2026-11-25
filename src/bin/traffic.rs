//! What keeping an exact routing index current costs, and what routing on stale times costs.
//!
//! Usage: traffic ROAD METRICS [QUERIES [REPEATS [THREADS]]]; defaults 1000, 3, 2.
//! METRICS holds K travel-time columns over ROAD's arcs (column 0 free flow, then the time slots);
//! every cost is max(1, round(1000 · seconds)), integer milliseconds, so all sums are exact.
//!
//! Reference. For each query (s, t) and each metric τ, a Dijkstra tree from s is certified, and its
//! s–t path is priced on every metric σ: c[τ][σ] = cost on σ of the path chosen on τ. The diagonal
//! c[σ][σ] is the exact optimum; regret(τ→σ) = c[τ][σ] / c[σ][σ] − 1. Trees run on THREADS workers.
//!
//! Timing, one thread. Preparation that reads no weights (overlay partitions, CCH order and
//! elimination) runs once. Then for each pass, in random slot order, each engine in rotated order
//! brings its index to the slot's metric (u, timed: CH full rebuild, CH re-contraction in the
//! free-flow hierarchy's node order, overlay customization of the fixed partition, CCH
//! customization; none for the Dijkstra searches) and answers, in random order,
//! the queries i with (i + pass) mod S = slot position (q, timed, every answer checked). Indexed
//! engines then answer the slot's other queries untimed and checked, so every index answers every
//! query on every slot. Both CH engines and the CCH return a path with each distance (the CCH's
//! paths are re-priced in the untimed checks); the Dijkstra searches and overlays return distances.

use route::cch::Cch;
use route::ch::Ch;
use route::graph::{Graph, check, dijkstra};
use route::overlay::{Model, Overlay, Search, customize, partition};
use route::query::BiSearch;
use route::road::{Road, metrics, pairs, shuffle};
use serde_json::json;
use std::hint::black_box;
use std::time::Instant;
use std::{env, fs, thread};

const SCALE: usize = 1000;
const BOUNDS: [usize; 2] = [1024, 4096];
const ENGINES: [&str; 7] = [
    "dijkstra",
    "bidirectional",
    "overlay-1024",
    "overlay-4096",
    "fast_paths-CH",
    "fast_paths-CH-fixed-order",
    "CCH",
];

/// An engine's index for one metric.
enum Index {
    Plain,
    Overlay(Overlay),
    Ch(Ch, fast_paths::PathCalculator),
    Cch(route::cch::Metric),
}

/// c[τ][σ] for one query, flattened τ-major; infinite when t is unreachable.
fn priced(gs: &[Graph], s: usize, t: usize) -> Vec<f64> {
    let k = gs.len();
    let mut c = vec![0.0; k * k];
    for (tau, g) in gs.iter().enumerate() {
        let tree = dijkstra(g, s);
        assert!(
            check(g, s, &tree, false),
            "uncertified tree {s} metric {tau}"
        );
        let path = g.path(&tree, t);
        for (sigma, h) in gs.iter().enumerate() {
            c[tau * k + sigma] = if tree.d[t].is_finite() {
                path.iter().map(|&a| h.weights()[a]).sum()
            } else {
                f64::INFINITY
            };
        }
        assert_eq!(
            c[tau * k + tau],
            tree.d[t],
            "path cost {s}->{t} metric {tau}"
        );
    }
    c
}

/// Cost of a node path on g, cheapest parallel arc per step; infinite when unreachable or broken.
fn priced_path(g: &Graph, path: &[u32], s: usize, t: usize) -> f64 {
    if path.first() != Some(&(s as u32)) || path.last() != Some(&(t as u32)) {
        return f64::INFINITY;
    }
    path.windows(2)
        .map(|a| {
            g.arcs(a[0] as usize)
                .filter(|&(v, _)| v == a[1])
                .map(|(_, w)| w)
                .fold(f64::INFINITY, f64::min)
        })
        .sum()
}

fn main() {
    let args: Vec<_> = env::args().skip(1).collect();
    let usage = "usage: traffic ROAD METRICS [QUERIES [REPEATS [THREADS]]]";
    let (road, cols) = (
        fs::read(args.first().expect(usage)).expect("read road binary"),
        fs::read(args.get(1).expect(usage)).expect("read metrics"),
    );
    let arg = |i: usize, default| args.get(i).map_or(default, |s| s.parse::<usize>().unwrap());
    let (q, repeats, threads) = (arg(2, 1000), arg(3, 3), arg(4, 2));
    assert!(q > 0 && repeats > 0 && threads > 0);
    let xy = Road::decode(&road).expect("invalid road binary").xy;
    let gs: Vec<Graph> = metrics(&road, &cols, SCALE, |_| true)
        .expect("invalid metrics")
        .into_iter()
        .flatten()
        .collect();
    let (n, k) = (gs[0].n, gs.len());
    assert!(k >= 2, "need free flow and at least one slot");
    let slots = k - 1;
    let pairs = pairs(&gs[0], q);

    let t0 = Instant::now();
    let chunk = q.div_ceil(threads);
    let c: Vec<Vec<f64>> = thread::scope(|scope| {
        let jobs: Vec<_> = pairs
            .chunks(chunk)
            .map(|p| {
                let gs = &gs;
                scope.spawn(move || p.iter().map(|&(s, t)| priced(gs, s, t)).collect::<Vec<_>>())
            })
            .collect();
        jobs.into_iter().flat_map(|j| j.join().unwrap()).collect()
    });
    let reference_s = t0.elapsed().as_secs_f64();
    eprintln!("{n} nodes: {} certified trees in {reference_s:.1} s", q * k);
    let rs: Vec<Graph> = gs.iter().map(Graph::reverse).collect();
    println!(
        "{}",
        json!({"kind":"graph", "nodes":n, "arcs":gs[0].weights().len(), "metrics":k, "queries":q,
        "repeats":repeats, "threads":threads, "scale":SCALE, "seed":20261123, "order_seed":20261125,
        "metric":"max(1, round(scale * seconds))", "certified_trees":q * k, "reference_s":reference_s,
        "pairs":pairs, "identical_endpoints":pairs.iter().filter(|(s,t)|s==t).count(),
        "reachable":c.iter().filter(|c|c[0].is_finite()).count(),
        "costs":c.iter().map(|c|c[0].is_finite().then(||c.clone())).collect::<Vec<_>>()})
    );

    let mut parts = Vec::new();
    for u in BOUNDS {
        let t0 = Instant::now();
        let p = partition(&gs[0], &xy, u, Model::Hypergraph);
        let seconds = t0.elapsed().as_secs_f64();
        eprintln!("overlay-{u} partition {seconds:.1} s");
        println!(
            "{}",
            json!({"kind":"prepare", "method":format!("overlay-{u}"), "seconds":seconds,
            "metric_independent":true})
        );
        parts.push(p);
    }
    let t0 = Instant::now();
    let order = Ch::new(&gs[0]).order();
    println!(
        "{}",
        json!({"kind":"prepare", "method":"fast_paths-CH-fixed-order", "seconds":t0.elapsed().as_secs_f64(),
        "metric_independent":false, "order_metric":0})
    );
    eprintln!("free-flow CH order {:.1} s", t0.elapsed().as_secs_f64());
    let t0 = Instant::now();
    let cch = Cch::new(&gs[0], &xy);
    eprintln!("CCH preparation {:.1} s", t0.elapsed().as_secs_f64());
    println!(
        "{}",
        json!({"kind":"prepare", "method":"CCH", "seconds":t0.elapsed().as_secs_f64(),
        "edges":cch.edges(), "tree_height":cch.height(), "metric_independent":true})
    );

    let update = |e: usize, g: &Graph| match e {
        0 | 1 => Index::Plain,
        2 | 3 => Index::Overlay(customize(g, &parts[e - 2], 1)),
        4 | 5 => {
            let ch = if e == 4 {
                Ch::new(g)
            } else {
                Ch::with_order(g, &order)
            };
            let calc = ch.calculator();
            Index::Ch(ch, calc)
        }
        _ => Index::Cch(cch.customize(g, 1).0),
    };
    let (mut search, mut bi, mut ws) = (Search::new(n), BiSearch::new(n), cch.workspace());
    let mut path = Vec::new();
    let mut answer = |e: usize, x: &mut Index, j: usize, s: usize, t: usize| match x {
        Index::Plain if e == 0 => search.run(&gs[j], s, Some(t), |_| true, None),
        Index::Plain => bi.query(&gs[j], &rs[j], s, t),
        Index::Overlay(o) => search.query(&gs[j], o, s, t),
        Index::Ch(ch, calc) => ch.query(calc, s, t),
        Index::Cch(m) => cch.query(m, &mut ws, s, t, Some(&mut path)),
    };
    let (mut ws2, mut route) = (cch.workspace(), Vec::new());
    let mut z = 20261125u64;
    for pass in 0..repeats {
        let mut order: Vec<usize> = (0..slots).collect();
        shuffle(&mut order, &mut z);
        for (step, &slot) in order.iter().enumerate() {
            let j = slot + 1;
            let mut mine: Vec<usize> = (0..q).filter(|i| (i + pass) % slots == slot).collect();
            shuffle(&mut mine, &mut z);
            for e in (0..ENGINES.len()).map(|e| (e + pass + step) % ENGINES.len()) {
                let t0 = Instant::now();
                let mut x = black_box(update(e, &gs[j]));
                let u = t0.elapsed().as_secs_f64();
                let t0 = Instant::now();
                for &i in &mine {
                    let (s, t) = pairs[i];
                    let got = black_box(answer(e, &mut x, j, s, t));
                    assert_eq!(got, c[i][j * k + j], "{} {s}->{t} slot {slot}", ENGINES[e]);
                }
                let seconds = t0.elapsed().as_secs_f64();
                if e >= 2 {
                    for i in (0..q).filter(|i| (i + pass) % slots != slot) {
                        let (s, t) = pairs[i];
                        let got = answer(e, &mut x, j, s, t);
                        assert_eq!(got, c[i][j * k + j], "{} {s}->{t} slot {slot}", ENGINES[e]);
                        if let Index::Cch(m) = &x {
                            cch.query(m, &mut ws2, s, t, Some(&mut route));
                            assert_eq!(priced_path(&gs[j], &route, s, t), got, "CCH path {s}->{t}");
                        }
                    }
                }
                eprintln!(
                    "pass {pass} slot {slot} {}: update {u:.3} s, {} queries {seconds:.3} s",
                    ENGINES[e],
                    mine.len()
                );
                println!(
                    "{}",
                    json!({"kind":"slot", "method":ENGINES[e], "pass":pass, "slot":slot,
                    "update_s":if e < 2 {0.0} else {u}, "queries":mine.len(), "query_s":seconds,
                    "checked":if e < 2 {mine.len()} else {q}, "wrong":0})
                );
            }
        }
    }
}
