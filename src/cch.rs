//! Customizable contraction hierarchies (Dibbelt, Strasser and Wagner, "Customizable Contraction
//! Hierarchies", ACM JEA 21, 2016) for exact point-to-point shortest paths under changing weights.
//!
//! Problem. Answer s–t queries on a directed graph whose weights change while its arcs do not.
//! Preparation reads the topology only: order the nodes by nested dissection and eliminate them in
//! that order, giving the chordal supergraph G⁺ of the undirected graph. Customization reads one
//! metric: give every G⁺ edge {x, y}, x ranked below y, the weights up(x→y) and down(y→x) of the
//! input arcs (∞ where none exists), then for each node v in rank order and each pair x < y of its
//! upper neighbours relax both directions of {x, y} through v (basic customization over lower
//! triangles). Afterwards up(x→y) is the shortest x–y distance over paths whose interior nodes rank
//! below x, so every shortest s–t path has an up-down representation over G⁺ with equal cost.
//!
//! Query (elimination-tree search, Bauer, Columbus, Rutter and Wagner 2016). The upper neighbours of
//! v are its ancestors in the elimination tree, whose parent of v is its lowest upper neighbour.
//! Relaxing the upward edges of every ancestor of s in tree order gives d↑(s, z); the same from t on
//! the down weights gives d↓(z, t); d(s, t) is the minimum of their sum over the common ancestors z.
//! A path is unpacked edge by edge: an edge whose weight is still its input arc's is that arc;
//! otherwise a lower triangle v attains it, and the edge expands to its two edges through v.
//!
//! Legend
//!   rank, node  position of each node in the elimination order, and its inverse; G⁺ is in rank space
//!   up          edges of x lead to its upper neighbours, ascending (off, head)
//!   low         lower end x of each edge; down lists the edges (v, x), v below x (doff, down)
//!   w[e]        [up(x→y), down(y→x)] for edge e = (x, y), x below y; arc[e] the input-arc values
//!   parent      elimination-tree parent, NONE at a root
//!   slot        for each input arc in CSR order, 2e + direction of the G⁺ edge it initializes

use crate::graph::Graph;
use crate::overlay::{Model, separate};

const NONE: u32 = u32::MAX;
/// Pieces of at most this many nodes are ordered without further dissection.
const LEAF: usize = 2;

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
}

/// Customized weights and the input-arc weights they started from, [up, down] per G⁺ edge.
pub struct Metric {
    w: Vec<[f64; 2]>,
    arc: Vec<[f64; 2]>,
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
        let rank = dissect(&nb, xy);
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
        let edge = |x: u32, y: u32| {
            let r = off[x as usize]..off[x as usize + 1];
            r.start + head[r].binary_search(&y).unwrap()
        };
        let slot = (0..n)
            .flat_map(|u| g.arcs(u).map(move |(v, _)| (u, v)))
            .map(|(u, v)| {
                let (x, y) = (rank[u], rank[v as usize]);
                (if x < y {
                    2 * edge(x, y)
                } else {
                    2 * edge(y, x) + 1
                }) as u32
            })
            .collect();
        Cch {
            rank,
            node,
            off,
            head,
            low,
            doff,
            down,
            parent,
            slot,
        }
    }

    /// Customize to the weights of `g`, which must have the arcs of the prepared graph in the same
    /// CSR order. O(triangles of G⁺).
    pub fn customize(&self, g: &Graph) -> Metric {
        assert_eq!(
            g.weights().len(),
            self.slot.len(),
            "graph does not match the hierarchy"
        );
        let mut w = vec![[f64::INFINITY; 2]; self.head.len()];
        for (&s, &c) in self.slot.iter().zip(g.weights()) {
            let x = &mut w[s as usize / 2][s as usize % 2];
            *x = x.min(c);
        }
        let arc = w.clone();
        for v in 0..self.parent.len() {
            let r = self.off[v]..self.off[v + 1];
            for i in r.clone() {
                let x = self.head[i] as usize;
                // up(v) above x is a subset of up(x), both ascending: merge to find edge (x, y).
                let mut k = self.off[x];
                for j in i + 1..r.end {
                    let y = self.head[j];
                    while self.head[k] != y {
                        k += 1;
                    }
                    let ([vx, xv], [vy, yv]) = (w[i], w[j]);
                    let e = &mut w[k];
                    e[0] = e[0].min(xv + vy);
                    e[1] = e[1].min(yv + vx);
                }
            }
        }
        Metric { w, arc }
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
        let (rs, rt) = (self.rank[s], self.rank[t]);
        self.climb(&m.w, &mut q.df, &mut q.pf, rs, 0);
        self.climb(&m.w, &mut q.db, &mut q.pb, rt, 1);
        let (mut best, mut z) = (f64::INFINITY, NONE);
        let mut x = rs;
        while x != NONE {
            let d = q.df[x as usize] + q.db[x as usize];
            if d < best {
                (best, z) = (d, x);
            }
            x = self.parent[x as usize];
        }
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
        for (mut x, d) in [(rs, &mut q.df), (rt, &mut q.db)] {
            while x != NONE {
                d[x as usize] = f64::INFINITY;
                x = self.parent[x as usize];
            }
        }
        best
    }

    /// Append the nodes reached along a stack of G⁺ edges (e, direction), popped in travel order.
    fn unpack(&self, m: &Metric, mut stack: Vec<(u32, usize)>, path: &mut Vec<u32>) {
        while let Some((e, dir)) = stack.pop() {
            let (e, x, y) = (e as usize, self.low[e as usize], self.head[e as usize]);
            if m.w[e][dir] == m.arc[e][dir] {
                path.push(self.node[if dir == 0 { y } else { x } as usize]);
                continue;
            }
            let (i, j) = self.down[self.doff[x as usize]..self.doff[x as usize + 1]]
                .iter()
                .find_map(|&i| {
                    let (i, v) = (i as usize, self.low[i as usize] as usize);
                    let r = self.off[v]..self.off[v + 1];
                    let j = r.start + self.head[r].binary_search(&y).ok()?;
                    let via = if dir == 0 {
                        m.w[i][1] + m.w[j][0]
                    } else {
                        m.w[j][1] + m.w[i][0]
                    };
                    (via == m.w[e][dir]).then_some((i as u32, j as u32))
                })
                .expect("shortcut without a supporting triangle");
            // x→y is x→v then v→y; y→x is y→v then v→x. The first half goes on top.
            stack.extend(if dir == 0 {
                [(j, 0), (i, 1)]
            } else {
                [(i, 0), (j, 1)]
            });
        }
    }

    /// Relax the upward edges of every ancestor of x in tree order, in direction `dir`, recording
    /// the edge that improves each node.
    fn climb(&self, w: &[[f64; 2]], d: &mut [f64], p: &mut [u32], mut x: u32, dir: usize) {
        d[x as usize] = 0.0;
        while x != NONE {
            let (dx, r) = (
                d[x as usize],
                self.off[x as usize]..self.off[x as usize + 1],
            );
            for ((e, &y), c) in r.clone().zip(&self.head[r.clone()]).zip(&w[r]) {
                if dx + c[dir] < d[y as usize] {
                    (d[y as usize], p[y as usize]) = (dx + c[dir], e as u32);
                }
            }
            x = self.parent[x as usize];
        }
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
}

/// Nested-dissection ranks: each piece's separator takes the highest free ranks, then its two sides
/// are dissected below it. A graph-model fallback cut (adjacent terminal sets) becomes a node
/// separator by moving the source-side end of each cut edge into it.
fn dissect(nb: &[Vec<u32>], xy: &[(f64, f64)]) -> Vec<u32> {
    let n = nb.len();
    let mut rank = vec![NONE; n];
    let mut top = n;
    let mut pos = vec![NONE; n];
    let mut stack = vec![(0..n as u32).collect::<Vec<u32>>()];
    while let Some(piece) = stack.pop() {
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
        stack.extend(parts.into_iter().filter(|p| !p.is_empty()));
    }
    rank
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
            for _ in 0..2 {
                let g = Graph::new(n, arcs.iter().map(|&(u, v)| (u, v, (next() % 9) as f64)));
                let m = cch.customize(&g);
                let (mut q, mut path) = (cch.workspace(), Vec::new());
                for s in 0..n {
                    let d = dijkstra(&g, s).d;
                    for (t, &d) in d.iter().enumerate() {
                        assert_eq!(cch.query(&m, &mut q, s, t, None), d, "n={n} {s}->{t}");
                        assert_eq!(cch.query(&m, &mut q, s, t, Some(&mut path)), d);
                        if d.is_infinite() {
                            assert!(path.is_empty());
                            continue;
                        }
                        assert_eq!((path[0], *path.last().unwrap()), (s as u32, t as u32));
                        let cost: f64 = path
                            .windows(2)
                            .map(|a| {
                                g.arcs(a[0] as usize)
                                    .filter(|&(v, _)| v == a[1])
                                    .map(|(_, w)| w)
                                    .fold(f64::INFINITY, f64::min)
                            })
                            .sum();
                        assert_eq!(cost, d, "path cost n={n} {s}->{t}");
                    }
                }
            }
        }
    }
}
