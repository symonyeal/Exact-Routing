//! Engines that take new metrics on a fixed topology, against Dijkstra on every metric: overlay
//! customization of one partition, CH rebuild and fixed-order re-contraction, CCH customization.

use route::cch::Cch;
use route::ch::Ch;
use route::graph::{Graph, dijkstra};
use route::overlay::{Model, Search, customize, partition};
use route::road::mix;

/// One h×w grid topology (each neighbour pair kept with probability 0.8, as one or two arcs) under
/// k metrics of random positive integer weights, with node coordinates (column, row).
fn grids(h: usize, w: usize, k: usize, z: &mut u64) -> (Vec<Graph>, Vec<(f64, f64)>) {
    let mut arcs = Vec::new();
    for v in 0..h * w {
        for u in [
            (v % w + 1 < w).then(|| v + 1),
            (v + w < h * w).then(|| v + w),
        ]
        .into_iter()
        .flatten()
        {
            if mix(z) % 10 < 8 {
                arcs.push((v as u32, u as u32));
                if mix(z) % 10 < 8 {
                    arcs.push((u as u32, v as u32));
                }
            }
        }
    }
    let gs = (0..k)
        .map(|_| {
            let c: Vec<f64> = arcs.iter().map(|_| (1 + mix(z) % 5000) as f64).collect();
            Graph::new(h * w, arcs.iter().zip(c).map(|(&(u, v), c)| (u, v, c)))
        })
        .collect();
    let xy = (0..h * w)
        .map(|v| ((v % w) as f64, (v / w) as f64))
        .collect();
    (gs, xy)
}

#[test]
fn fixed_preparation_answers_every_metric_exactly() {
    let mut z = 20261125;
    let (gs, xy) = grids(18, 23, 4, &mut z);
    let n = gs[0].n;
    let p = partition(&gs[0], &xy, 40, Model::Hypergraph);
    assert!(
        gs.iter()
            .all(|g| partition(g, &xy, 40, Model::Hypergraph) == p)
    );
    let cch = Cch::new(&gs[0], &xy);
    let order = Ch::new(&gs[0]).order();
    let (mut search, mut ws, mut path) = (Search::new(n), cch.workspace(), Vec::new());
    for g in &gs {
        let o = customize(g, &p, 2);
        let m = cch.customize(g);
        let (full, fixed) = (Ch::new(g), Ch::with_order(g, &order));
        let (mut c1, mut c2) = (full.calculator(), fixed.calculator());
        for _ in 0..200 {
            let (s, t) = (mix(&mut z) as usize % n, mix(&mut z) as usize % n);
            let d = dijkstra(g, s).d[t];
            assert_eq!(search.query(g, &o, s, t), d, "overlay {s}->{t}");
            assert_eq!(full.query(&mut c1, s, t), d, "CH {s}->{t}");
            assert_eq!(fixed.query(&mut c2, s, t), d, "fixed-order CH {s}->{t}");
            assert_eq!(
                cch.query(&m, &mut ws, s, t, Some(&mut path)),
                d,
                "CCH {s}->{t}"
            );
            if d.is_finite() {
                let cost: f64 = path
                    .windows(2)
                    .map(|a| {
                        g.arcs(a[0] as usize)
                            .filter(|&(v, _)| v == a[1])
                            .map(|(_, w)| w)
                            .fold(f64::INFINITY, f64::min)
                    })
                    .sum();
                assert_eq!(
                    (path[0], *path.last().unwrap(), cost),
                    (s as u32, t as u32, d)
                );
            }
        }
    }
}

#[test]
fn tree_paths_price_to_the_tree_distance_on_their_own_metric() {
    let mut z = 3;
    let (gs, _) = grids(9, 11, 2, &mut z);
    for s in [0, 17, 50] {
        let tree = dijkstra(&gs[0], s);
        for t in 0..gs[0].n {
            let cost: f64 = gs[0]
                .path(&tree, t)
                .iter()
                .map(|&a| gs[0].weights()[a])
                .sum();
            let other: f64 = gs[0]
                .path(&tree, t)
                .iter()
                .map(|&a| gs[1].weights()[a])
                .sum();
            if tree.d[t].is_finite() {
                assert_eq!(cost, tree.d[t]);
                assert!(other >= dijkstra(&gs[1], s).d[t], "a path beat the optimum");
            } else {
                assert!(gs[0].path(&tree, t).is_empty());
            }
        }
    }
}
