//! Single-source shortest paths on sparse directed graphs, each with a certificate.
//!
//! Problem. A graph of n nodes and m weighted arcs, weights finite and nonnegative, and a source s.
//! Return a shortest-path tree: the distance d(v) from s and a parent π(v), the tail of the last
//! arc of a shortest s–v path, for every node v. Unreached nodes have d = ∞ and π(v) = v; π(s) = s.
//!
//! Legend
//!   n, m   nodes, arcs (self-loops are dropped)
//!   off    row offsets: arcs off[u] .. off[u + 1] leave u
//!   head   head of each arc, ascending within a row
//!   w      weight of each arc
//!   s      source
//!   d, π   distances and parents

use std::cmp::Reverse;
use std::collections::BinaryHeap;

/// A directed graph in compressed sparse row form.
pub struct Graph {
    pub n: usize,
    pub(crate) off: Vec<usize>,
    pub(crate) head: Vec<u32>,
    pub(crate) w: Vec<f64>,
}

/// A shortest-path tree: d and π.
#[derive(Debug, PartialEq)]
pub struct Tree {
    pub d: Vec<f64>,
    pub pi: Vec<u32>,
}

impl Graph {
    /// The graph on n nodes with arcs (u, v, w), u, v < n; self-loops are dropped and each row is
    /// sorted by head.
    pub fn new(n: usize, arcs: impl IntoIterator<Item = (u32, u32, f64)>) -> Graph {
        let mut arcs: Vec<(u32, u32, f64)> = arcs.into_iter().filter(|a| a.0 != a.1).collect();
        arcs.sort_by_key(|a| (a.0, a.1));
        let mut off = vec![0; n + 1];
        arcs.iter().for_each(|a| off[a.0 as usize + 1] += 1);
        (0..n).for_each(|u| off[u + 1] += off[u]);
        Graph {
            n,
            off,
            head: arcs.iter().map(|a| a.1).collect(),
            w: arcs.iter().map(|a| a.2).collect(),
        }
    }

    /// The graph with every arc reversed.
    pub fn reverse(&self) -> Graph {
        Graph::new(
            self.n,
            (0..self.n).flat_map(|u| self.arcs(u).map(move |(v, w)| (v, u as u32, w))),
        )
    }

    /// Arcs (head, weight) leaving u, by ascending head.
    pub fn arcs(&self, u: usize) -> impl Iterator<Item = (u32, f64)> + '_ {
        let r = self.off[u]..self.off[u + 1];
        self.head[r.clone()]
            .iter()
            .copied()
            .zip(self.w[r].iter().copied())
    }

    /// Arc weights in CSR order.
    pub fn weights(&self) -> &[f64] {
        &self.w
    }

    /// The same arcs with weights w, in CSR order.
    pub fn with_weights(&self, w: Vec<f64>) -> Graph {
        assert_eq!(w.len(), self.w.len());
        Graph {
            n: self.n,
            off: self.off.clone(),
            head: self.head.clone(),
            w,
        }
    }

    /// CSR indices of the arcs of the tree path from the root of `t` to v, last arc first. Each step
    /// takes the first parallel arc (π(x), x) that is tight in `t`. Empty when v is the root or
    /// unreached; `t` must be a certified tree of this graph.
    pub fn path(&self, t: &Tree, v: usize) -> Vec<usize> {
        let mut arcs = Vec::new();
        let mut x = v;
        while t.d[x].is_finite() && t.pi[x] as usize != x {
            let u = t.pi[x] as usize;
            let r = self.off[u]..self.off[u + 1];
            let a = r.start + self.head[r.clone()].partition_point(|&h| (h as usize) < x);
            let a = (a..r.end)
                .find(|&a| t.d[u] + self.w[a] == t.d[x])
                .expect("tree arc is not tight");
            arcs.push(a);
            x = u;
        }
        arcs
    }

    fn degree(&self, u: usize) -> usize {
        self.off[u + 1] - self.off[u]
    }
}

/// Breadth-first search from s, ignoring weights. π(v) is the smallest-index node one layer
/// closer to s, the rule of the CLRS benchmark's breadth-first search.
///
/// Direction-optimizing (Beamer, Asanović and Patterson 2012): a layer is expanded top-down, from
/// each frontier node along its arcs, while the frontier is small, and bottom-up, each unreached
/// node scanning its in-arcs (`r`, the reverse graph) for a frontier node, once the frontier's arcs
/// exceed 1/α of the unexplored arcs; it returns to top-down when the frontier falls below n/β.
/// Both directions visit nodes in ascending order, so both apply the same tie rule. O(n + m).
pub fn bfs(g: &Graph, r: &Graph, s: usize) -> Tree {
    const ALPHA: usize = 14;
    const BETA: usize = 24;
    let n = g.n;
    let mut t = Tree {
        d: vec![f64::INFINITY; n],
        pi: (0..n as u32).collect(),
    };
    let mut seen = vec![false; n];
    let mut front = vec![s as u32];
    let mut inside = vec![false; n]; // frontier membership, kept for bottom-up layers
    let mut unexplored = g.head.len() - g.degree(s);
    let mut down = false;
    (t.d[s], seen[s]) = (0.0, true);
    for k in 1.. {
        if front.is_empty() {
            break;
        }
        let arcs: usize = front.iter().map(|&u| g.degree(u as usize)).sum();
        down = front.len() >= n / BETA && (down || arcs > unexplored / ALPHA);
        let mut next = Vec::new();
        if down {
            front.iter().for_each(|&u| inside[u as usize] = true);
            for v in (0..n).filter(|&v| !seen[v]) {
                if let Some((u, _)) = r.arcs(v).find(|&(u, _)| inside[u as usize]) {
                    t.pi[v] = u;
                    next.push(v as u32);
                }
            }
            front.iter().for_each(|&u| inside[u as usize] = false);
        } else {
            for &u in &front {
                for (v, _) in g.arcs(u as usize) {
                    if !seen[v as usize] {
                        seen[v as usize] = true;
                        t.pi[v as usize] = u;
                        next.push(v);
                    }
                }
            }
            next.sort_unstable();
        }
        for &v in &next {
            (t.d[v as usize], seen[v as usize]) = (k as f64, true);
            unexplored -= g.degree(v as usize);
        }
        front = next;
    }
    t
}

/// Dijkstra's algorithm from s with a binary heap, settling ties in d by node index. π(v) is the
/// first settled node attaining d(v): a relaxation must strictly improve d. O(m log n).
pub fn dijkstra(g: &Graph, s: usize) -> Tree {
    let mut t = Tree {
        d: vec![f64::INFINITY; g.n],
        pi: (0..g.n as u32).collect(),
    };
    let mut done = vec![false; g.n];
    let mut heap = BinaryHeap::from([Reverse((Key(0.0), s as u32))]);
    t.d[s] = 0.0;
    while let Some(Reverse((Key(du), u))) = heap.pop() {
        let u = u as usize;
        if std::mem::replace(&mut done[u], true) {
            continue;
        }
        for (v, w) in g.arcs(u) {
            let v = v as usize;
            if !done[v] && du + w < t.d[v] {
                (t.d[v], t.pi[v]) = (du + w, u as u32);
                heap.push(Reverse((Key(du + w), v as u32)));
            }
        }
    }
    t
}

/// A distance ordered by `f64::total_cmp`.
pub(crate) struct Key(pub(crate) f64);

impl PartialEq for Key {
    fn eq(&self, o: &Self) -> bool {
        self.cmp(o).is_eq()
    }
}

impl Eq for Key {}

impl PartialOrd for Key {
    fn partial_cmp(&self, o: &Self) -> Option<std::cmp::Ordering> {
        Some(self.cmp(o))
    }
}

impl Ord for Key {
    fn cmp(&self, o: &Self) -> std::cmp::Ordering {
        self.0.total_cmp(&o.0)
    }
}

/// Verify a shortest-path tree (the certificate of Mehlhorn and Näher, LEDA, 1999): d(s) = 0 and
/// π(s) = s; d(v) ≤ d(u) + w for every arc (u, v); each reached v ≠ s has a tight arc (π(v), v),
/// d(v) = d(π(v)) + w, and following π from v reaches s; each unreached v has π(v) = v.
/// Feasibility bounds every s–v path below by d(v), and the tight arcs back to s form a path of
/// length d(v). Sums are compared exactly as the floating-point additions `dijkstra` performs;
/// `unit` checks with weight 1 on every arc, for breadth-first search. O(n + m).
pub fn check(g: &Graph, s: usize, t: &Tree, unit: bool) -> bool {
    let n = g.n;
    let cost = |w: f64| if unit { 1.0 } else { w };
    if t.d.len() != n || t.pi.len() != n || s >= n || t.d[s] != 0.0 || t.pi[s] as usize != s {
        return false;
    }
    let feasible = (0..n).all(|u| g.arcs(u).all(|(v, w)| t.d[v as usize] <= t.d[u] + cost(w)));
    let tight = (0..n).filter(|&v| v != s).all(|v| {
        let u = t.pi[v] as usize;
        if t.d[v] == f64::INFINITY {
            return u == v;
        }
        u < n
            && g.arcs(u)
                .any(|(h, w)| h as usize == v && t.d[u] + cost(w) == t.d[v])
    });
    // π traces every reached node back to s. State: 0 unknown, 1 on the current chain, 2 reaches s,
    // 3 does not. Reached parents are reached, since their arc is tight.
    let mut state = vec![0u8; n];
    state[s] = 2;
    let rooted = tight
        && (0..n).filter(|&v| t.d[v] < f64::INFINITY).all(|v| {
            let (mut x, mut chain) = (v, vec![]);
            while state[x] == 0 {
                state[x] = 1;
                chain.push(x);
                x = t.pi[x] as usize;
            }
            let ok = state[x] == 2;
            chain
                .iter()
                .for_each(|&y| state[y] = if ok { 2 } else { 3 });
            ok
        });
    feasible && rooted
}
