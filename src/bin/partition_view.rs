//! Export certified query traces and the exact partition used by one saved metro benchmark.
//!
//! Usage: `partition_view FILE RESULTS_JSONL CITY U SCALE`; U is 256, 1024 or 4096 and
//! SCALE is 0 (native seconds) or 1000 (the explicitly rounded integer metric). This diagnostic
//! rebuild is not timed evidence. Each selected distance is checked against an independently
//! certified full Dijkstra tree and the frozen result row before JSON is written.

use route::graph::{Graph, check, dijkstra};
use route::overlay::{Model, Overlay, Search, build_parallel};
use route::road::Road;
use serde_json::{Value, json};
use std::{env, fs, process};

const PRESETS: [usize; 10] = [0, 1, 2, 3, 125, 126, 501, 502, 750, 999];

fn fail(message: impl std::fmt::Display) -> ! {
    eprintln!("partition_view: {message}");
    process::exit(2)
}

fn need(ok: bool, message: impl std::fmt::Display) {
    if !ok {
        fail(message);
    }
}

fn num(row: &Value, key: &str) -> usize {
    row.get(key)
        .and_then(Value::as_u64)
        .and_then(|v| usize::try_from(v).ok())
        .unwrap_or_else(|| fail(format!("invalid {key} in saved results")))
}

fn close(a: f64, b: f64, scale: usize) -> bool {
    a == b
        || (scale == 0
            && a.is_finite()
            && b.is_finite()
            && (a - b).abs() <= 1e-9 * b.abs().max(1.0))
}

fn arc_count(g: &Graph) -> usize {
    (0..g.n).map(|u| g.arcs(u).count()).sum()
}

fn graph_row<'a>(rows: &'a [Value], city: &str, scale: usize) -> &'a Value {
    let found: Vec<_> = rows
        .iter()
        .filter(|r| {
            r["kind"] == "graph" && r["city"] == city && r["scale"].as_u64() == Some(scale as u64)
        })
        .collect();
    need(
        found.len() == 1,
        "expected exactly one matching graph result row",
    );
    found[0]
}

fn build_row<'a>(
    rows: &'a [Value],
    city: &str,
    scale: usize,
    model: &str,
    bound: usize,
) -> &'a Value {
    let method = format!("{model}-{bound}");
    let found: Vec<_> = rows
        .iter()
        .filter(|r| {
            r["kind"] == "build"
                && r["city"] == city
                && r["scale"].as_u64() == Some(scale as u64)
                && r["method"] == method
        })
        .collect();
    need(
        found.len() == 1,
        format!("expected exactly one saved build row for {method}"),
    );
    found[0]
}

fn verify_build(o: &Overlay, saved: &Value) {
    need(
        o.cells == num(saved, "cells"),
        "rebuilt cell count differs from saved build row",
    );
    need(
        o.boundary == num(saved, "boundary"),
        "rebuilt boundary count differs from saved build row",
    );
    need(
        o.arcs() == num(saved, "arcs"),
        "rebuilt overlay arc count differs from saved build row",
    );
    need(
        o.bytes() == num(saved, "array_bytes"),
        "rebuilt array byte count differs from saved build row",
    );
}

fn main() {
    let args: Vec<_> = env::args().skip(1).collect();
    need(
        args.len() == 5,
        "usage: partition_view FILE RESULTS_JSONL CITY U SCALE",
    );
    let bound = args[3]
        .parse::<usize>()
        .unwrap_or_else(|_| fail("invalid U"));
    let scale = args[4]
        .parse::<usize>()
        .unwrap_or_else(|_| fail("invalid SCALE"));
    need(
        [256, 1024, 4096].contains(&bound),
        "U must be 256, 1024 or 4096",
    );
    need([0, 1000].contains(&scale), "SCALE must be 0 or 1000");

    let road_bytes =
        fs::read(&args[0]).unwrap_or_else(|e| fail(format!("cannot read road file: {e}")));
    let Road { g, xy } = Road::decode(&road_bytes)
        .unwrap_or_else(|e| fail(format!("invalid road file: {e}")))
        .with_scale(scale)
        .unwrap_or_else(|e| fail(format!("invalid scaled metric: {e}")));
    let m = arc_count(&g);
    let text = fs::read_to_string(&args[1])
        .unwrap_or_else(|e| fail(format!("cannot read results JSONL: {e}")));
    let rows: Vec<Value> = text
        .lines()
        .filter(|s| !s.trim().is_empty())
        .map(|s| {
            serde_json::from_str(s).unwrap_or_else(|e| fail(format!("invalid results JSON: {e}")))
        })
        .collect();
    let saved = graph_row(&rows, &args[2], scale);
    need(
        num(saved, "nodes") == g.n && num(saved, "arcs") == m,
        "road graph dimensions differ from saved graph row",
    );
    let pairs = saved["pairs"]
        .as_array()
        .unwrap_or_else(|| fail("missing query pairs"));
    let refs = saved["reference"]
        .as_array()
        .unwrap_or_else(|| fail("missing reference distances"));
    need(
        pairs.len() == refs.len() && num(saved, "queries") == pairs.len(),
        "saved query population is inconsistent",
    );

    let graph_saved = build_row(&rows, &args[2], scale, "Graph", bound);
    let hyper_saved = build_row(&rows, &args[2], scale, "Hypergraph", bound);
    // These diagnostic builds are intentionally independent of recorded benchmark timings.
    let (og, oh) = std::thread::scope(|scope| {
        let a = scope.spawn(|| build_parallel(&g, &xy, bound, Model::Graph, 2));
        let b = scope.spawn(|| build_parallel(&g, &xy, bound, Model::Hypergraph, 2));
        (a.join().unwrap(), b.join().unwrap())
    });
    verify_build(&og, graph_saved);
    verify_build(&oh, hyper_saved);

    let mut tail = Vec::with_capacity(m);
    let mut head = Vec::with_capacity(m);
    for u in 0..g.n {
        for (v, _) in g.arcs(u) {
            tail.push(u as u32);
            head.push(v);
        }
    }
    let model_json = |o: &Overlay| {
        json!({
            "cells":o.cells,
            "boundary":o.boundary,
            "overlay_arcs":o.arcs(),
            "array_bytes":o.bytes(),
            "arc_cells":o.arc_cells().collect::<Vec<_>>(),
            "boundary_nodes":o.boundary_nodes().collect::<Vec<_>>(),
        })
    };

    let mut qg = Search::new(g.n);
    let mut qh = Search::new(g.n);
    let mut plain = Search::new(g.n);
    let mut queries = Vec::new();
    for i in PRESETS.into_iter().filter(|&i| i < pairs.len()) {
        let pair = pairs[i]
            .as_array()
            .unwrap_or_else(|| fail("malformed saved query pair"));
        need(pair.len() == 2, "malformed saved query pair");
        let s = pair[0]
            .as_u64()
            .and_then(|v| usize::try_from(v).ok())
            .unwrap_or_else(|| fail("invalid source ID"));
        let t = pair[1]
            .as_u64()
            .and_then(|v| usize::try_from(v).ok())
            .unwrap_or_else(|| fail("invalid target ID"));
        need(s < g.n && t < g.n, "saved query endpoint outside graph");
        let want = refs[i].as_f64();
        need(
            refs[i].is_null() || want.is_some_and(|d| d.is_finite() && d >= 0.0),
            "invalid saved reference distance",
        );
        let want = want.unwrap_or(f64::INFINITY);
        let tree = dijkstra(&g, s);
        need(
            check(&g, s, &tree, false),
            "full Dijkstra certificate failed",
        );
        need(
            close(tree.d[t], want, scale),
            "certified Dijkstra distance differs from frozen reference",
        );
        let plain_d = plain.run(&g, s, Some(t), |_| true, None);
        let graph_d = qg.query(&g, &og, s, t);
        let hyper_d = qh.query(&g, &oh, s, t);
        need(
            close(plain_d, want, scale),
            "plain reusable search differs from reference",
        );
        need(
            close(graph_d, want, scale),
            "Graph overlay query differs from reference",
        );
        need(
            close(hyper_d, want, scale),
            "Hypergraph overlay query differs from reference",
        );
        let trace = |q: &Search, overlay: &Overlay| {
            let settled: Vec<_> = q.settled_nodes().collect();
            need(
                settled.len() == q.settled,
                "settled-node trace count differs from search counter",
            );
            let boundary_settled = settled.iter().filter(|&&v| overlay.is_boundary(v)).count();
            json!({"settled":q.settled,"scanned":q.scanned,
                "boundary_settled":boundary_settled,"interior_settled":q.settled-boundary_settled,
                "settled_nodes":settled})
        };
        let graph_trace = trace(&qg, &og);
        let hyper_trace = trace(&qh, &oh);
        let plain_trace = json!({"settled":plain.settled,"scanned":plain.scanned});
        let category = match i % 4 {
            0 => "walk8",
            1 => "walk32",
            _ => "uniform",
        };
        queries.push(json!({"index":i,"source":s,"target":t,"category":category,
            "distance":want.is_finite().then_some(want),"certified":true,"dijkstra":plain_trace,
            "Graph":graph_trace,"Hypergraph":hyper_trace}));
    }
    println!(
        "{}",
        json!({"schema":1,"city":args[2],"bound":bound,"scale":scale,
        "nodes":g.n,"arcs":m,"tail":tail,"head":head,
        "models":{"Graph":model_json(&og),"Hypergraph":model_json(&oh)},"queries":queries})
    );
}
