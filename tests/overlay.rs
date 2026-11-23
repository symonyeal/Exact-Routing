//! Overlay queries against plain Dijkstra on random weighted grids, under both partition models.

use route::graph::{Graph, dijkstra};
use route::overlay::{Model, Search, build};

/// splitmix64 (Steele, Lea and Flood 2014).
fn mix(z: &mut u64) -> u64 {
    *z = z.wrapping_add(0x9e3779b97f4a7c15);
    let mut x = *z;
    x = (x ^ x >> 30).wrapping_mul(0xbf58476d1ce4e5b9);
    x = (x ^ x >> 27).wrapping_mul(0x94d049bb133111eb);
    x ^ x >> 31
}

/// An h×w grid keeping each neighbour pair with probability 0.8, as one or two arcs of random
/// weight, with node coordinates (column, row).
fn grid(h: usize, w: usize, z: &mut u64) -> (Graph, Vec<(f64, f64)>) {
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
                let c = 1.0 + (mix(z) % 1000) as f64 / 100.0;
                arcs.push((v as u32, u as u32, c));
                if mix(z) % 10 < 8 {
                    arcs.push((u as u32, v as u32, c));
                }
            }
        }
    }
    let xy = (0..h * w)
        .map(|v| ((v % w) as f64, (v / w) as f64))
        .collect();
    (Graph::new(h * w, arcs), xy)
}

#[test]
fn overlay_distances_equal_dijkstra() {
    let mut z = 11;
    for (h, w) in [(12, 12), (20, 35)] {
        let (g, xy) = grid(h, w, &mut z);
        for model in [Model::Graph, Model::Hypergraph] {
            for u in [8, 40, 1000] {
                let o = build(&g, &xy, u, model);
                let mut search = Search::new(g.n);
                for _ in 0..60 {
                    let (s, t) = (mix(&mut z) as usize % g.n, mix(&mut z) as usize % g.n);
                    let d0 = dijkstra(&g, s).d[t];
                    let d = search.query(&g, &o, s, t);
                    assert!(
                        d == d0 || (d - d0).abs() <= 1e-9 * d0,
                        "{model:?} u {u}: {s}->{t}: {d} vs {d0}"
                    );
                }
            }
        }
    }
}
