//! Paired latency, preprocessing and parallel throughput on metropolitan road networks.
//!
//! Usage: metro FILE [QUERIES [THREADS [SCALE [REPEATS]]]]; defaults 1000, 2, 0, 3.
//! SCALE=0 retains native seconds. Positive SCALE explicitly changes each arc cost to
//! max(1, round(SCALE*seconds)), allowing exact comparison with integer contraction hierarchies.
//! Queries alternate 8-arc walks, 32-arc walks and two uniform node pairs, seed 20261123.
//! Every reference query uses a full independently certified tree. Latency is single-threaded;
//! throughput uses separate persistent worker state and includes thread launch and join overhead.

use route::ch::Ch;
use route::graph::{Graph, check, dijkstra};
use route::overlay::{Model, Overlay, Search, build_parallel};
use route::query::BiSearch;
use route::road::Road;
use serde_json::json;
use std::hint::black_box;
use std::time::Instant;
use std::{env, fs, thread};

enum Engine<'a> {
    Dijkstra(Search),
    Bidirectional(BiSearch),
    Overlay(&'a Overlay, Search),
    Ch(&'a Ch, fast_paths::PathCalculator),
}

impl Engine<'_> {
    fn query(&mut self, g: &Graph, r: &Graph, s: usize, t: usize) -> f64 {
        match self {
            Self::Dijkstra(q) => q.run(g, s, Some(t), |_| true, None),
            Self::Bidirectional(q) => q.query(g, r, s, t),
            Self::Overlay(o, q) => q.query(g, o, s, t),
            Self::Ch(ch, q) => ch.query(q, s, t),
        }
    }

    fn counts(&self) -> Option<(usize, usize)> {
        match self {
            Self::Dijkstra(q) | Self::Overlay(_, q) => Some((q.settled, q.scanned)),
            Self::Bidirectional(q) => Some((q.settled, q.scanned)),
            Self::Ch(..) => None,
        }
    }
}

fn engine<'a>(j: usize, n: usize, overlays: &'a [Overlay], ch: Option<&'a Ch>) -> Engine<'a> {
    match j {
        0 => Engine::Dijkstra(Search::new(n)),
        1 => Engine::Bidirectional(BiSearch::new(n)),
        j if j < overlays.len() + 2 => Engine::Overlay(&overlays[j - 2], Search::new(n)),
        _ => {
            let ch = ch.unwrap();
            Engine::Ch(ch, ch.calculator())
        }
    }
}

fn mix(z: &mut u64) -> u64 {
    *z = z.wrapping_add(0x9e3779b97f4a7c15);
    let mut x = *z;
    x = (x ^ x >> 30).wrapping_mul(0xbf58476d1ce4e5b9);
    x = (x ^ x >> 27).wrapping_mul(0x94d049bb133111eb);
    x ^ x >> 31
}

fn equal(a: f64, b: f64, integer: bool) -> bool {
    a == b
        || (!integer && a.is_finite() && b.is_finite() && (a - b).abs() <= 1e-9 * b.abs().max(1.0))
}

fn percentile(x: &[f64], p: f64) -> f64 {
    let mut x = x.to_vec();
    x.sort_unstable_by(f64::total_cmp);
    x[((p * x.len() as f64).ceil() as usize).max(1) - 1]
}

fn main() {
    let args: Vec<_> = env::args().skip(1).collect();
    let file = args
        .first()
        .expect("usage: metro FILE [QUERIES [THREADS [SCALE [REPEATS]]]]");
    let arg = |i: usize, default| args.get(i).map_or(default, |s| s.parse::<usize>().unwrap());
    let (q, threads, scale, repeats) = (arg(1, 1000), arg(2, 2), arg(3, 0), arg(4, 3));
    assert!(q > 0 && threads > 0 && repeats > 0);
    let bytes = fs::read(file).expect("read road binary");
    let Road { g, xy } = Road::decode(&bytes)
        .expect("invalid road binary")
        .with_scale(scale)
        .expect("invalid road metric");
    let n = g.n;
    let t0 = Instant::now();
    let r = g.reverse();
    let reverse_s = t0.elapsed().as_secs_f64();
    let mut z = 20261123;
    let pairs: Vec<_> = (0..q)
        .map(|i| {
            let s = mix(&mut z) as usize % n;
            let mut t = mix(&mut z) as usize % n;
            if i % 4 < 2 {
                t = s;
                for _ in 0..if i % 4 == 0 { 8 } else { 32 } {
                    let arcs: Vec<_> = g.arcs(t).collect();
                    if arcs.is_empty() {
                        break;
                    }
                    t = arcs[mix(&mut z) as usize % arcs.len()].0 as usize;
                }
            }
            (s, t)
        })
        .collect();
    for s in [0, n / 2, n - 1] {
        assert!(
            check(&g, s, &dijkstra(&g, s), false),
            "uncertified source {s}"
        );
    }
    let expected: Vec<_> = pairs
        .iter()
        .map(|&(s, t)| {
            let tree = dijkstra(&g, s);
            assert!(check(&g, s, &tree, false), "uncertified query source {s}");
            tree.d[t]
        })
        .collect();
    let sidecar = std::path::Path::new(file).with_extension("meta.json");
    let source = fs::read(sidecar)
        .ok()
        .map(|b| serde_json::from_slice::<serde_json::Value>(&b).expect("invalid source metadata"));
    println!(
        "{}",
        json!({"kind":"graph", "file":file, "nodes":n, "arcs":(0..n).map(|u|g.arcs(u).count()).sum::<usize>(), "queries":q, "repeats":repeats,
        "threads":threads, "available_threads":thread::available_parallelism().map_or(1,usize::from),
        "seed":20261123, "scale":scale, "metric":if scale==0 {"native seconds"} else {"max(1, round(scale * seconds))"},
        "source":source, "reverse_s":reverse_s, "certified_trees":3, "certified_queries":q,
        "pairs":pairs, "reference":expected.iter().map(|d|d.is_finite().then_some(*d)).collect::<Vec<_>>(),
        "warmup_queries":q.min(16), "native_tolerance":1e-9,
        "reachable":expected.iter().filter(|x| x.is_finite()).count(), "identical_endpoints":pairs.iter().filter(|(s,t)|s==t).count()})
    );

    let mut overlays = Vec::new();
    let mut names = vec!["dijkstra".to_string(), "bidirectional".to_string()];
    for model in [Model::Graph, Model::Hypergraph] {
        for u in [256, 1024, 4096] {
            let name = format!("{model:?}-{u}");
            let (mut serial, mut parallel) = (None, None);
            let (mut serial_trials, mut parallel_trials) = (Vec::new(), Vec::new());
            for rep in 0..repeats {
                for is_parallel in [rep % 2 == 0, rep % 2 != 0] {
                    let t0 = Instant::now();
                    let o =
                        build_parallel(&g, &xy, u, model, if is_parallel { threads } else { 1 });
                    let seconds = t0.elapsed().as_secs_f64();
                    if is_parallel {
                        parallel_trials.push(seconds);
                        parallel = Some(o);
                    } else {
                        serial_trials.push(seconds);
                        serial = Some(o);
                    }
                }
            }
            let (serial, parallel) = (serial.unwrap(), parallel.unwrap());
            let serial_s = percentile(&serial_trials, 0.5);
            let parallel_s = percentile(&parallel_trials, 0.5);
            assert_eq!(
                (serial.cells, serial.boundary, serial.arcs()),
                (parallel.cells, parallel.boundary, parallel.arcs())
            );
            let (mut a, mut b) = (Search::new(n), Search::new(n));
            for (&(s, t), &want) in pairs.iter().zip(&expected) {
                let x = a.query(&g, &serial, s, t);
                let y = b.query(&g, &parallel, s, t);
                assert_eq!(x, y, "serial/parallel {name} {s}->{t}");
                assert!(
                    equal(x, want, scale > 0),
                    "incorrect {name} {s}->{t}: {x} vs {want}"
                );
            }
            println!(
                "{}",
                json!({"kind":"build", "method":name, "serial_s":serial_s, "parallel_s":parallel_s,
                "serial_trials_s":serial_trials, "parallel_trials_s":parallel_trials,
                "threads":threads, "cells":parallel.cells, "boundary":parallel.boundary,
                "arcs":parallel.arcs(), "array_bytes":parallel.bytes(), "parity_queries":q, "wrong":0})
            );
            names.push(name);
            overlays.push(parallel);
        }
    }
    let ch = (scale > 0).then(|| {
        let t0 = Instant::now();
        let ch = Ch::new(&g);
        println!(
            "{}",
            json!({"kind":"build", "method":"fast_paths-CH", "serial_s":t0.elapsed().as_secs_f64(),
            "arcs":ch.arcs(), "query_includes_path_reconstruction":true})
        );
        ch
    });
    if ch.is_some() {
        names.push("fast_paths-CH".to_string());
    }
    let k = names.len();
    let mut engines: Vec<_> = (0..k)
        .map(|j| engine(j, n, &overlays, ch.as_ref()))
        .collect();
    for engine in &mut engines {
        for &(s, t) in pairs.iter().take(16) {
            black_box(engine.query(&g, &r, s, t));
        }
    }
    let mut us = vec![vec![0.0; q * repeats]; k];
    let mut counts = vec![(0usize, 0usize); k];
    for rep in 0..repeats {
        for (i, &(s, t)) in pairs.iter().enumerate() {
            for j in (0..k).map(|j| (i + rep + j) % k) {
                let t0 = Instant::now();
                let got = black_box(engines[j].query(&g, &r, s, t));
                us[j][rep * q + i] = t0.elapsed().as_secs_f64() * 1e6;
                assert!(
                    equal(got, expected[i], scale > 0),
                    "{} {s}->{t}: {got} vs {}",
                    names[j],
                    expected[i]
                );
                if let Some((s, a)) = engines[j].counts() {
                    counts[j].0 += s;
                    counts[j].1 += a;
                }
            }
        }
    }
    let per_query: Vec<Vec<f64>> = us
        .iter()
        .map(|samples| {
            (0..q)
                .map(|i| (0..repeats).map(|r| samples[r * q + i]).sum::<f64>() / repeats as f64)
                .collect()
        })
        .collect();
    for j in 0..k {
        let samples = &per_query[j];
        let group = |which: usize| {
            samples
                .iter()
                .enumerate()
                .filter(|(i, _)| {
                    if which == 2 {
                        i % 4 >= 2
                    } else {
                        i % 4 == which
                    }
                })
                .map(|(_, v)| *v)
                .collect::<Vec<_>>()
        };
        let grouped = |which| {
            let x = group(which);
            (!x.is_empty()).then(|| percentile(&x, 0.5))
        };
        println!(
            "{}",
            json!({"kind":"latency", "method":names[j], "queries":q, "repeats":repeats,
            "mean_us":samples.iter().sum::<f64>()/q as f64, "p50_us":percentile(samples,0.5), "p95_us":percentile(samples,0.95),
            "walk8_p50_us":grouped(0), "walk32_p50_us":grouped(1), "uniform_p50_us":grouped(2),
            "trial_mean_us":us[j].chunks(q).map(|v|v.iter().sum::<f64>()/q as f64).collect::<Vec<_>>(),
            "settled":engines[j].counts().map(|_| counts[j].0/(q*repeats)),
            "scanned":engines[j].counts().map(|_| counts[j].1/(q*repeats)), "wrong":0,
            "paired_ratio_to_ch":ch.as_ref().map(|_|percentile(&samples.iter().zip(&per_query[k-1]).map(|(a,b)|a/b).collect::<Vec<_>>(),0.5))})
        );
    }
    drop(engines);
    for (j, name) in names.iter().enumerate() {
        for workers in [1, threads]
            .into_iter()
            .take(if threads == 1 { 1 } else { 2 })
        {
            let workers = workers.min(q);
            let mut seconds = Vec::new();
            for _ in 0..repeats {
                let mut engines: Vec<_> = (0..workers)
                    .map(|_| engine(j, n, &overlays, ch.as_ref()))
                    .collect();
                let t0 = Instant::now();
                let wrong: usize = thread::scope(|scope| {
                    let jobs: Vec<_> = engines
                        .iter_mut()
                        .enumerate()
                        .map(|(c, engine)| {
                            let lo = c * q / workers;
                            let pairs = &pairs[lo..(c + 1) * q / workers];
                            let (g, r, expected) = (&g, &r, &expected);
                            scope.spawn(move || {
                                pairs
                                    .iter()
                                    .enumerate()
                                    .filter(|(i, p)| {
                                        !equal(
                                            black_box(engine.query(g, r, p.0, p.1)),
                                            expected[lo + i],
                                            scale > 0,
                                        )
                                    })
                                    .count()
                            })
                        })
                        .collect();
                    jobs.into_iter().map(|job| job.join().unwrap()).sum()
                });
                seconds.push(t0.elapsed().as_secs_f64());
                assert_eq!(wrong, 0, "parallel {name}");
            }
            println!(
                "{}",
                json!({"kind":"throughput", "method":name, "threads":workers, "queries":q,
                "repeats":repeats, "median_qps":q as f64/percentile(&seconds,0.5), "trial_seconds":seconds,"wrong":0})
            );
        }
    }
}
