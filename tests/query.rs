//! Bidirectional distances against Bellman–Ford on raw directed arcs.

use route::graph::Graph;
use route::query::BiSearch;

fn bellman(n: usize, arcs: &[(u32, u32, f64)], s: usize) -> Vec<f64> {
    let mut d = vec![f64::INFINITY; n];
    d[s] = 0.0;
    for _ in 1..n {
        let mut changed = false;
        for &(u, v, w) in arcs {
            if d[u as usize] + w < d[v as usize] {
                d[v as usize] = d[u as usize] + w;
                changed = true;
            }
        }
        if !changed {
            break;
        }
    }
    d
}

fn check(n: usize, arcs: Vec<(u32, u32, f64)>) {
    let g = Graph::new(n, arcs.iter().copied());
    let r = g.reverse();
    let mut search = BiSearch::new(n);
    for s in 0..n {
        let expected = bellman(n, &arcs, s);
        for (t, &want) in expected.iter().enumerate() {
            assert_eq!(search.query(&g, &r, s, t), want, "{s}->{t}, n={n}");
        }
    }
}

#[test]
fn all_pairs_on_deterministic_directed_graphs() {
    check(1, vec![]);
    check(4, vec![(0, 1, 0.0), (1, 2, 0.0), (2, 1, 0.0), (2, 3, 4.0)]);
    check(
        6,
        vec![
            (0, 1, 3.0),
            (0, 1, 1.0),
            (1, 2, 2.0),
            (2, 0, 1.0),
            (3, 4, 0.0),
            (4, 3, 0.0),
            (4, 5, 5.0),
            (5, 5, 0.0),
        ],
    );
    for n in 2..=12 {
        let arcs = (0..n)
            .flat_map(|u| {
                (0..n).filter_map(move |v| {
                    ((u * 17 + v * 11 + n) % 7 < 2).then_some((
                        u as u32,
                        v as u32,
                        ((u * 5 + v * 3) % 4) as f64,
                    ))
                })
            })
            .collect();
        check(n, arcs);
    }
}

#[test]
fn continues_after_an_expensive_first_meeting() {
    let arcs = vec![
        (0, 1, 1.0),
        (1, 5, 100.0),
        (0, 2, 2.0),
        (2, 3, 2.0),
        (3, 4, 2.0),
        (4, 5, 2.0),
    ];
    let g = Graph::new(6, arcs);
    let r = g.reverse();
    let mut search = BiSearch::new(6);
    assert_eq!(search.query(&g, &r, 0, 5), 8.0);
}

#[test]
fn noninteger_weights_and_reuse_after_unreachable_query() {
    let g = Graph::new(4, [(0, 1, 0.1), (1, 2, 0.2), (0, 2, 0.5)]);
    let r = g.reverse();
    let mut search = BiSearch::new(4);
    assert!(search.query(&g, &r, 3, 0).is_infinite());
    let d = search.query(&g, &r, 0, 2);
    assert!((d - 0.3).abs() < 1e-12);
    assert_eq!(search.query(&g, &r, 2, 2), 0.0);
}
