//! CH adapter distances against Bellman–Ford on the input arcs.

use route::ch::Ch;
use route::graph::Graph;

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
    let ch = Ch::new(&g);
    let mut calc = ch.calculator();
    for s in 0..n {
        let expected = bellman(n, &arcs, s);
        for (t, &want) in expected.iter().enumerate() {
            assert_eq!(ch.query(&mut calc, s, t), want, "{s}->{t}, n={n}");
        }
    }
}

#[test]
fn all_pairs_on_positive_integer_graphs() {
    check(1, vec![]);
    check(2, vec![]);
    check(
        9,
        vec![
            (0, 1, 8.0),
            (0, 1, 2.0),
            (1, 2, 3.0),
            (2, 0, 1.0),
            (2, 3, 7.0),
            (3, 1, 2.0),
            (4, 5, 1.0),
            (5, 4, 4.0),
        ],
    );
    for n in 2..=14 {
        let arcs = (0..n)
            .flat_map(|u| {
                (0..n).filter_map(move |v| {
                    ((u * 13 + v * 7 + n) % 9 < 2).then_some((
                        u as u32,
                        v as u32,
                        ((u * 3 + v * 5) % 11 + 1) as f64,
                    ))
                })
            })
            .collect();
        check(n, arcs);
    }
}

#[test]
fn self_distance_and_trailing_isolates() {
    let g = Graph::new(8, [(0, 2, 1.0), (2, 4, 3.0)]);
    let ch = Ch::new(&g);
    let mut calc = ch.calculator();
    assert_eq!(ch.query(&mut calc, 4, 4), 0.0);
    assert_eq!(ch.query(&mut calc, 0, 4), 4.0);
    assert!(ch.query(&mut calc, 0, 7).is_infinite());
    assert!(ch.query(&mut calc, 7, 0).is_infinite());

    let no_edges = Graph::new(3, []);
    let ch = Ch::new(&no_edges);
    let mut calc = ch.calculator();
    assert_eq!(ch.query(&mut calc, 1, 1), 0.0);
    assert!(ch.query(&mut calc, 0, 2).is_infinite());
}

fn rejected(g: &Graph) {
    assert!(std::panic::catch_unwind(|| Ch::new(g)).is_err());
}

#[test]
fn rejects_zero_fractional_and_unsafe_weights() {
    rejected(&Graph::new(2, [(0, 1, 0.0)]));
    rejected(&Graph::new(2, [(0, 1, 1.5)]));
    rejected(&Graph::new(3, [(0, 1, (1u64 << 51) as f64)]));
    rejected(&Graph::new(10_000, [(0, 1, (1u64 << 51) as f64)]));
}
