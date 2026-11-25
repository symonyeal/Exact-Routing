//! How exact routing engines scale with network size, and what partial CCH updates cost.
//!
//! Usage
//!   scale ROAD METRICS time QUERIES PASSES SLOTS THREADS
//!   scale ROAD METRICS partial QUERIES THREADS OUT
//!   scale ROAD METRICS export QUERIES THREADS OUT
//!   scale ROAD METRICS memory QUERIES ENGINE
//! METRICS holds 17 travel-time columns over ROAD's arcs: free flow, then the eight weekday and the
//! eight weekend slots in the order of `scripts/scale.py`. Every cost is max(1, round(1000 ·
//! seconds)), integer milliseconds. PASSES and SLOTS are comma lists, pass indices and weekday slot
//! positions 0–7, naming one shard of the timing population.
//!
//! time. As `traffic`, with more engines. Pass p visits the weekday slots in a random order drawn
//! from seed 20261125 + p; the shard takes those it names. At each, every engine in rotated order
//! brings its index to the slot's metric (u, timed) and answers, in random order, the queries i
//! with (i + p) mod 8 = slot (q, timed); indexed engines then answer the slot's other queries
//! untimed. Every answer equals the distance of a certified Dijkstra tree on its metric. CCH-partial
//! updates the metric of the slot it visited before in the shard (free flow first), given the arcs
//! whose weight changed; finding them is not timed, since a feed lists them. CCH-4-threads reports
//! core-seconds (summed busy time of its threads) and wall seconds.
//!
//! partial. For each of three passes, in random order, each update a → b: the weekday and the
//! weekend slots in sequence (consecutive) and their observed slots in sequence (observed), and
//! synthetic updates that give a random fraction f of the arcs of an observed weekday slot the
//! weight of the next observed slot. Time a full customization of b and a partial update from a's
//! metric, alternating which runs first, and require bit-identical weights. On dataset slots every
//! query is answered from the partial metric and checked; on synthetic metrics the first CHECK
//! queries, in pass 0, against trees computed on that metric. OUT receives the graph, metrics,
//! workload, reference distances and synthetic change sets for the RoutingKit reference.
//!
//! export. Only OUT, with certified distances on the weekday metrics: input for RoutingKit's CH.
//!
//! memory. Peak resident memory (Linux VmHWM) after preparing ENGINE, bringing it to the am_peak
//! metric and answering QUERIES checked queries; ENGINE `none` loads the graphs alone.

use fast_paths::PathCalculator;
use route::cch::{Cch, Metric, Perfect};
use route::ch::Ch;
use route::graph::{Graph, Search, check, dijkstra};
use route::levels::{Multi, Nest, Query, nest};
use route::query::BiSearch;
use route::road::{Road, metrics, mix, pairs, shuffle};
use serde_json::{Value, json};
use std::hint::black_box;
use std::time::Instant;
use std::{env, fs, thread};

const SCALE: usize = 1000;
const SLOTS: usize = 8;
const ENGINES: [&str; 10] = [
    "dijkstra",
    "bidirectional",
    "fast_paths-CH",
    "fast_paths-CH-fixed-order",
    "CCH",
    "CCH-4-threads",
    "CCH-partial",
    "CCH-perfect",
    "overlay-1024-32768",
    "overlay-256-4096-65536",
];
const NESTS: [&[usize]; 2] = [&[1024, 32768], &[256, 4096, 65536]];
/// Synthetic changed-arc fractions.
const F: [f64; 5] = [0.001, 0.01, 0.1, 0.3, 0.6];
/// Weekday observed slots (am_peak, midday, pm_peak, offpeak) as metric indices.
const OBSERVED: [usize; 4] = [1, 3, 5, 7];
const CHECK: usize = 10;

/// Metric-independent preparation of the engines asked for, except the CH order (free flow).
#[derive(Default)]
struct Prep {
    nests: Vec<Option<Nest>>,
    order: Option<Vec<usize>>,
    cch: Option<Cch>,
}

/// An engine's index for one metric.
enum Index {
    Plain,
    Ch(Ch, PathCalculator),
    Cch(Metric),
    Perfect(Perfect, Metric),
    Multi(Multi),
}

/// Query state shared by the engines.
struct Work {
    search: Search,
    bi: BiSearch,
    ws: Option<route::cch::Workspace>,
    multi: Query,
    path: Vec<u32>,
}

/// Prepare the engines e with need(e), printing one row per preparation.
fn prepare(need: impl Fn(usize) -> bool, free: &Graph, xy: &[(f64, f64)]) -> Prep {
    let mut p = Prep::default();
    let row = |method: &str, t0: Instant, extra: Value| {
        let mut r =
            json!({"kind":"prepare", "method":method, "seconds":t0.elapsed().as_secs_f64()});
        r.as_object_mut()
            .unwrap()
            .extend(extra.as_object().unwrap().clone());
        println!("{r}");
        eprintln!("prepare {method} {:.1} s", t0.elapsed().as_secs_f64());
    };
    for (i, &us) in NESTS.iter().enumerate() {
        p.nests.push(need(8 + i).then(|| {
            let t0 = Instant::now();
            let x = nest(free, xy, us);
            let boundary: Vec<usize> = (0..x.levels()).map(|l| x.boundary(l)).collect();
            row(
                ENGINES[8 + i],
                t0,
                json!({"metric_independent":true, "boundary":boundary}),
            );
            x
        }));
    }
    if need(3) {
        let t0 = Instant::now();
        p.order = Some(Ch::new(free).order());
        row(
            ENGINES[3],
            t0,
            json!({"metric_independent":false, "order_metric":0}),
        );
    }
    if (4..8).any(&need) {
        let t0 = Instant::now();
        let cch = Cch::new(free, xy);
        row(
            "CCH",
            t0,
            json!({"metric_independent":true, "edges":cch.edges(), "tree_height":cch.height(),
                "triangles":cch.triangles()}),
        );
        p.cch = Some(cch);
    }
    p
}

/// Bring engine e to the metric of g: the index, its core-seconds when the engine measures them
/// itself, and extra row fields. `carry` is CCH-partial's metric and `changed` the arcs that differ.
fn update(
    e: usize,
    p: &Prep,
    g: &Graph,
    carry: &mut Option<Metric>,
    changed: &[usize],
) -> (Index, Option<f64>, Value) {
    let cch = || p.cch.as_ref().unwrap();
    match e {
        0 | 1 => (Index::Plain, None, json!({})),
        2 | 3 => {
            let ch = match &p.order {
                Some(order) if e == 3 => Ch::with_order(g, order),
                _ => Ch::new(g),
            };
            let calc = ch.calculator();
            (Index::Ch(ch, calc), None, json!({}))
        }
        4 | 5 => {
            let (m, core) = cch().customize(g, if e == 4 { 1 } else { 4 });
            (Index::Cch(m), Some(core), json!({}))
        }
        6 => {
            let mut m = carry.take().unwrap();
            let recomputed = cch().update(&mut m, g, changed);
            let f = changed.len() as f64 / g.weights().len() as f64;
            (
                Index::Cch(m),
                None,
                json!({"f":f, "changed":changed.len(), "recomputed":recomputed}),
            )
        }
        7 => {
            let (m, basic) = cch().customize(g, 1);
            let t0 = Instant::now();
            let x = cch().perfect(&m);
            let perfect = t0.elapsed().as_secs_f64();
            let edges = x.edges();
            (
                Index::Perfect(x, m),
                None,
                json!({"basic_s":basic, "perfect_s":perfect, "kept":edges}),
            )
        }
        _ => {
            let m = route::levels::customize(g, p.nests[e - 8].as_ref().unwrap());
            let arcs = m.arcs();
            (Index::Multi(m), None, json!({"overlay_arcs":arcs}))
        }
    }
}

/// Engine e's distance from s to t on metric graph g (reverse r).
#[allow(clippy::too_many_arguments)]
fn answer(
    e: usize,
    x: &mut Index,
    p: &Prep,
    w: &mut Work,
    g: &Graph,
    r: &Graph,
    s: usize,
    t: usize,
) -> f64 {
    match x {
        Index::Plain if e == 0 => w.search.run(g, s, Some(t), |_| true),
        Index::Plain => w.bi.query(g, r, s, t),
        Index::Ch(ch, calc) => ch.query(calc, s, t),
        Index::Cch(m) => {
            let ws = w.ws.as_mut().unwrap();
            p.cch
                .as_ref()
                .unwrap()
                .query(m, ws, s, t, Some(&mut w.path))
        }
        Index::Perfect(q, m) => {
            let ws = w.ws.as_mut().unwrap();
            p.cch
                .as_ref()
                .unwrap()
                .query_perfect(q, m, ws, s, t, Some(&mut w.path))
        }
        Index::Multi(m) => w.multi.run(g, p.nests[e - 8].as_ref().unwrap(), m, s, t),
    }
}

/// Distances d(s, t) of certified Dijkstra trees on g, one tree per pair, on `threads` workers.
fn reference(g: &Graph, pairs: &[(usize, usize)], threads: usize) -> Vec<f64> {
    let chunk = pairs.len().div_ceil(threads).max(1);
    thread::scope(|scope| {
        let jobs: Vec<_> = pairs
            .chunks(chunk)
            .map(|p| {
                scope.spawn(move || {
                    p.iter()
                        .map(|&(s, t)| {
                            let tree = dijkstra(g, s);
                            assert!(check(g, s, &tree), "uncertified tree from {s}");
                            tree.d[t]
                        })
                        .collect::<Vec<_>>()
                })
            })
            .collect();
        jobs.into_iter().flat_map(|j| j.join().unwrap()).collect()
    })
}

/// Cost of a node path on g, cheapest parallel arc per step; infinite when broken.
fn priced(g: &Graph, path: &[u32], s: usize, t: usize) -> f64 {
    if path.first() != Some(&(s as u32)) || path.last() != Some(&(t as u32)) {
        return f64::INFINITY;
    }
    path.windows(2)
        .map(|a| {
            g.arcs(a[0] as usize)
                .filter(|&(v, _)| v == a[1])
                .fold(f64::INFINITY, |c, (_, w)| c.min(w))
        })
        .sum()
}

/// Arcs whose weight differs between g and h.
fn differ(g: &Graph, h: &Graph) -> Vec<usize> {
    let (a, b) = (g.weights(), h.weights());
    (0..a.len()).filter(|&i| a[i] != b[i]).collect()
}

/// A /proc/self/status field in bytes (Linux: VmRSS resident, VmHWM peak resident), else None.
fn status(key: &str) -> Option<u64> {
    let s = fs::read_to_string("/proc/self/status").ok()?;
    let line = s.lines().find(|l| l.starts_with(key))?;
    Some(line.split_whitespace().nth(1)?.parse::<u64>().ok()? * 1024)
}

fn list(s: &str) -> Vec<usize> {
    s.split(',').map(|x| x.parse().unwrap()).collect()
}

fn main() {
    let args: Vec<String> = env::args().skip(1).collect();
    let usage = "usage: scale ROAD METRICS (time QUERIES PASSES SLOTS THREADS | partial QUERIES \
                 THREADS OUT | export QUERIES THREADS OUT | memory QUERIES ENGINE)";
    assert!(args.len() >= 5, "{usage}");
    let road = fs::read(&args[0]).expect("read road binary");
    let cols = fs::read(&args[1]).expect("read metrics");
    let xy = Road::decode(&road).expect("invalid road binary").xy;
    let q: usize = args[3].parse().expect(usage);
    match args[2].as_str() {
        "time" => {
            assert!(args.len() == 7, "{usage}");
            let (passes, slots) = (list(&args[4]), list(&args[5]));
            time(
                &road,
                &cols,
                &xy,
                q,
                &passes,
                &slots,
                args[6].parse().unwrap(),
            );
        }
        "partial" | "export" => {
            assert!(args.len() == 6, "{usage}");
            let study = args[2] == "partial";
            partial(
                &road,
                &cols,
                &xy,
                q,
                args[4].parse().unwrap(),
                &args[5],
                study,
            );
        }
        "memory" => {
            let e = ENGINES.iter().position(|&x| x == args[4]);
            assert!(
                e.is_some() || args[4] == "none",
                "unknown engine {}",
                args[4]
            );
            memory(&road, &cols, &xy, e, q.min(1000));
        }
        _ => panic!("{usage}"),
    }
}

/// Load the metric graphs j with keep(j), checking there are 1 + 2 · SLOTS columns.
fn load(road: &[u8], cols: &[u8], keep: impl Fn(usize) -> bool) -> Vec<Option<Graph>> {
    let gs = metrics(road, cols, SCALE, keep).expect("invalid metrics");
    assert_eq!(gs.len(), 1 + 2 * SLOTS, "need free flow and 16 slots");
    gs
}

fn graph_row(g: &Graph, pairs: &[(usize, usize)], extra: Value) {
    let mut r = json!({"kind":"graph", "nodes":g.n, "arcs":g.weights().len(), "queries":pairs.len(),
        "scale":SCALE, "seed":20261123, "metric":"max(1, round(scale * seconds))", "pairs":pairs,
        "identical_endpoints":pairs.iter().filter(|(s, t)| s == t).count()});
    r.as_object_mut()
        .unwrap()
        .extend(extra.as_object().unwrap().clone());
    println!("{r}");
}

fn time(
    road: &[u8],
    cols: &[u8],
    xy: &[(f64, f64)],
    q: usize,
    passes: &[usize],
    slots: &[usize],
    threads: usize,
) {
    assert!(slots.iter().all(|&s| s < SLOTS) && threads > 0);
    let gs = load(road, cols, |j| j == 0 || slots.iter().any(|&s| s + 1 == j));
    let g = |j: usize| gs[j].as_ref().unwrap();
    let rs: Vec<Option<Graph>> = gs
        .iter()
        .enumerate()
        .map(|(j, x)| x.as_ref().filter(|_| j > 0).map(Graph::reverse))
        .collect();
    let pairs = pairs(g(0), q);
    let t0 = Instant::now();
    let refs: Vec<Option<Vec<f64>>> = gs
        .iter()
        .enumerate()
        .map(|(j, x)| {
            x.as_ref()
                .filter(|_| j > 0)
                .map(|x| reference(x, &pairs, threads))
        })
        .collect();
    let reference_s = t0.elapsed().as_secs_f64();
    eprintln!(
        "{} nodes: {} certified trees in {reference_s:.1} s",
        g(0).n,
        q * slots.len()
    );
    let reachable: Vec<usize> = refs
        .iter()
        .flatten()
        .map(|d| d.iter().filter(|d| d.is_finite()).count())
        .collect();
    graph_row(
        g(0),
        &pairs,
        json!({"mode":"time", "passes":passes, "slots":slots, "threads":threads,
        "order_seed":20261125, "certified_trees":q * slots.len(), "reference_s":reference_s,
        "reachable":reachable}),
    );
    let p = prepare(|_| true, g(0), xy);
    let n = g(0).n;
    let mut w = Work {
        search: Search::new(n),
        bi: BiSearch::new(n),
        ws: p.cch.as_ref().map(Cch::workspace),
        multi: Query::new(n),
        path: Vec::new(),
    };
    let cch = p.cch.as_ref().unwrap();
    for &pass in passes {
        let mut z = 20261125 + pass as u64;
        let mut order: Vec<usize> = (0..SLOTS).collect();
        shuffle(&mut order, &mut z);
        let mut carry: (usize, Option<Metric>) = (0, None);
        for (step, &slot) in order.iter().enumerate() {
            let mut mine: Vec<usize> = (0..q).filter(|i| (i + pass) % SLOTS == slot).collect();
            shuffle(&mut mine, &mut z);
            if !slots.contains(&slot) {
                continue;
            }
            let j = slot + 1;
            let (gj, rj, d) = (g(j), rs[j].as_ref().unwrap(), refs[j].as_ref().unwrap());
            for e in (0..ENGINES.len()).map(|e| (e + pass + step) % ENGINES.len()) {
                let mut changed = Vec::new();
                if e == 6 {
                    if carry.1.is_none() {
                        carry = (0, Some(cch.customize(g(0), 1).0));
                    }
                    changed = differ(g(carry.0), gj);
                }
                let from = carry.0;
                let t0 = Instant::now();
                let (x, core, extra) = update(e, &p, gj, &mut carry.1, &changed);
                let wall = t0.elapsed().as_secs_f64();
                let mut x = black_box(x);
                let t0 = Instant::now();
                for &i in &mine {
                    let (s, t) = pairs[i];
                    let got = black_box(answer(e, &mut x, &p, &mut w, gj, rj, s, t));
                    assert_eq!(got, d[i], "{} {s}->{t} slot {slot}", ENGINES[e]);
                }
                let seconds = t0.elapsed().as_secs_f64();
                if e >= 2 {
                    for i in (0..q).filter(|i| (i + pass) % SLOTS != slot) {
                        let (s, t) = pairs[i];
                        let got = answer(e, &mut x, &p, &mut w, gj, rj, s, t);
                        assert_eq!(got, d[i], "{} {s}->{t} slot {slot}", ENGINES[e]);
                        // The CCH engines return a path with each distance: re-price it.
                        if (4..8).contains(&e) && got.is_finite() {
                            let c = priced(gj, &w.path, s, t);
                            assert_eq!(c, got, "{} path {s}->{t}", ENGINES[e]);
                        }
                    }
                }
                if let (6, Index::Cch(m)) = (e, x) {
                    carry = (j, Some(m));
                }
                let (u, uw) = if e < 2 {
                    (0.0, 0.0)
                } else {
                    (core.unwrap_or(wall), wall)
                };
                eprintln!(
                    "pass {pass} slot {slot} {}: update {u:.3} s, {} queries {seconds:.3} s",
                    ENGINES[e],
                    mine.len()
                );
                let mut r = json!({"kind":"slot", "method":ENGINES[e], "pass":pass, "slot":slot,
                    "update_s":u, "update_wall_s":uw, "queries":mine.len(), "query_s":seconds,
                    "checked":if e < 2 {mine.len()} else {q}, "wrong":0});
                if e == 6 {
                    r["from"] = json!(from);
                }
                r.as_object_mut()
                    .unwrap()
                    .extend(extra.as_object().unwrap().clone());
                println!("{r}");
            }
        }
    }
}

/// The updates of the partial study: (sequence, from, to, f), f = None for a dataset slot. Metric
/// indices: 1–8 weekday, 9–16 weekend.
fn updates() -> Vec<(&'static str, usize, usize, Option<f64>)> {
    let mut u = Vec::new();
    let names = [
        (0, "weekday-consecutive", "weekday-observed"),
        (SLOTS, "weekend-consecutive", "weekend-observed"),
    ];
    for (base, consecutive, observed) in names {
        for j in 1..=SLOTS {
            u.push((consecutive, base + j, base + j % SLOTS + 1, None));
        }
        for (i, &a) in OBSERVED.iter().enumerate() {
            let b = OBSERVED[(i + 1) % OBSERVED.len()];
            u.push((observed, base + a, base + b, None));
        }
    }
    for (i, &a) in OBSERVED.iter().enumerate() {
        for f in F {
            u.push(("synthetic", a, OBSERVED[(i + 1) % OBSERVED.len()], Some(f)));
        }
    }
    u
}

/// The synthetic change set of update (a → b, f): round(f · m) of the arcs that differ, at random.
fn synthetic(diff: &[usize], m: usize, a: usize, f: f64) -> Vec<usize> {
    let mut z = 20261126 + 1000 * a as u64 + (f * 1e4) as u64;
    let k = ((f * m as f64).round() as usize).min(diff.len());
    let mut d = diff.to_vec();
    for i in 0..k {
        let j = i + mix(&mut z) as usize % (d.len() - i);
        d.swap(i, j);
    }
    d.truncate(k);
    d.sort_unstable();
    d
}

/// The partial-update study, or with `study` false only the RoutingKit input (`export`), which
/// needs certified distances on the weekday metrics alone.
fn partial(
    road: &[u8],
    cols: &[u8],
    xy: &[(f64, f64)],
    q: usize,
    threads: usize,
    out: &str,
    study: bool,
) {
    let gs: Vec<Graph> = load(road, cols, |_| true).into_iter().flatten().collect();
    let pairs = pairs(&gs[0], q);
    let t0 = Instant::now();
    let last = if study { 2 * SLOTS } else { SLOTS };
    let refs: Vec<Vec<f64>> = gs
        .iter()
        .enumerate()
        .map(|(j, g)| {
            if j == 0 || j > last {
                Vec::new()
            } else {
                reference(g, &pairs, threads)
            }
        })
        .collect();
    let reference_s = t0.elapsed().as_secs_f64();
    let reachable: Vec<usize> = refs[1..=last]
        .iter()
        .map(|d| d.iter().filter(|d| d.is_finite()).count())
        .collect();
    graph_row(
        &gs[0],
        &pairs,
        json!({"mode":if study {"partial"} else {"export"}, "threads":threads, "certified_trees":q * last,
        "reference_s":reference_s, "reachable":reachable}),
    );
    let (m, ups) = (gs[0].weights().len(), updates());
    let sets: Vec<Vec<usize>> = ups
        .iter()
        .map(|&(_, a, b, f)| {
            let d = differ(&gs[a], &gs[b]);
            f.map_or(d.clone(), |f| synthetic(&d, m, a, f))
        })
        .collect();
    write_routingkit(out, &gs, xy, &pairs, &refs, &ups, &sets);
    if !study {
        return;
    }
    let p = prepare(|e| e == 4, &gs[0], xy);
    let cch = p.cch.as_ref().unwrap();
    let mut ws = cch.workspace();
    let mut z = 20261127u64;
    for pass in 0..3 {
        let mut order: Vec<usize> = (0..ups.len()).collect();
        shuffle(&mut order, &mut z);
        for (step, &k) in order.iter().enumerate() {
            let (sequence, a, b, f) = ups[k];
            let changed = &sets[k];
            let target = match f {
                None => None,
                Some(_) => {
                    let mut c = gs[a].weights().to_vec();
                    changed.iter().for_each(|&i| c[i] = gs[b].weights()[i]);
                    Some(gs[a].with_weights(c))
                }
            };
            let to = target.as_ref().unwrap_or(&gs[b]);
            let source = cch.customize(&gs[a], 1).0;
            let mut part = source.clone();
            let (mut full, mut full_s, mut part_s, mut recomputed) = (None, 0.0, 0.0, 0);
            for side in [(step + pass) % 2, (step + pass + 1) % 2] {
                let t0 = Instant::now();
                if side == 0 {
                    full = Some(black_box(cch.customize(to, 1).0));
                    full_s = t0.elapsed().as_secs_f64();
                } else {
                    recomputed = black_box(cch.update(&mut part, to, changed));
                    part_s = t0.elapsed().as_secs_f64();
                }
            }
            assert!(
                full.as_ref() == Some(&part),
                "partial differs from full: {sequence} {a}->{b} {f:?}"
            );
            let checked = match f {
                None => {
                    for (i, &(s, t)) in pairs.iter().enumerate() {
                        assert_eq!(
                            cch.query(&part, &mut ws, s, t, None),
                            refs[b][i],
                            "{sequence} {a}->{b} {s}->{t}"
                        );
                    }
                    q
                }
                Some(_) if pass == 0 => {
                    let d = reference(to, &pairs[..CHECK.min(q)], threads);
                    for (i, &(s, t)) in pairs[..d.len()].iter().enumerate() {
                        assert_eq!(
                            cch.query(&part, &mut ws, s, t, None),
                            d[i],
                            "synthetic {a}->{b} {f:?} {s}->{t}"
                        );
                    }
                    d.len()
                }
                Some(_) => 0,
            };
            eprintln!(
                "pass {pass} {sequence} {a}->{b} f={:.4}: full {full_s:.3} s, partial {part_s:.3} s",
                changed.len() as f64 / m as f64
            );
            println!(
                "{}",
                json!({"kind":"update", "pass":pass, "sequence":sequence, "from":a, "to":b,
                "target_f":f, "changed":changed.len(), "f":changed.len() as f64 / m as f64,
                "recomputed":recomputed, "edges":cch.edges(), "full_s":full_s, "partial_s":part_s,
                "identical":true, "checked":checked, "wrong":0})
            );
        }
    }
}

/// The RoutingKit input, little-endian: n, m: u64; tail, head: u32[m] in CSR order; lat, lon:
/// f32[n] (the projected coordinates); the eight weekday metrics, u32[8][m] ms; q: u64; pairs
/// u32[2q]; certified distances u32[8][q] (u32::MAX unreachable); the synthetic updates: count:
/// u64, each from, to: u32, f: f64, len: u64, arcs: u32[len] (CSR indices).
fn write_routingkit(
    out: &str,
    gs: &[Graph],
    xy: &[(f64, f64)],
    pairs: &[(usize, usize)],
    refs: &[Vec<f64>],
    ups: &[(&str, usize, usize, Option<f64>)],
    sets: &[Vec<usize>],
) {
    let g = &gs[0];
    let mut b: Vec<u8> = Vec::new();
    let u64s = |b: &mut Vec<u8>, x: u64| b.extend(x.to_le_bytes());
    let u32s = |b: &mut Vec<u8>, x: u32| b.extend(x.to_le_bytes());
    u64s(&mut b, g.n as u64);
    u64s(&mut b, g.weights().len() as u64);
    let arcs: Vec<(u32, u32)> = (0..g.n)
        .flat_map(|u| g.arcs(u).map(move |(v, _)| (u as u32, v)))
        .collect();
    arcs.iter().for_each(|a| u32s(&mut b, a.0));
    arcs.iter().for_each(|a| u32s(&mut b, a.1));
    xy.iter().for_each(|p| b.extend((p.1 as f32).to_le_bytes()));
    xy.iter().for_each(|p| b.extend((p.0 as f32).to_le_bytes()));
    for gj in &gs[1..=SLOTS] {
        gj.weights().iter().for_each(|&w| u32s(&mut b, w as u32));
    }
    u64s(&mut b, pairs.len() as u64);
    pairs.iter().for_each(|&(s, t)| {
        u32s(&mut b, s as u32);
        u32s(&mut b, t as u32);
    });
    for d in &refs[1..=SLOTS] {
        d.iter()
            .for_each(|&d| u32s(&mut b, if d.is_finite() { d as u32 } else { u32::MAX }));
    }
    let syn: Vec<usize> = (0..ups.len()).filter(|&k| ups[k].3.is_some()).collect();
    u64s(&mut b, syn.len() as u64);
    for k in syn {
        let (_, a, c, f) = ups[k];
        u32s(&mut b, a as u32);
        u32s(&mut b, c as u32);
        b.extend(f.unwrap().to_le_bytes());
        u64s(&mut b, sets[k].len() as u64);
        sets[k].iter().for_each(|&i| u32s(&mut b, i as u32));
    }
    fs::write(out, b).expect("write RoutingKit input");
}

fn memory(road: &[u8], cols: &[u8], xy: &[(f64, f64)], e: Option<usize>, q: usize) {
    let gs = load(road, cols, |j| j <= 1);
    let (free, g) = (gs[0].as_ref().unwrap(), gs[1].as_ref().unwrap());
    let r = g.reverse();
    let pairs = pairs(free, q);
    let d = reference(g, &pairs, 1);
    // Reset the peak to the current resident set (Linux 4.0+), so the peak is this engine's.
    let _ = fs::write("/proc/self/clear_refs", "5");
    let base = status("VmRSS:");
    let mut row = json!({"kind":"memory", "method":e.map_or("none", |e| ENGINES[e]), "queries":q,
        "baseline_rss_bytes":base});
    if let Some(e) = e {
        let p = prepare(|x| x == e, free, xy);
        let mut carry = p.cch.as_ref().map(|c| c.customize(free, 1).0);
        let changed = differ(free, g);
        let n = g.n;
        let mut w = Work {
            search: Search::new(n),
            bi: BiSearch::new(n),
            ws: p.cch.as_ref().map(Cch::workspace),
            multi: Query::new(n),
            path: Vec::new(),
        };
        let mut x = update(e, &p, g, &mut carry, &changed).0;
        for (i, &(s, t)) in pairs.iter().enumerate() {
            assert_eq!(
                answer(e, &mut x, &p, &mut w, g, &r, s, t),
                d[i],
                "{} {s}->{t}",
                ENGINES[e]
            );
        }
    }
    row["peak_rss_bytes"] = json!(status("VmHWM:"));
    println!("{row}");
}
