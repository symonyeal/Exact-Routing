//! Controlled replay of frozen query pairs to measure previous-query cleanup effects.
//!
//! Usage: query_order ROAD RESULTS_JSONL CITY. Native seconds, three passes, plain
//! Dijkstra and Hypergraph U=1024. Preparation in the precleared condition is timed
//! separately and includes an identity query; its cost is never treated as a speedup.

use route::graph::{check, dijkstra};
use route::overlay::{Model, Search, build_parallel};
use route::road::Road;
use serde_json::{Value, json};
use std::{env, fs, hint::black_box, time::Instant};

fn median(values: impl IntoIterator<Item = f64>) -> f64 {
    let mut x: Vec<_> = values.into_iter().collect();
    x.sort_unstable_by(f64::total_cmp);
    x[(x.len() - 1) / 2]
}

fn equal(a: f64, b: f64) -> bool {
    a == b || (a.is_finite() && b.is_finite() && (a - b).abs() <= 1e-9 * b.abs().max(1.0))
}

fn main() {
    let args: Vec<_> = env::args().skip(1).collect();
    assert_eq!(args.len(), 3, "usage: query_order ROAD RESULTS_JSONL CITY");
    let Road { g, xy } = Road::decode(&fs::read(&args[0]).unwrap()).unwrap();
    let rows: Vec<Value> = fs::read_to_string(&args[1])
        .unwrap()
        .lines()
        .filter(|s| !s.trim().is_empty())
        .map(|s| serde_json::from_str(s).unwrap())
        .collect();
    let record = rows
        .iter()
        .find(|r| r["kind"] == "graph" && r["city"] == args[2] && r["scale"] == 0)
        .unwrap();
    assert_eq!(record["nodes"].as_u64().unwrap() as usize, g.n);
    let pairs: Vec<(usize, usize)> = record["pairs"]
        .as_array()
        .unwrap()
        .iter()
        .map(|p| {
            (
                p[0].as_u64().unwrap() as usize,
                p[1].as_u64().unwrap() as usize,
            )
        })
        .collect();
    let expected: Vec<_> = record["reference"]
        .as_array()
        .unwrap()
        .iter()
        .map(|v| v.as_f64().unwrap_or(f64::INFINITY))
        .collect();
    let n = pairs.len();
    assert_eq!(n, 1000);
    assert_eq!(expected.len(), n);
    for source in [0, g.n / 2, g.n - 1] {
        assert!(check(&g, source, &dijkstra(&g, source), false));
    }
    let overlay = build_parallel(&g, &xy, 1024, Model::Hypergraph, 2);
    let repeats = 3;
    let mut elapsed = vec![vec![vec![0.0; n * repeats]; 2]; 3];
    let mut prepared = elapsed.clone();
    let mut previous = vec![vec![vec![0usize; n * repeats]; 2]; 3];
    let mut counts = vec![vec![(0, 0); n]; 2];
    for rep in 0..repeats {
        for condition in (0..3).map(|c| (c + rep) % 3) {
            let order: Vec<_> = if condition == 1 {
                (0..n).rev().collect()
            } else {
                (0..n).collect()
            };
            let mut searches = [Search::new(g.n), Search::new(g.n)];
            for &i in order.iter().take(16) {
                let (s, t) = pairs[i];
                searches[0].run(&g, s, Some(t), |_| true, None);
                searches[1].query(&g, &overlay, s, t);
            }
            for &i in &order {
                let (s, t) = pairs[i];
                for method in (0..2).map(|j| (j + i + rep) % 2) {
                    let search = &mut searches[method];
                    let at = rep * n + i;
                    previous[condition][method][at] = search.touched_count();
                    if condition == 2 {
                        let start = Instant::now();
                        let zero = if method == 0 {
                            search.run(&g, s, Some(s), |_| true, None)
                        } else {
                            search.query(&g, &overlay, s, s)
                        };
                        prepared[condition][method][at] = start.elapsed().as_secs_f64() * 1e6;
                        assert_eq!(zero, 0.0);
                        assert_eq!(search.touched_count(), 1);
                    }
                    let start = Instant::now();
                    let got = black_box(if method == 0 {
                        search.run(&g, s, Some(t), |_| true, None)
                    } else {
                        search.query(&g, &overlay, s, t)
                    });
                    elapsed[condition][method][at] = start.elapsed().as_secs_f64() * 1e6;
                    assert!(equal(got, expected[i]), "query {i}");
                    let count = (search.settled, search.scanned);
                    if rep == 0 && condition == 0 {
                        counts[method][i] = count;
                    } else {
                        assert_eq!(
                            counts[method][i], count,
                            "search work changed for query {i}"
                        );
                    }
                }
            }
        }
    }
    let mut results = Vec::new();
    for (condition, label) in ["fixed", "reversed", "precleared"].iter().enumerate() {
        for (method, name) in ["dijkstra", "Hypergraph-1024"].iter().enumerate() {
            let means: Vec<_> = (0..n)
                .map(|i| {
                    (0..repeats)
                        .map(|r| elapsed[condition][method][r * n + i])
                        .sum::<f64>()
                        / repeats as f64
                })
                .collect();
            let prep: Vec<_> = (0..n)
                .map(|i| {
                    (0..repeats)
                        .map(|r| prepared[condition][method][r * n + i])
                        .sum::<f64>()
                        / repeats as f64
                })
                .collect();
            let touched: Vec<_> = (0..n)
                .map(|i| {
                    (0..repeats)
                        .map(|r| previous[condition][method][r * n + i] as f64)
                        .sum::<f64>()
                        / repeats as f64
                })
                .collect();
            let mut groups = Vec::new();
            for (group, category) in ["walk8", "walk32", "uniform"].iter().enumerate() {
                let indices: Vec<_> = (0..n)
                    .filter(|i| {
                        if group < 2 {
                            i % 4 == group
                        } else {
                            i % 4 >= 2
                        }
                    })
                    .collect();
                groups.push(json!({"category":category,"count":indices.len(),
                    "median_query_us":median(indices.iter().map(|&i|means[i])),
                    "median_preparation_us":median(indices.iter().map(|&i|prep[i])),
                    "median_total_us":median(indices.iter().map(|&i|means[i]+prep[i])),
                    "median_previous_touched":median(indices.iter().map(|&i|touched[i]))}));
            }
            results.push(json!({"condition":label,"method":name,"groups":groups,
                "mean_query_us":means.iter().sum::<f64>()/n as f64,
                "mean_preparation_us":prep.iter().sum::<f64>()/n as f64,
                "per_query_mean_us":means,"per_query_preparation_us":prep,
                "per_query_previous_touched":touched,"per_query_work":counts[method],"wrong":0}));
        }
    }
    println!(
        "{}",
        json!({"schema":1,"city":args[2],"scale":0,"nodes":g.n,
        "arcs":(0..g.n).map(|u|g.arcs(u).count()).sum::<usize>(),"queries":n,"repeats":repeats,
        "bound":1024,"source_binary_sha256":record["binary_sha256"],"source_benchmark_sha":record["machine"]["git_sha"],
        "definition":"Precleared preparation includes an identity query outside the actual query timer; total cost includes preparation.",
        "same_current_query_work":true,"results":results})
    );
}
