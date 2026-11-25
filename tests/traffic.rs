//! Engines that take new metrics on a fixed topology, against Dijkstra on every metric: CH rebuild
//! and fixed-order re-contraction, CCH customization (full, partial, threaded), multilevel overlays.

use route::cch::Cch;
use route::ch::Ch;
use route::graph::{Graph, dijkstra};
use route::levels::{Query, nest};
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
    let cch = Cch::new(&gs[0], &xy);
    let order = Ch::new(&gs[0]).order();
    let (mut ws, mut path) = (cch.workspace(), Vec::new());
    for g in &gs {
        let m = cch.customize(g, 1).0;
        let (full, fixed) = (Ch::new(g), Ch::with_order(g, &order));
        let (mut c1, mut c2) = (full.calculator(), fixed.calculator());
        for _ in 0..200 {
            let (s, t) = (mix(&mut z) as usize % n, mix(&mut z) as usize % n);
            let d = dijkstra(g, s).d[t];
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

/// Every CCH distance from a few sources equals Dijkstra's on g.
fn exact(cch: &Cch, m: &route::cch::Metric, g: &Graph, z: &mut u64) {
    let mut ws = cch.workspace();
    for _ in 0..6 {
        let s = mix(z) as usize % g.n;
        let d = dijkstra(g, s).d;
        for (t, &d) in d.iter().enumerate() {
            assert_eq!(cch.query(m, &mut ws, s, t, None), d, "CCH {s}->{t}");
        }
    }
}

#[test]
fn partial_parallel_and_full_customization_give_identical_weights() {
    let mut z = 20261126;
    let (gs, xy) = grids(24, 31, 3, &mut z);
    let cch = Cch::new(&gs[0], &xy);
    for (a, b) in [(0, 1), (1, 2), (2, 0)] {
        let (from, to) = (&gs[a], &gs[b]);
        let full = cch.customize(to, 1).0;
        assert!(full == cch.customize(to, 4).0, "4 threads differ from 1");
        let m = cch.customize(from, 1).0;
        let differ: Vec<usize> = (0..to.weights().len())
            .filter(|&i| from.weights()[i] != to.weights()[i])
            .collect();
        let mut all = m.clone();
        cch.update(&mut all, to, &differ);
        assert!(
            all == full,
            "partial over every changed arc differs from full"
        );
        // A random 1% of the arcs take their new weight.
        let mut changed: Vec<usize> = differ
            .iter()
            .copied()
            .filter(|_| mix(&mut z).is_multiple_of(100))
            .collect();
        changed.push(differ[mix(&mut z) as usize % differ.len()]);
        let mut c = from.weights().to_vec();
        changed.iter().for_each(|&i| c[i] = to.weights()[i]);
        let g = from.with_weights(c.clone());
        let mut part = m.clone();
        cch.update(&mut part, &g, &changed);
        assert!(
            part == cch.customize(&g, 1).0,
            "partial 1% differs from full"
        );
        exact(&cch, &part, &g, &mut z);
    }
}

#[test]
fn partial_update_at_the_top_of_the_elimination_tree_is_exact() {
    let mut z = 11;
    let (gs, xy) = grids(20, 20, 1, &mut z);
    let g = &gs[0];
    let cch = Cch::new(g, &xy);
    let arcs: Vec<(usize, usize)> = (0..g.n)
        .flat_map(|u| g.arcs(u).map(move |(v, _)| (u, v as usize)))
        .collect();
    let top = (0..arcs.len())
        .max_by_key(|&i| cch.rank(arcs[i].0).min(cch.rank(arcs[i].1)))
        .unwrap();
    let mut m = cch.customize(g, 1).0;
    for factor in [50.0, 0.0] {
        let mut c = g.weights().to_vec();
        c[top] = (c[top] * factor).max(1.0);
        let h = g.with_weights(c.clone());
        cch.update(&mut m, &h, &[top]);
        assert!(m == cch.customize(&h, 1).0, "top update x{factor} differs");
        exact(&cch, &m, &h, &mut z);
        // The next round updates from h back towards g's other weights.
        let back = h.with_weights(g.weights().to_vec());
        cch.update(&mut m, &back, &[top]);
        assert!(m == cch.customize(&back, 1).0);
    }
}

#[test]
fn multilevel_overlays_answer_every_metric_exactly() {
    let mut z = 5;
    let (gs, xy) = grids(21, 26, 3, &mut z);
    let n = gs[0].n;
    for us in [vec![16, 64], vec![8, 40, 160]] {
        let p = nest(&gs[0], &xy, &us);
        let mut q = Query::new(n);
        for g in &gs {
            let m = route::levels::customize(g, &p);
            for _ in 0..8 {
                let s = mix(&mut z) as usize % n;
                let d = dijkstra(g, s).d;
                for (t, &d) in d.iter().enumerate() {
                    assert_eq!(q.run(g, &p, &m, s, t), d, "{us:?} {s}->{t}");
                }
            }
        }
    }
}
