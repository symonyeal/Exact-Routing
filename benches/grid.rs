//! End-to-end timing and peak heap of the solver and rival exact methods, question bytes to answer
//! bytes, single-threaded unless stated.
//!
//! Usage: cargo bench --bench grid [-- data/test.csv]
//!
//! Rivals: `queue`, breadth-first search from G with an array queue over a wall-padded copy of the
//! grid and the same tie rule; `pathfinding bfs` and `pathfinding astar` (Manhattan heuristic), the
//! pathfinding crate's searches from S with successors in D, L, R, U order. Methods are timed in
//! turn on each maze (paired); a maze's time is the minimum over passes of the mean over R calls,
//! and the tables summarize those times over mazes. `exact` counts answers equal to the reference,
//! `optimal` answers with the reference's path length.

use pathfinding::prelude::{astar, bfs, bfs_reach};
use route::grid::{Maze, check, potential, render, solve};
use std::alloc::{GlobalAlloc, Layout, System};
use std::hint::black_box;
use std::sync::atomic::{AtomicUsize, Ordering::Relaxed};
use std::time::Instant;
use std::{env, fs, thread};

/// The system allocator, recording live and peak heap bytes.
struct Counted;
static LIVE: AtomicUsize = AtomicUsize::new(0);
static PEAK: AtomicUsize = AtomicUsize::new(0);

unsafe impl GlobalAlloc for Counted {
    unsafe fn alloc(&self, l: Layout) -> *mut u8 {
        let live = LIVE.fetch_add(l.size(), Relaxed) + l.size();
        PEAK.fetch_max(live, Relaxed);
        unsafe { System.alloc(l) }
    }
    unsafe fn dealloc(&self, p: *mut u8, l: Layout) {
        LIVE.fetch_sub(l.size(), Relaxed);
        unsafe { System.dealloc(p, l) }
    }
}

#[global_allocator]
static A: Counted = Counted;

/// A method maps (question, H, W) to the answer grid, or None when G is unreachable.
type Method = fn(&[u8], usize, usize) -> Option<Vec<u8>>;

fn wave(q: &[u8], h: usize, w: usize) -> Option<Vec<u8>> {
    let m = Maze::parse(q, h, w).unwrap();
    solve(&m).map(|x| render(q, &x))
}

fn certify(q: &[u8], h: usize, w: usize) -> Option<Vec<u8>> {
    let m = Maze::parse(q, h, w).unwrap();
    let x = solve(&m);
    assert!(check(&m, x.as_deref(), &potential(&m)));
    x.map(|x| render(q, &x))
}

fn queue(q: &[u8], h: usize, w: usize) -> Option<Vec<u8>> {
    let pw = w + 2;
    let mut g = vec![b'#'; (h + 2) * pw];
    for (r, row) in q.chunks_exact(w).enumerate() {
        g[(r + 1) * pw + 1..(r + 1) * pw + 1 + w].copy_from_slice(row);
    }
    let (s, t) = ends(&g);
    let mut d = vec![u32::MAX; g.len()];
    let mut fifo = Vec::with_capacity(h * w);
    d[t] = 0;
    fifo.push(t as u32);
    let mut head = 0;
    'search: while let Some(&v) = fifo.get(head) {
        let v = v as usize;
        head += 1;
        for u in [v + pw, v - 1, v + 1, v - pw] {
            if g[u] != b'#' && d[u] == u32::MAX {
                d[u] = d[v] + 1;
                if u == s {
                    break 'search;
                }
                fifo.push(u as u32);
            }
        }
    }
    if d[s] == u32::MAX {
        return None;
    }
    let cell = |v: usize| (v / pw - 1) * w + v % pw - 1;
    let mut path = vec![cell(s)];
    let mut v = s;
    while v != t {
        v = [v + pw, v - 1, v + 1, v - pw]
            .into_iter()
            .find(|&u| d[u].wrapping_add(1) == d[v])
            .unwrap();
        path.push(cell(v));
    }
    Some(render(q, &path))
}

/// Open cells adjacent to v, in D, L, R, U order.
fn successors(q: &[u8], h: usize, w: usize, v: usize) -> impl Iterator<Item = usize> + '_ {
    let (r, c) = (v / w, v % w);
    [
        (r + 1 < h).then(|| v + w),
        (c > 0).then(|| v - 1),
        (c + 1 < w).then(|| v + 1),
        (r > 0).then(|| v - w),
    ]
    .into_iter()
    .flatten()
    .filter(move |&u| q[u] != b'#')
}

fn ends(q: &[u8]) -> (usize, usize) {
    (
        q.iter().position(|&x| x == b'S').unwrap(),
        q.iter().position(|&x| x == b'G').unwrap(),
    )
}

fn pf_bfs(q: &[u8], h: usize, w: usize) -> Option<Vec<u8>> {
    let (s, t) = ends(q);
    bfs(&s, |&v| successors(q, h, w, v), |&v| v == t).map(|x| render(q, &x))
}

fn pf_astar(q: &[u8], h: usize, w: usize) -> Option<Vec<u8>> {
    let (s, t) = ends(q);
    let manhattan = |v: usize| (v / w).abs_diff(t / w) + (v % w).abs_diff(t % w);
    astar(
        &s,
        |&v| successors(q, h, w, v).map(|u| (u, 1)),
        |&v| manhattan(v),
        |&v| v == t,
    )
    .map(|(x, _)| render(q, &x))
}

const METHODS: [(&str, Method); 5] = [
    ("wave", wave),
    ("wave + certificate", certify),
    ("queue", queue),
    ("pathfinding bfs", pf_bfs),
    ("pathfinding astar", pf_astar),
];

/// Seconds per call, [method][maze]: `fs[j](i)` runs method j once on maze i; the methods run in
/// turn on each maze, and each entry is the minimum over `passes` of the mean over `reps` calls.
fn time(fs: &[&dyn Fn(usize)], count: usize, passes: usize, reps: usize) -> Vec<Vec<f64>> {
    let mut best = vec![vec![f64::INFINITY; count]; fs.len()];
    for _ in 0..passes {
        for i in 0..count {
            for (b, f) in best.iter_mut().zip(fs) {
                let t0 = Instant::now();
                (0..reps).for_each(|_| f(black_box(i)));
                b[i] = b[i].min(t0.elapsed().as_secs_f64() / reps as f64);
            }
        }
    }
    best
}

/// Median over mazes of the paired ratio x/y, with a 95% percentile-bootstrap interval over mazes.
fn ratio(x: &[f64], y: &[f64]) -> (f64, f64, f64) {
    let r: Vec<f64> = x.iter().zip(y).map(|(a, b)| a / b).collect();
    let mut z = 7;
    let boot: Vec<f64> = (0..2000)
        .map(|_| {
            let sample: Vec<f64> = (0..r.len())
                .map(|_| r[mix(&mut z) as usize % r.len()])
                .collect();
            quantile(&sample, 0.5)
        })
        .collect();
    (
        quantile(&r, 0.5),
        quantile(&boot, 0.025),
        quantile(&boot, 0.975),
    )
}

/// Peak heap bytes of one call, maximized over mazes.
fn heap(f: Method, mazes: &[Vec<u8>], h: usize, w: usize) -> usize {
    mazes
        .iter()
        .map(|q| {
            let base = LIVE.load(Relaxed);
            PEAK.store(base, Relaxed);
            black_box(f(q, h, w));
            PEAK.load(Relaxed) - base
        })
        .max()
        .unwrap()
}

fn quantile(x: &[f64], a: f64) -> f64 {
    let mut x = x.to_vec();
    x.sort_by(f64::total_cmp);
    x[((x.len() - 1) as f64 * a).round() as usize]
}

/// splitmix64 (Steele, Lea and Flood 2014).
fn mix(z: &mut u64) -> u64 {
    *z = z.wrapping_add(0x9e3779b97f4a7c15);
    let mut x = *z;
    x = (x ^ x >> 30).wrapping_mul(0xbf58476d1ce4e5b9);
    x = (x ^ x >> 27).wrapping_mul(0x94d049bb133111eb);
    x ^ x >> 31
}

/// An n×n grid with each cell a wall with probability 0.37 (the Maze-Hard mean), S uniform among
/// open cells and G uniform among the later half of S's component in breadth-first order.
fn generate(n: usize, z: &mut u64) -> Vec<u8> {
    loop {
        let mut q: Vec<u8> = (0..n * n)
            .map(|_| if mix(z) % 100 < 37 { b'#' } else { b' ' })
            .collect();
        let s = mix(z) as usize % (n * n);
        if q[s] == b'#' {
            continue;
        }
        let reach: Vec<usize> =
            bfs_reach(s, |&v| successors(&q, n, n, v).collect::<Vec<_>>()).collect();
        if reach.len() < 2 * n {
            continue;
        }
        let t = reach[reach.len() / 2 + mix(z) as usize % (reach.len() - reach.len() / 2)];
        (q[s], q[t]) = (b'S', b'G');
        return q;
    }
}

fn main() {
    let file = env::args()
        .skip(1)
        .find(|a| !a.starts_with('-'))
        .unwrap_or("data/test.csv".into());
    let text =
        fs::read_to_string(&file).unwrap_or_else(|e| panic!("{file}: {e}; run scripts/fetch.py"));
    let rows: Vec<(&str, &str)> = text
        .lines()
        .skip(1)
        .map(|l| {
            let f: Vec<&str> = l.split(',').collect();
            (f[1], f[2])
        })
        .collect();
    let mazes: Vec<Vec<u8>> = rows.iter().map(|r| r.0.as_bytes().to_vec()).collect();
    let n = mazes[0].len().isqrt();
    let moves = |a: &[u8]| a.iter().filter(|&&x| x == b'o').count();

    println!("## {file}: {} mazes, {n}×{n}\n", mazes.len());
    println!(
        "| method | exact | optimal | median µs | p95 µs | mazes/s | time / wave (95% CI) | peak heap B |"
    );
    println!("| --- | ---: | ---: | ---: | ---: | ---: | ---: | ---: |");
    let mz = &mazes;
    let calls: Vec<Box<dyn Fn(usize)>> = METHODS
        .iter()
        .map(|&(_, f)| {
            Box::new(move |i: usize| _ = black_box(f(&mz[i], n, n))) as Box<dyn Fn(usize)>
        })
        .collect();
    let times = time(
        &calls.iter().map(|c| &**c).collect::<Vec<_>>(),
        mazes.len(),
        5,
        20,
    );
    for ((name, f), t) in METHODS.iter().zip(&times) {
        let answers: Vec<Option<Vec<u8>>> = mazes.iter().map(|q| f(q, n, n)).collect();
        let exact = rows
            .iter()
            .zip(&answers)
            .filter(|(r, a)| a.as_deref() == Some(r.1.as_bytes()))
            .count();
        let optimal = rows
            .iter()
            .zip(&answers)
            .filter(|(r, a)| a.as_deref().map(moves) == Some(moves(r.1.as_bytes())))
            .count();
        let mean = t.iter().sum::<f64>() / t.len() as f64;
        let (r, lo, hi) = ratio(t, &times[0]);
        println!(
            "| {name} | {exact} | {optimal} | {:.2} | {:.2} | {:.0} | {r:.2} ({lo:.2}–{hi:.2}) | {} |",
            1e6 * quantile(t, 0.5),
            1e6 * quantile(t, 0.95),
            1.0 / mean,
            heap(*f, &mazes, n, n)
        );
    }

    let parsed: Vec<Maze> = mazes
        .iter()
        .map(|q| Maze::parse(q, n, n).unwrap())
        .collect();
    let paths: Vec<Vec<usize>> = parsed.iter().map(|m| solve(m).unwrap()).collect();
    let ps: Vec<Vec<u32>> = parsed.iter().map(potential).collect();
    let stages: [(&str, &dyn Fn(usize)); 5] = [
        ("parse", &|i| _ = black_box(Maze::parse(&mazes[i], n, n))),
        ("solve", &|i| _ = black_box(solve(&parsed[i]))),
        ("render", &|i| _ = black_box(render(&mazes[i], &paths[i]))),
        ("potential", &|i| _ = black_box(potential(&parsed[i]))),
        ("check", &|i| {
            _ = black_box(check(&parsed[i], Some(&paths[i]), &ps[i]))
        }),
    ];
    let t = time(&stages.map(|p| p.1), mazes.len(), 5, 20);
    let cells: Vec<String> = t
        .iter()
        .map(|t| format!("{:.2}", 1e6 * quantile(t, 0.5)))
        .collect();
    println!("\n| stage | {} |", stages.map(|p| p.0).join(" | "));
    println!("| --- |{}", " ---: |".repeat(stages.len()));
    println!("| median µs | {} |", cells.join(" | "));

    let threads = thread::available_parallelism().map_or(1, |x| x.get());
    let (reps, t0) = (20, Instant::now());
    thread::scope(|scope| {
        for part in mazes.chunks(mazes.len().div_ceil(threads)) {
            scope.spawn(move || {
                (0..reps).for_each(|_| part.iter().for_each(|q| _ = black_box(wave(q, n, n))))
            });
        }
    });
    let rate = (reps * mazes.len()) as f64 / t0.elapsed().as_secs_f64();
    println!("\nwave on {threads} threads: {rate:.0} mazes/s\n");

    println!("## Generated n×n grids, wall probability 0.37\n");
    println!("| n | mazes | mean D | wave µs | queue µs | pathfinding bfs µs |");
    println!("| ---: | ---: | ---: | ---: | ---: | ---: |");
    let mut z = 2026;
    for n in [30, 64, 128, 256, 512, 1024] {
        let count = (6000 / n).min(200);
        let mazes: Vec<Vec<u8>> = (0..count).map(|_| generate(n, &mut z)).collect();
        let d: usize = mazes
            .iter()
            .map(|q| moves(&wave(q, n, n).unwrap()) + 1)
            .sum();
        let mz = &mazes;
        let call = |f: Method| move |i: usize| _ = black_box(f(&mz[i], n, n));
        let (a, b, c) = (call(wave), call(queue), call(pf_bfs));
        let t = time(&[&a, &b, &c], count, 3, (20 * 900 / (n * n)).max(1));
        let median = |i: usize| 1e6 * quantile(&t[i], 0.5);
        println!(
            "| {n} | {count} | {:.0} | {:.1} | {:.1} | {:.1} |",
            d as f64 / count as f64,
            median(0),
            median(1),
            median(2)
        );
    }
}
