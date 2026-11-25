//! Breadth-first search and Dijkstra against oracles that share no code with them: Bellman–Ford
//! relaxation to a fixed point, and the parent rule applied to those distances.

use route::graph::{Graph, Tree, check, dijkstra};

/// splitmix64 (Steele, Lea and Flood 2014).
fn mix(z: &mut u64) -> u64 {
    *z = z.wrapping_add(0x9e3779b97f4a7c15);
    let mut x = *z;
    x = (x ^ x >> 30).wrapping_mul(0xbf58476d1ce4e5b9);
    x = (x ^ x >> 27).wrapping_mul(0x94d049bb133111eb);
    x ^ x >> 31
}

/// A random directed graph: each ordered pair is an arc with probability p, weight in [1, 2).
fn random(n: usize, p: f64, z: &mut u64) -> Vec<(u32, u32, f64)> {
    let mut arcs = Vec::new();
    for i in 0..n * n {
        if ((mix(z) % 1_000_000) as f64) < p * 1e6 {
            arcs.push((
                (i / n) as u32,
                (i % n) as u32,
                1.0 + (mix(z) % 1000) as f64 / 1000.0,
            ));
        }
    }
    arcs
}

/// Distances from s by Bellman–Ford relaxation to a fixed point.
fn bellman(n: usize, arcs: &[(u32, u32, f64)], s: usize, unit: bool) -> Vec<f64> {
    let mut d = vec![f64::INFINITY; n];
    d[s] = 0.0;
    for _ in 0..n {
        for &(u, v, w) in arcs.iter().filter(|a| a.0 != a.1) {
            let c = d[u as usize] + if unit { 1.0 } else { w };
            if c < d[v as usize] {
                d[v as usize] = c;
            }
        }
    }
    d
}

#[test]
fn dijkstra_matches_relaxation() {
    let mut z = 5;
    for (n, p) in [(1, 0.0), (8, 0.4), (50, 0.08), (200, 0.02)] {
        for _ in 0..20 {
            let arcs = random(n, p, &mut z);
            let g = Graph::new(n, arcs.iter().copied());
            let s = mix(&mut z) as usize % n;
            let t = dijkstra(&g, s);
            let d = bellman(n, &arcs, s, false);
            assert!(
                t.d.iter()
                    .zip(&d)
                    .all(|(a, b)| a == b || (a - b).abs() <= 1e-12 * b),
                "n {n}"
            );
            assert!(check(&g, s, &t));
        }
    }
}

#[test]
fn check_rejects_wrong_trees() {
    // 0 → 1 → 2 and a longer detour 0 → 3 → 2.
    let g = Graph::new(4, [(0, 1, 1.0), (1, 2, 1.0), (0, 3, 1.5), (3, 2, 1.5)]);
    let t = dijkstra(&g, 0);
    assert_eq!(
        t,
        Tree {
            d: vec![0.0, 1.0, 2.0, 1.5],
            pi: vec![0, 0, 1, 0]
        }
    );
    assert!(check(&g, 0, &t));
    let bad = |d: Vec<f64>, pi: Vec<u32>| !check(&g, 0, &Tree { d, pi });
    assert!(
        bad(vec![0.0, 1.0, 3.0, 1.5], vec![0, 0, 3, 0]),
        "detour parent"
    );
    assert!(
        bad(vec![0.0, 1.0, 1.9, 1.5], vec![0, 0, 1, 0]),
        "distance below the tree path"
    );
    assert!(
        bad(vec![0.0, 1.0, 2.0, f64::INFINITY], vec![0, 0, 1, 3]),
        "reachable node marked unreached"
    );
    assert!(
        bad(vec![0.0, 1.0, 2.0, 1.5], vec![0, 2, 1, 0]),
        "parent cycle"
    );
    assert!(
        bad(vec![0.0, 1.0, 2.0, 1.5], vec![0, 0, 9, 0]),
        "parent outside the graph"
    );
    assert!(bad(vec![0.0, 1.0, 2.0], vec![0, 0, 1]), "short tree");
    // A zero-weight cycle: tight arcs that never reach the source.
    let z = Graph::new(3, [(0, 1, 1.0), (1, 2, 0.0), (2, 1, 0.0)]);
    assert!(!check(
        &z,
        0,
        &Tree {
            d: vec![0.0, 1.0, 1.0],
            pi: vec![0, 2, 1]
        }
    ));
    assert!(check(&z, 0, &dijkstra(&z, 0)));
}
