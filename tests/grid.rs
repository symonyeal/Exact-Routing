//! Tests against oracles that share no code with the solver: enumeration of every simple path on
//! small grids, and a queue breadth-first search on wide ones.
//!
//! Legend: q grid bytes, h × w; s, t cells of S and G; moves ranked D < L < R < U.

use route::grid::{Maze, check, potential, render, solve};
use std::collections::VecDeque;

/// (rank, cell) for each move from v, in rank order D, L, R, U, walls excluded.
fn moves(q: &[u8], h: usize, w: usize, v: usize) -> Vec<(usize, usize)> {
    let (r, c) = (v / w, v % w);
    [
        (r + 1 < h).then(|| v + w),
        (c > 0).then(|| v - 1),
        (c + 1 < w).then(|| v + 1),
        (r > 0).then(|| v - w),
    ]
    .into_iter()
    .enumerate()
    .filter_map(|(rank, u)| u.filter(|&u| q[u] != b'#').map(|u| (rank, u)))
    .collect()
}

/// Minimum over all simple s–t paths of (length, move ranks), by exhaustive depth-first search.
fn brute(q: &[u8], h: usize, w: usize) -> Option<Vec<usize>> {
    fn go(
        q: &[u8],
        h: usize,
        w: usize,
        x: &mut Vec<usize>,
        key: &mut Vec<usize>,
        best: &mut Option<(Vec<usize>, Vec<usize>)>,
    ) {
        let v = *x.last().unwrap();
        if q[v] == b'G' {
            let better = best
                .as_ref()
                .is_none_or(|(b, k)| (x.len(), &*key) < (b.len(), k));
            if better {
                *best = Some((x.clone(), key.clone()));
            }
            return;
        }
        for (rank, u) in moves(q, h, w, v) {
            if !x.contains(&u) {
                x.push(u);
                key.push(rank);
                go(q, h, w, x, key, best);
                x.pop();
                key.pop();
            }
        }
    }
    let mut best = None;
    let s = q.iter().position(|&y| y == b'S').unwrap();
    go(q, h, w, &mut vec![s], &mut vec![], &mut best);
    best.map(|(x, _)| x)
}

/// Distances to G by queue breadth-first search; usize::MAX when unreachable.
fn bfs(q: &[u8], h: usize, w: usize) -> Vec<usize> {
    let t = q.iter().position(|&y| y == b'G').unwrap();
    let mut d = vec![usize::MAX; q.len()];
    d[t] = 0;
    let mut queue = VecDeque::from([t]);
    while let Some(v) = queue.pop_front() {
        for (_, u) in moves(q, h, w, v) {
            if d[u] == usize::MAX {
                d[u] = d[v] + 1;
                queue.push_back(u);
            }
        }
    }
    d
}

/// splitmix64 (Steele, Lea and Flood 2014).
fn mix(z: &mut u64) -> u64 {
    *z = z.wrapping_add(0x9e3779b97f4a7c15);
    let mut x = *z;
    x = (x ^ x >> 30).wrapping_mul(0xbf58476d1ce4e5b9);
    x = (x ^ x >> 27).wrapping_mul(0x94d049bb133111eb);
    x ^ x >> 31
}

/// The grid with walls at the set bits of `walls`, S at cell i and G at cell j.
fn grid(n: usize, walls: u64, i: usize, j: usize) -> Vec<u8> {
    let mut q: Vec<u8> = (0..n)
        .map(|v| if walls >> v & 1 == 1 { b'#' } else { b' ' })
        .collect();
    (q[i], q[j]) = (b'S', b'G');
    q
}

/// The solver's outcome and certificate, asserting that the certificate verifies.
fn certified(q: &[u8], h: usize, w: usize) -> Option<Vec<usize>> {
    let m = Maze::parse(q, h, w).unwrap();
    let x = solve(&m);
    assert!(
        check(&m, x.as_deref(), &potential(&m)),
        "certificate rejected:\n{}",
        show(q, w)
    );
    x
}

fn show(q: &[u8], w: usize) -> String {
    q.chunks(w)
        .map(|r| String::from_utf8_lossy(r).replace(' ', ".") + "\n")
        .collect()
}

#[test]
fn every_small_grid_matches_enumeration() {
    let mut z = 1;
    for (h, w) in [
        (1, 2),
        (1, 6),
        (6, 1),
        (2, 4),
        (3, 3),
        (3, 4),
        (4, 3),
        (4, 4),
    ] {
        let n = h * w;
        for walls in 0..1u64 << n {
            let open: Vec<usize> = (0..n).filter(|&v| walls >> v & 1 == 0).collect();
            let pairs: Vec<(usize, usize)> = if n <= 9 {
                open.iter()
                    .flat_map(|&i| open.iter().map(move |&j| (i, j)))
                    .filter(|(i, j)| i != j)
                    .collect()
            } else if open.len() >= 2 {
                let i = mix(&mut z) as usize % open.len();
                let j = (i + 1 + mix(&mut z) as usize % (open.len() - 1)) % open.len();
                vec![(open[i], open[j])]
            } else {
                vec![]
            };
            for (i, j) in pairs {
                let q = grid(n, walls, i, j);
                assert_eq!(certified(&q, h, w), brute(&q, h, w), "\n{}", show(&q, w));
            }
        }
    }
}

#[test]
fn wide_grids_match_breadth_first_search() {
    let mut z = 7;
    for (h, w) in [
        (9, 31),
        (9, 32),
        (40, 20),
        (5, 63),
        (4, 64),
        (6, 65),
        (3, 127),
        (7, 128),
        (5, 130),
        (40, 200),
    ] {
        for _ in 0..40 {
            let n = h * w;
            let mut q: Vec<u8> = (0..n)
                .map(|_| if mix(&mut z) % 100 < 30 { b'#' } else { b' ' })
                .collect();
            let i = mix(&mut z) as usize % n;
            let j = (i + 1 + mix(&mut z) as usize % (n - 1)) % n;
            (q[i], q[j]) = (b'S', b'G');
            let d = bfs(&q, h, w);
            let x = certified(&q, h, w);
            assert_eq!(
                x.as_ref().map(|x| x.len() - 1),
                (d[i] != usize::MAX).then_some(d[i])
            );
            if let Some(x) = x {
                // Proposition 1: each move is the first in D, L, R, U order that lowers d.
                for e in x.windows(2) {
                    assert_eq!(
                        Some(e[1]),
                        moves(&q, h, w, e[0])
                            .into_iter()
                            .map(|(_, u)| u)
                            .find(|&u| d[u] + 1 == d[e[0]])
                    );
                }
            }
        }
    }
}

#[test]
fn certificates_reject_wrong_outcomes() {
    let q = b"S  #\n #  \n   G"
        .iter()
        .filter(|&&y| y != b'\n')
        .copied()
        .collect::<Vec<u8>>();
    let m = Maze::parse(&q, 3, 4).unwrap();
    let (x, p) = (solve(&m).unwrap(), potential(&m));
    assert!(check(&m, Some(&x), &p));
    assert_eq!(render(&q, &x), b"S  #o#  oooG".to_vec());
    assert!(!check(&m, Some(&[0, 4]), &p), "path ending before G");
    assert!(
        !check(&m, Some(&[0, 4, 8, 9, 10, 6, 7, 11]), &p),
        "longer path"
    );
    assert!(!check(&m, Some(&[0, 1, 2, 6, 11]), &p), "jump");
    assert!(!check(&m, Some(&[0, 1, 2, 3, 7, 11]), &p), "wall");
    assert!(
        !check(&m, Some(&[0, 1_000_000, 11]), &p),
        "cell outside the grid"
    );
    assert!(
        !check(&m, None, &p),
        "infeasibility claimed for a solvable maze"
    );
    let mut p2 = p.clone();
    p2[0] += 1;
    assert!(!check(&m, Some(&x), &p2), "p_s above the bound");
    let mut p3 = p.clone();
    p3[6] = 0;
    assert!(!check(&m, Some(&x), &p3), "Lipschitz violation");
    assert!(!check(&m, Some(&x), &p[1..]), "short potential");

    let q = b"S# #G".to_vec();
    let m = Maze::parse(&q, 1, 5).unwrap();
    let p = potential(&m);
    assert_eq!(solve(&m), None);
    assert!(check(&m, None, &p));
    assert!(!check(&m, None, &[0; 5]), "finite p_s");
}

#[test]
fn parse_rejects_malformed_grids() {
    for (q, h, w) in [
        (&b"S G"[..], 1, 2),
        (b"S.G", 1, 3),
        (b"SSG", 1, 3),
        (b"S  ", 1, 3),
        (b"", 0, 0),
    ] {
        assert!(
            Maze::parse(q, h, w).is_err(),
            "{:?}",
            String::from_utf8_lossy(q)
        );
    }
}
