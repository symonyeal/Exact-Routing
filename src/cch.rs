//! Customizable contraction hierarchies (Dibbelt, Strasser and Wagner, "Customizable Contraction
//! Hierarchies", ACM JEA 21, 2016) for exact point-to-point shortest paths under changing weights.
//!
//! Problem. Answer s–t queries on a directed graph whose weights change while its arcs do not.
//! Preparation reads the topology only: order the nodes by nested dissection and eliminate them in
//! that order, giving the chordal supergraph G⁺ of the undirected graph. Customization reads one
//! metric: give every G⁺ edge {x, y}, x ranked below y, the weights up(x→y) and down(y→x) of the
//! input arcs (∞ where none exists), then lower each by its lower triangles {v, x, y}, v below x:
//!   up(x→y) = min(arc(x→y), min_v down(x→v) + up(v→y)),  down(y→x) likewise,     (basic)
//! taking the edges in rank order of x. Afterwards up(x→y) is the shortest x–y distance over paths
//! whose interior nodes rank below x, so every shortest s–t path has an up-down representation over
//! G⁺ with equal cost.
//!
//! Customization is pulled: node x recomputes its own upward edges from rows of lower nodes.
//!   serial    every x in rank order;
//!   parallel  the pieces of one nested-dissection level on separate threads, then the separators
//!             above them in rank order. A piece owns a contiguous rank range and shares no G⁺
//!             edge with another piece of its level, so threads write disjoint rows;
//!   partial   only the edges whose arc changed, then each edge with a lower triangle whose weight
//!             changed, by rank of the lower end (Buchhold, Sanders and Wagner, "Real-time traffic
//!             assignment using engineered customizable contraction hierarchies", ACM JEA 24, 2019).
//!             Each is recomputed from (basic) rather than relaxed, since weights may rise.
//! All three take the minimum over the same terms, so their weights are bit-identical.
//!
//! Perfect customization then visits nodes v in descending rank and lowers the edges of v by the
//! edge {x, y} between each pair of its upper neighbours, which is already exact; every edge then
//! carries its true distance. An edge direction equal to the sum of two strictly shorter pieces of
//! an intermediate or upper triangle is dropped; queries search the edges kept.
//!
//! Query (elimination-tree search, Bauer, Columbus, Rutter and Wagner 2016). The upper neighbours of
//! v are its ancestors in the elimination tree, whose parent of v is its lowest upper neighbour.
//! Relaxing the upward edges of every ancestor of s in tree order gives d↑(s, z); the same from t on
//! the down weights gives d↓(z, t); d(s, t) is the minimum of their sum over the common ancestors z.
//! A basic path is unpacked edge by edge: an edge whose weight is still its input arc's is that arc;
//! otherwise a lower triangle v attains it, and the edge expands to its two edges through v.
//!
//! Legend
//!   rank, node  position of each node in the elimination order, and its inverse; G⁺ is in rank space
//!   up          edges of x lead to its upper neighbours, ascending (off, head)
//!   low         lower end x of each edge; down lists the edges (v, x), v below x, v ascending
//!               (doff, down)
//!   w[e]        [up(x→y), down(y→x)] for edge e = (x, y), x below y; arc[e] the input-arc values
//!   parent      elimination-tree parent, NONE at a root
//!   slot        for each input arc in CSR order, 2e + direction of the G⁺ edge it initializes
//!   piece       [lo, hi, depth]: the rank range of one nested-dissection piece and its depth

use crate::graph::Graph;
use crate::overlay::{Model, separate};
use std::cmp::Reverse;
use std::collections::BinaryHeap;
use std::time::Instant;

const NONE: u32 = u32::MAX;
/// Pieces of at most this many nodes are ordered without further dissection.
const LEAF: usize = 2;
/// Pieces are recorded down to this dissection depth, enough for 2⁸ parallel tasks.
const DEPTH: u32 = 8;

/// The metric-independent hierarchy.
pub struct Cch {
    rank: Vec<u32>,
    node: Vec<u32>,
    off: Vec<usize>,
    head: Vec<u32>,
    low: Vec<u32>,
    doff: Vec<usize>,
    down: Vec<u32>,
    parent: Vec<u32>,
    slot: Vec<u32>,
    pieces: Vec<[u32; 3]>,
}

/// Customized weights and the input-arc weights they started from, [up, down] per G⁺ edge.
#[derive(Clone, PartialEq)]
pub struct Metric {
    w: Vec<[f64; 2]>,
    arc: Vec<[f64; 2]>,
}

/// The edge directions kept after perfect customization, one row per node and direction: their
/// G⁺ edge, head and exact weight. A kept direction is attained by no intermediate or upper
/// triangle, so its weight is its basic weight and it unpacks through the basic metric.
pub struct Perfect {
    off: [Vec<usize>; 2],
    id: [Vec<u32>; 2],
    head: [Vec<u32>; 2],
    w: [Vec<f64>; 2],
}

/// Reusable query state in rank space: distances and the edge that last improved each. Only
/// elimination-tree paths are touched and reset.
pub struct Workspace {
    df: Vec<f64>,
    db: Vec<f64>,
    pf: Vec<u32>,
    pb: Vec<u32>,
}

impl Cch {
    /// Order by nested dissection with node separators from inertial flow on coordinates `xy`, then
    /// eliminate. Reads the arcs of `g`, never their weights.
    pub fn new(g: &Graph, xy: &[(f64, f64)]) -> Cch {
        let n = g.n;
        assert!(xy.len() == n && n < NONE as usize);
        let mut nb: Vec<Vec<u32>> = vec![Vec::new(); n];
        for u in 0..n {
            for (v, _) in g.arcs(u) {
                nb[u].push(v);
                nb[v as usize].push(u as u32);
            }
        }
        for a in &mut nb {
            a.sort_unstable();
            a.dedup();
        }
        let (rank, pieces) = dissect(&nb, xy);
        // Elimination game: v's upper neighbours, less the lowest p, become p's neighbours.
        let mut up: Vec<Vec<u32>> = vec![Vec::new(); n];
        for (x, a) in nb.iter().enumerate() {
            for &y in a {
                let (rx, ry) = (rank[x], rank[y as usize]);
                if rx < ry {
                    up[rx as usize].push(ry);
                }
            }
        }
        drop(nb);
        let mut parent = vec![NONE; n];
        for v in 0..n {
            let mut a = std::mem::take(&mut up[v]);
            a.sort_unstable();
            a.dedup();
            if let Some((&p, rest)) = a.split_first() {
                parent[v] = p;
                up[p as usize].extend_from_slice(rest);
            }
            up[v] = a;
        }
        let mut off = vec![0; n + 1];
        (0..n).for_each(|v| off[v + 1] = off[v] + up[v].len());
        let head: Vec<u32> = up.into_iter().flatten().collect();
        let low: Vec<u32> = (0..n)
            .flat_map(|x| std::iter::repeat_n(x as u32, off[x + 1] - off[x]))
            .collect();
        let mut doff = vec![0; n + 1];
        head.iter().for_each(|&y| doff[y as usize + 1] += 1);
        (0..n).for_each(|v| doff[v + 1] += doff[v]);
        let (mut down, mut at) = (vec![0; head.len()], doff.clone());
        for (e, &y) in head.iter().enumerate() {
            down[at[y as usize]] = e as u32;
            at[y as usize] += 1;
        }
        let mut node = vec![0; n];
        for (v, &r) in rank.iter().enumerate() {
            node[r as usize] = v as u32;
        }
        let mut cch = Cch {
            rank,
            node,
            off,
            head,
            low,
            doff,
            down,
            parent,
            slot: Vec::new(),
            pieces,
        };
        cch.slot = (0..n)
            .flat_map(|u| g.arcs(u).map(move |(v, _)| (u, v)))
            .map(|(u, v)| {
                let (x, y) = (cch.rank[u], cch.rank[v as usize]);
                (if x < y {
                    2 * cch.edge(x, y)
                } else {
                    2 * cch.edge(y, x) + 1
                }) as u32
            })
            .collect();
        cch
    }

    /// The edge (x, y), x below y.
    fn edge(&self, x: u32, y: u32) -> usize {
        let r = self.off[x as usize]..self.off[x as usize + 1];
        r.start + self.head[r].binary_search(&y).unwrap()
    }

    /// Customize to the weights of `g`, which must have the arcs of the prepared graph in the same
    /// CSR order, on `threads` threads. Returns the metric and the core-seconds spent: the summed
    /// busy time of every thread. O(triangles of G⁺) work.
    pub fn customize(&self, g: &Graph, threads: usize) -> (Metric, f64) {
        let t0 = Instant::now();
        let arc = self.arcs(g);
        let mut w = vec![[f64::INFINITY; 2]; arc.len()];
        let (mut ranges, rest) = self.tasks(threads);
        // Rows of disjoint rank ranges are disjoint slices of w; bin b goes to thread b.
        ranges.sort_unstable();
        type Rows<'a> = Vec<(usize, usize, &'a mut [[f64; 2]])>;
        let mut bins: Vec<Rows> = (0..threads).map(|_| Vec::new()).collect();
        let (mut tail, mut at): (&mut [[f64; 2]], usize) = (&mut w, 0);
        for (lo, hi, b) in ranges {
            let (_, x) = std::mem::take(&mut tail).split_at_mut(self.off[lo] - at);
            let (chunk, y) = x.split_at_mut(self.off[hi] - self.off[lo]);
            bins[b].push((lo, hi, chunk));
            (tail, at) = (y, self.off[hi]);
        }
        let t1 = Instant::now();
        let busy: f64 = std::thread::scope(|scope| {
            let jobs: Vec<_> = bins
                .into_iter()
                .filter(|bin| !bin.is_empty())
                .map(|bin| {
                    let arc = &arc;
                    scope.spawn(move || {
                        let t = Instant::now();
                        for (lo, hi, w) in bin {
                            (lo..hi).for_each(|x| self.pull(x, w, self.off[lo], arc));
                        }
                        t.elapsed().as_secs_f64()
                    })
                })
                .collect();
            jobs.into_iter().map(|j| j.join().unwrap()).sum()
        });
        let parallel = t1.elapsed().as_secs_f64();
        rest.into_iter().for_each(|x| self.pull(x, &mut w, 0, &arc));
        // Core-seconds: the calling thread's own time plus each worker's busy time.
        let core = t0.elapsed().as_secs_f64() - parallel + busy;
        (Metric { w, arc }, core)
    }

    /// The input-arc values of every G⁺ edge under the weights of `g`, cheapest of parallel arcs.
    fn arcs(&self, g: &Graph) -> Vec<[f64; 2]> {
        assert_eq!(
            g.weights().len(),
            self.slot.len(),
            "graph does not match the hierarchy"
        );
        let mut arc = vec![[f64::INFINITY; 2]; self.head.len()];
        for (&s, &c) in self.slot.iter().zip(g.weights()) {
            let x = &mut arc[s as usize / 2][s as usize % 2];
            *x = x.min(c);
        }
        arc
    }

    /// Work for `threads` threads: with one thread, every node in rank order. Otherwise the rank
    /// ranges (lo, hi, thread) of the shallowest recorded dissection level with at least four pieces
    /// per thread, assigned largest edge count first to the least-loaded thread; and the nodes
    /// outside every range, ascending, for the calling thread afterwards.
    fn tasks(&self, threads: usize) -> (Vec<(usize, usize, usize)>, Vec<usize>) {
        let n = self.parent.len();
        if threads == 1 {
            return (Vec::new(), (0..n).collect());
        }
        let d = (0..=DEPTH)
            .find(|&d| self.pieces.iter().filter(|p| p[2] == d).count() >= 4 * threads)
            .unwrap_or(DEPTH);
        let size = |p: &[u32; 3]| self.off[p[1] as usize] - self.off[p[0] as usize];
        let mut pieces: Vec<&[u32; 3]> = self.pieces.iter().filter(|p| p[2] == d).collect();
        pieces.sort_by_key(|p| Reverse(size(p)));
        let mut load = vec![0; threads];
        let ranges: Vec<_> = pieces
            .into_iter()
            .map(|p| {
                let b = (0..threads).min_by_key(|&b| load[b]).unwrap();
                load[b] += size(p);
                (p[0] as usize, p[1] as usize, b)
            })
            .collect();
        let mut inside = vec![false; n];
        ranges
            .iter()
            .for_each(|&(lo, hi, _)| inside[lo..hi].fill(true));
        (ranges, (0..n).filter(|&x| !inside[x]).collect())
    }

    /// Recompute the upward edges of x by (basic). `w` holds the rows of x and of every node below x
    /// adjacent to it, from edge `base` on.
    fn pull(&self, x: usize, w: &mut [[f64; 2]], base: usize, arc: &[[f64; 2]]) {
        let r = self.off[x]..self.off[x + 1];
        w[r.start - base..r.end - base].copy_from_slice(&arc[r.clone()]);
        for &i in &self.down[self.doff[x]..self.doff[x + 1]] {
            let i = i as usize;
            // up(v) above x is a subset of up(x), both ascending: merge to find edge (x, y).
            let mut k = r.start;
            for j in i + 1..self.off[self.low[i] as usize + 1] {
                let y = self.head[j];
                while self.head[k] != y {
                    k += 1;
                }
                let ([vx, xv], [vy, yv]) = (w[i - base], w[j - base]);
                let e = &mut w[k - base];
                e[0] = e[0].min(xv + vy);
                e[1] = e[1].min(yv + vx);
            }
        }
    }

    /// The lower triangles of edge e = (x, y): pairs of edges ((v, x), (v, y)), v ascending.
    fn lower(&self, e: usize) -> impl Iterator<Item = (usize, usize)> + '_ {
        let (x, y) = (self.low[e] as usize, self.head[e]);
        self.down[self.doff[x]..self.doff[x + 1]]
            .iter()
            .filter_map(move |&i| {
                let r = self.off[self.low[i as usize] as usize + 1];
                let r = i as usize + 1..r;
                let j = r.start + self.head[r].binary_search(&y).ok()?;
                Some((i as usize, j))
            })
    }

    /// Bring `m`, customized to some metric, to the weights of `g`, given the input arcs (CSR
    /// indices) whose weight differs. Returns the number of edges recomputed.
    pub fn update(&self, m: &mut Metric, g: &Graph, changed: &[usize]) -> usize {
        let mut queued = vec![false; self.parent.len()];
        let mut heap = BinaryHeap::new();
        let mut mark = |x: u32, heap: &mut BinaryHeap<Reverse<u32>>| {
            if !std::mem::replace(&mut queued[x as usize], true) {
                heap.push(Reverse(x));
            }
        };
        for &a in changed {
            let e = self.slot[a] as usize / 2;
            let (x, y) = (
                self.node[self.low[e] as usize],
                self.node[self.head[e] as usize],
            );
            let cost = |u: u32, v: u32| {
                g.arcs(u as usize)
                    .filter(|&(h, _)| h == v)
                    .fold(f64::INFINITY, |c, (_, w)| c.min(w))
            };
            m.arc[e] = [cost(x, y), cost(y, x)];
            mark(self.low[e], &mut heap);
        }
        let (mut count, mut prev) = (0, Vec::new());
        while let Some(Reverse(x)) = heap.pop() {
            let r = self.off[x as usize]..self.off[x as usize + 1];
            prev.clear();
            prev.extend_from_slice(&m.w[r.clone()]);
            self.pull(x as usize, &mut m.w, 0, &m.arc);
            count += r.len();
            // Edges k = (x, y) and k2 = (x, z) are the lower edges of triangle {x, y, z}. Its top
            // edge {y, z} needs recomputing only if the triangle attained its weight before or
            // beats it now.
            let moved = |k: usize| prev[k - r.start] != m.w[k];
            if !r.clone().any(moved) {
                continue;
            }
            for k in r.clone() {
                // up(x) above y is a subset of up(y), both ascending: merge to find edge (y, z).
                let mut e = self.off[self.head[k] as usize];
                for k2 in k + 1..r.end {
                    while self.head[e] != self.head[k2] {
                        e += 1;
                    }
                    if !moved(k) && !moved(k2) {
                        continue;
                    }
                    // [up(y→z), down(z→y)] through x, before and after.
                    let via = |c: [f64; 2], d: [f64; 2]| [c[1] + d[0], d[1] + c[0]];
                    let a = via(prev[k - r.start], prev[k2 - r.start]);
                    let (b, c) = (via(m.w[k], m.w[k2]), m.w[e]);
                    if (0..2).any(|i| b[i] < c[i] || (a[i] == c[i] && b[i] > a[i])) {
                        mark(self.low[e], &mut heap);
                    }
                }
            }
        }
        count
    }

    /// Perfect customization of a basic metric and the edge directions a query still needs.
    pub fn perfect(&self, m: &Metric) -> Perfect {
        let n = self.parent.len();
        let mut w = m.w.clone();
        for v in (0..n).rev() {
            let r = self.off[v]..self.off[v + 1];
            for i in r.clone() {
                let mut k = self.off[self.head[i] as usize];
                for j in i + 1..r.end {
                    while self.head[k] != self.head[j] {
                        k += 1;
                    }
                    // i = (v, x), j = (v, y), k = (x, y), v < x < y; k is exact.
                    let ([vx, xv], [vy, yv], [xy, yx]) = (w[i], w[j], w[k]);
                    w[i] = [vx.min(vy + yx), xv.min(xy + yv)];
                    w[j] = [vy.min(vx + xy), yv.min(yx + xv)];
                }
            }
        }
        // A direction is redundant when two strictly shorter pieces of an intermediate or upper
        // triangle sum to it; strictness rules out removing both of two edges through each other.
        let mut keep: Vec<[bool; 2]> = w.iter().map(|c| c.map(f64::is_finite)).collect();
        let tie = |c: f64, a: f64, b: f64| a < c && b < c && a + b == c;
        for v in 0..n {
            let r = self.off[v]..self.off[v + 1];
            for i in r.clone() {
                let mut k = self.off[self.head[i] as usize];
                for j in i + 1..r.end {
                    while self.head[k] != self.head[j] {
                        k += 1;
                    }
                    let ([vx, xv], [vy, yv], [xy, yx]) = (w[i], w[j], w[k]);
                    // For (v, x) the triangle is upper (y above x); for (v, y), intermediate.
                    keep[i][0] &= !tie(vx, vy, yx);
                    keep[i][1] &= !tie(xv, xy, yv);
                    keep[j][0] &= !tie(vy, vx, xy);
                    keep[j][1] &= !tie(yv, yx, xv);
                }
            }
        }
        let rows = |d: usize| {
            let mut off = vec![0; n + 1];
            let (mut id, mut head, mut c) = (Vec::new(), Vec::new(), Vec::new());
            for x in 0..n {
                for e in self.off[x]..self.off[x + 1] {
                    if keep[e][d] {
                        id.push(e as u32);
                        head.push(self.head[e]);
                        c.push(w[e][d]);
                    }
                }
                off[x + 1] = head.len();
            }
            (off, id, head, c)
        };
        let ((o0, i0, h0, w0), (o1, i1, h1, w1)) = (rows(0), rows(1));
        Perfect {
            off: [o0, o1],
            id: [i0, i1],
            head: [h0, h1],
            w: [w0, w1],
        }
    }

    /// Query state for this hierarchy.
    pub fn workspace(&self) -> Workspace {
        let n = self.parent.len();
        Workspace {
            df: vec![f64::INFINITY; n],
            db: vec![f64::INFINITY; n],
            pf: vec![NONE; n],
            pb: vec![NONE; n],
        }
    }

    /// d(s, t) under metric `m`; infinity when t is unreachable. With `path`, it is overwritten by
    /// the nodes of one shortest s–t path, s first, or left empty when t is unreachable.
    pub fn query(
        &self,
        m: &Metric,
        q: &mut Workspace,
        s: usize,
        t: usize,
        path: Option<&mut Vec<u32>>,
    ) -> f64 {
        let rows = |x: usize| self.off[x]..self.off[x + 1];
        let edges = |d: usize, x: usize| rows(x).map(move |e| (e as u32, self.head[e], m.w[e][d]));
        self.search(m, q, s, t, path, edges)
    }

    /// The same on the edges kept by perfect customization `p` of basic metric `m`. Paths need
    /// positive weights: with a zero-weight piece a tie is kept, and its edge may lie below its
    /// basic weight.
    pub fn query_perfect(
        &self,
        p: &Perfect,
        m: &Metric,
        q: &mut Workspace,
        s: usize,
        t: usize,
        path: Option<&mut Vec<u32>>,
    ) -> f64 {
        let edges = |d: usize, x: usize| {
            (p.off[d][x]..p.off[d][x + 1]).map(move |e| (p.id[d][e], p.head[d][e], p.w[d][e]))
        };
        self.search(m, q, s, t, path, edges)
    }

    /// Elimination-tree search over `edges(direction, x)`, the upward edges (id, head, cost) of x,
    /// then the meeting node and, with `path`, the unpacked path through basic metric m.
    fn search<I>(
        &self,
        m: &Metric,
        q: &mut Workspace,
        s: usize,
        t: usize,
        path: Option<&mut Vec<u32>>,
        edges: impl Fn(usize, usize) -> I,
    ) -> f64
    where
        I: Iterator<Item = (u32, u32, f64)>,
    {
        let (rs, rt) = (self.rank[s], self.rank[t]);
        self.climb(|x| edges(0, x), &mut q.df, &mut q.pf, rs);
        self.climb(|x| edges(1, x), &mut q.db, &mut q.pb, rt);
        let (best, z) = self.meet(q, rs);
        if let Some(path) = path {
            path.clear();
            if best.is_finite() {
                // Edges from z down to t, then from z down to s: the stack pops s's side first.
                let mut stack = Vec::new();
                let mut x = z;
                while x != rt {
                    let e = q.pb[x as usize];
                    stack.push((e, 1));
                    x = self.low[e as usize];
                }
                stack.reverse();
                let mut x = z;
                while x != rs {
                    let e = q.pf[x as usize];
                    stack.push((e, 0));
                    x = self.low[e as usize];
                }
                path.push(s as u32);
                self.unpack(m, stack, path);
            }
        }
        self.reset(q, rs, rt);
        best
    }

    /// The least df + db over the ancestors of rs, and the ancestor attaining it.
    fn meet(&self, q: &Workspace, rs: u32) -> (f64, u32) {
        let (mut best, mut z) = (f64::INFINITY, NONE);
        let mut x = rs;
        while x != NONE {
            let d = q.df[x as usize] + q.db[x as usize];
            if d < best {
                (best, z) = (d, x);
            }
            x = self.parent[x as usize];
        }
        (best, z)
    }

    /// Clear the distances on the tree paths of rs and rt.
    fn reset(&self, q: &mut Workspace, rs: u32, rt: u32) {
        for (mut x, d) in [(rs, &mut q.df), (rt, &mut q.db)] {
            while x != NONE {
                d[x as usize] = f64::INFINITY;
                x = self.parent[x as usize];
            }
        }
    }

    /// Append the nodes reached along a stack of G⁺ edges (e, direction), popped in travel order.
    fn unpack(&self, m: &Metric, mut stack: Vec<(u32, usize)>, path: &mut Vec<u32>) {
        while let Some((e, dir)) = stack.pop() {
            let (e, x, y) = (e as usize, self.low[e as usize], self.head[e as usize]);
            if m.w[e][dir] == m.arc[e][dir] {
                path.push(self.node[if dir == 0 { y } else { x } as usize]);
                continue;
            }
            let (i, j) = self
                .lower(e)
                .find(|&(i, j)| {
                    let via = if dir == 0 {
                        m.w[i][1] + m.w[j][0]
                    } else {
                        m.w[j][1] + m.w[i][0]
                    };
                    via == m.w[e][dir]
                })
                .expect("shortcut without a supporting triangle");
            let (i, j) = (i as u32, j as u32);
            // x→y is x→v then v→y; y→x is y→v then v→x. The first half goes on top.
            stack.extend(if dir == 0 {
                [(j, 0), (i, 1)]
            } else {
                [(i, 0), (j, 1)]
            });
        }
    }

    /// Relax the upward edges (id, head, cost) of every ancestor of x in tree order, recording the
    /// edge that improves each node.
    fn climb<I>(&self, edges: impl Fn(usize) -> I, d: &mut [f64], p: &mut [u32], mut x: u32)
    where
        I: Iterator<Item = (u32, u32, f64)>,
    {
        d[x as usize] = 0.0;
        while x != NONE {
            let dx = d[x as usize];
            for (e, y, c) in edges(x as usize) {
                if dx + c < d[y as usize] {
                    (d[y as usize], p[y as usize]) = (dx + c, e);
                }
            }
            x = self.parent[x as usize];
        }
    }

    /// Elimination rank of node v.
    pub fn rank(&self, v: usize) -> usize {
        self.rank[v] as usize
    }

    /// Edges of G⁺.
    pub fn edges(&self) -> usize {
        self.head.len()
    }

    /// Height of the elimination tree, the most nodes a query visits from one end.
    pub fn height(&self) -> usize {
        let mut h = vec![0usize; self.parent.len()];
        for v in (0..self.parent.len()).rev() {
            let p = self.parent[v];
            h[v] = if p == NONE { 1 } else { h[p as usize] + 1 };
        }
        h.into_iter().max().unwrap_or(0)
    }

    /// Lower triangles of G⁺, the work of one basic customization.
    pub fn triangles(&self) -> usize {
        (0..self.head.len())
            .map(|i| self.off[self.low[i] as usize + 1] - i - 1)
            .sum()
    }
}

impl Perfect {
    /// Edge directions kept.
    pub fn edges(&self) -> usize {
        self.head[0].len() + self.head[1].len()
    }
}

/// Nested-dissection ranks: each piece's separator takes the highest free ranks, then its two sides
/// are dissected below it, so a piece owns the rank range [top − size, top) of its turn. A
/// graph-model fallback cut (adjacent terminal sets) becomes a node separator by moving the
/// source-side end of each cut edge into it. Also returns the pieces down to depth DEPTH.
fn dissect(nb: &[Vec<u32>], xy: &[(f64, f64)]) -> (Vec<u32>, Vec<[u32; 3]>) {
    let n = nb.len();
    let mut rank = vec![NONE; n];
    let mut top = n;
    let mut pos = vec![NONE; n];
    let mut pieces = Vec::new();
    let mut stack = vec![((0..n as u32).collect::<Vec<u32>>(), 0)];
    while let Some((piece, depth)) = stack.pop() {
        if depth <= DEPTH {
            pieces.push([(top - piece.len()) as u32, top as u32, depth]);
        }
        if piece.len() <= LEAF {
            for &v in &piece {
                top -= 1;
                rank[v as usize] = top as u32;
            }
            continue;
        }
        for (i, &v) in piece.iter().enumerate() {
            pos[v as usize] = i as u32;
        }
        let mut ends = Vec::new();
        for (i, &v) in piece.iter().enumerate() {
            for &y in &nb[v as usize] {
                let j = pos[y as usize];
                if j != NONE && j as usize > i {
                    ends.push((i, j as usize));
                }
            }
        }
        let mut side = separate(&piece, &ends, xy, Model::Hypergraph);
        for &(i, j) in &ends {
            match (side[i], side[j]) {
                (0, 1) => side[i] = 2,
                (1, 0) => side[j] = 2,
                _ => {}
            }
        }
        let mut parts = [Vec::new(), Vec::new()];
        for (&v, &c) in piece.iter().zip(&side) {
            pos[v as usize] = NONE;
            if c == 2 {
                top -= 1;
                rank[v as usize] = top as u32;
            } else {
                parts[c as usize].push(v);
            }
        }
        stack.extend(
            parts
                .into_iter()
                .filter(|p| !p.is_empty())
                .map(|p| (p, depth + 1)),
        );
    }
    (rank, pieces)
}

#[cfg(test)]
mod tests {
    use super::Cch;
    use crate::graph::{Graph, dijkstra};

    #[test]
    fn distances_and_paths_match_dijkstra_under_two_metrics() {
        let mut z = 7u64;
        let mut next = || {
            z ^= z << 13;
            z ^= z >> 7;
            z ^= z << 17;
            z
        };
        for n in [1usize, 2, 3, 5, 9, 17, 40] {
            let arcs: Vec<(u32, u32)> = (0..3 * n)
                .map(|_| ((next() % n as u64) as u32, (next() % n as u64) as u32))
                .collect();
            let xy: Vec<(f64, f64)> = (0..n)
                .map(|_| ((next() % 100) as f64, (next() % 100) as f64))
                .collect();
            let topo = Graph::new(n, arcs.iter().map(|&(u, v)| (u, v, 1.0)));
            let cch = Cch::new(&topo, &xy);
            for low in [0, 1] {
                let g = Graph::new(
                    n,
                    arcs.iter().map(|&(u, v)| (u, v, (low + next() % 9) as f64)),
                );
                let m = cch.customize(&g, 1).0;
                let p = cch.perfect(&m);
                let (mut q, mut path) = (cch.workspace(), Vec::new());
                // Cost on g of a node path from s to t, cheapest parallel arc per step.
                let cost = |path: &[u32], s: usize, t: usize| -> f64 {
                    if path.first() != Some(&(s as u32)) || path.last() != Some(&(t as u32)) {
                        return f64::INFINITY;
                    }
                    path.windows(2)
                        .map(|a| {
                            g.arcs(a[0] as usize)
                                .filter(|&(v, _)| v == a[1])
                                .fold(f64::INFINITY, |c, (_, w)| c.min(w))
                        })
                        .sum()
                };
                for s in 0..n {
                    let d = dijkstra(&g, s).d;
                    for (t, &d) in d.iter().enumerate() {
                        assert_eq!(cch.query(&m, &mut q, s, t, None), d, "n={n} {s}->{t}");
                        let mut route = Vec::new();
                        let perfect = cch.query_perfect(&p, &m, &mut q, s, t, Some(&mut route));
                        assert_eq!(perfect, d, "perfect {s}->{t}");
                        assert_eq!(cch.query(&m, &mut q, s, t, Some(&mut path)), d);
                        if d.is_infinite() {
                            assert!(path.is_empty() && route.is_empty());
                            continue;
                        }
                        if low > 0 {
                            assert_eq!(cost(&route, s, t), d, "perfect path n={n} {s}->{t}");
                        }
                        assert_eq!(cost(&path, s, t), d, "path cost n={n} {s}->{t}");
                    }
                }
            }
        }
    }
}
