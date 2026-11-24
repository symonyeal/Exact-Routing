//! Two-level overlays for exact point-to-point distance queries, built from a graph partition or
//! a hypergraph partition.
//!
//! Problem. Answer many s–t distance queries on one road graph. Cells partition the edges; a node
//! incident to edges of two cells, or to a cut edge, is a boundary node. The overlay stores, for
//! each cell, the in-cell distances between its boundary nodes, and the cut arcs. A query runs
//! Dijkstra over the arcs of the cells holding s and t plus the overlay (Delling, Goldberg, Pajor
//! and Werneck, customizable route planning, 2011). A shortest path splits at boundary nodes into
//! in-cell pieces and cut arcs, each represented, so the distance is exact.
//!
//! Partitions come from recursive inertial-flow bisection (Schild and Sommer 2015) until cells have
//! at most U nodes, under one of two cut models:
//!   graph       a minimum set of edges separates the source and sink nodes; the two sides are
//!               node sets, and the cut edges join them;
//!   hypergraph  a minimum set of nodes separates them; the sides are edge sets that share the
//!               separator. This is bisection of the dual hypergraph (one hyperedge per node, over
//!               its incident edges) under the connectivity metric, which counts boundary nodes
//!               directly.
//!
//! Legend
//!   U      maximum nodes per cell
//!   e      undirected edge {x, y}, x < y, of the road graph
//!   c(a)   cell of arc a; NONE for a cut arc
//!   B      boundary nodes

use crate::graph::{Graph, Key};
use std::cmp::Reverse;
use std::collections::BinaryHeap;

const NONE: u32 = u32::MAX;

/// The cut model of the bisection.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Model {
    Graph,
    Hypergraph,
}

/// Cells of the arcs, boundary nodes and overlay arcs.
pub struct Overlay {
    cell: Vec<u32>,
    home: Vec<u32>,
    off: Vec<usize>,
    head: Vec<u32>,
    w: Vec<f64>,
    pub cells: usize,
    pub boundary: usize,
}

impl Overlay {
    /// Overlay arcs stored.
    pub fn arcs(&self) -> usize {
        self.head.len()
    }

    /// Bytes in the stored array elements, excluding Vec headers, spare capacity and query state.
    pub fn bytes(&self) -> usize {
        4 * (self.cell.len() + self.home.len() + self.head.len())
            + size_of::<usize>() * self.off.len()
            + 8 * self.w.len()
    }

    /// Cell IDs for base arcs in graph CSR order; `None` marks a cut arc.
    pub fn arc_cells(&self) -> impl Iterator<Item = Option<u32>> + '_ {
        self.cell.iter().map(|&c| (c != NONE).then_some(c))
    }

    /// The cell containing an interior node, or `None` for a boundary or isolated node.
    pub fn interior_cell(&self, v: usize) -> Option<u32> {
        self.home.get(v).copied().filter(|&c| c < NONE - 1)
    }

    /// Whether `v` is a boundary node.
    pub fn is_boundary(&self, v: usize) -> bool {
        self.home.get(v) == Some(&NONE)
    }

    /// Boundary node IDs in ascending order.
    pub fn boundary_nodes(&self) -> impl Iterator<Item = usize> + '_ {
        self.home
            .iter()
            .enumerate()
            .filter_map(|(v, &c)| (c == NONE).then_some(v))
    }
}

/// Partition the edges of g into cells of at most `u` nodes and build the overlay. `xy` are planar
/// node coordinates for the inertial-flow projections.
pub fn build(g: &Graph, xy: &[(f64, f64)], u: usize, model: Model) -> Overlay {
    build_parallel(g, xy, u, model, 1)
}

/// Build the same overlay with at most `threads` independent customization workers. The partition
/// is deterministic and serial; each worker owns its search workspace and a disjoint set of cells.
pub fn build_parallel(
    g: &Graph,
    xy: &[(f64, f64)],
    u: usize,
    model: Model,
    threads: usize,
) -> Overlay {
    let n = g.n;
    assert!(u >= 2 && xy.len() == n && threads > 0);
    // Undirected edges and, for each arc, its edge.
    let mut keys: Vec<(u32, u32, usize)> = (0..n)
        .flat_map(|x| (g.off[x]..g.off[x + 1]).map(move |a| (x as u32, a)))
        .map(|(x, a)| (x.min(g.head[a]), x.max(g.head[a]), a))
        .collect();
    keys.sort_unstable();
    let (mut edges, mut edge) = (Vec::new(), vec![0u32; g.head.len()]);
    for &(x, y, a) in &keys {
        if edges.last() != Some(&(x, y)) {
            edges.push((x, y));
        }
        edge[a] = edges.len() as u32 - 1;
    }
    // Recursive bisection of edge sets; `owner` is each edge's cell, NONE once cut.
    let mut owner = vec![NONE; edges.len()];
    let mut cells = 0;
    let mut stack = vec![(0..edges.len() as u32).collect::<Vec<u32>>()];
    while let Some(piece) = stack.pop() {
        let nodes = endpoints(&edges, &piece);
        if nodes.len() <= u {
            piece.iter().for_each(|&e| owner[e as usize] = cells);
            cells += 1;
        } else {
            let (a, b) = bisect(&edges, &piece, &nodes, xy, model);
            stack.extend([a, b]);
        }
    }
    let cell: Vec<u32> = edge.iter().map(|&e| owner[e as usize]).collect();
    // A node is interior to one cell or a boundary node.
    let mut home = vec![NONE - 1; n]; // NONE - 1: no incident edge yet
    for (e, &(x, y)) in edges.iter().enumerate() {
        for v in [x, y] {
            let h = &mut home[v as usize];
            *h = if *h == NONE - 1 || *h == owner[e] {
                owner[e]
            } else {
                NONE
            };
        }
    }
    let on = |v: usize| home[v] == NONE;
    // Cliques: in-cell distances between the boundary nodes of each cell, by Dijkstra from each.
    let mut members: Vec<Vec<u32>> = vec![Vec::new(); cells as usize];
    for (e, &(x, y)) in edges.iter().enumerate() {
        if owner[e] != NONE {
            for v in [x, y].into_iter().filter(|&v| on(v as usize)) {
                members[owner[e] as usize].push(v);
            }
        }
    }
    for b in &mut members {
        b.sort_unstable();
        b.dedup();
    }
    let work = |lo: usize, cells: &[Vec<u32>]| {
        let mut over = Vec::new();
        let mut search = Search::new(n);
        for (i, b) in cells.iter().enumerate() {
            for &x in b {
                search.run(g, x as usize, None, |a| cell[a] == (lo + i) as u32, None);
                over.extend(
                    b.iter()
                        .filter(|&&y| y != x && search.d[y as usize] < f64::INFINITY)
                        .map(|&y| (x, y, search.d[y as usize])),
                );
            }
        }
        over
    };
    let mut over = if threads == 1 || members.is_empty() {
        work(0, &members)
    } else {
        let chunk = members.len().div_ceil(threads).max(1);
        std::thread::scope(|scope| {
            let jobs: Vec<_> = members
                .chunks(chunk)
                .enumerate()
                .map(|(i, b)| {
                    let work = &work;
                    scope.spawn(move || work(i * chunk, b))
                })
                .collect();
            jobs.into_iter()
                .flat_map(|job| job.join().unwrap())
                .collect()
        })
    };
    for x in 0..n {
        let r = g.off[x]..g.off[x + 1];
        let arcs = cell[r.clone()].iter().zip(&g.head[r.clone()]).zip(&g.w[r]);
        over.extend(
            arcs.filter(|a| *a.0.0 == NONE)
                .map(|((_, &y), &w)| (x as u32, y, w)),
        );
    }
    let boundary = (0..n).filter(|&v| on(v)).count();
    let o = Graph::new(n, over);
    Overlay {
        cell,
        home,
        off: o.off,
        head: o.head,
        w: o.w,
        cells: cells as usize,
        boundary,
    }
}

/// Nodes incident to the edges of a piece, ascending.
fn endpoints(edges: &[(u32, u32)], piece: &[u32]) -> Vec<u32> {
    let mut v: Vec<u32> = piece
        .iter()
        .flat_map(|&e| [edges[e as usize].0, edges[e as usize].1])
        .collect();
    v.sort_unstable();
    v.dedup();
    v
}

/// Split a piece's edges in two by the best of four inertial-flow cuts: project the nodes on a
/// direction, take the first and last quarters as sources and sinks, and cut a minimum set of edges
/// (graph model) or nodes (hypergraph model) between them. Graph-model cut edges are dropped.
fn bisect(
    edges: &[(u32, u32)],
    piece: &[u32],
    nodes: &[u32],
    xy: &[(f64, f64)],
    model: Model,
) -> (Vec<u32>, Vec<u32>) {
    let local = |v: u32| nodes.binary_search(&v).unwrap();
    let ends: Vec<(usize, usize)> = piece
        .iter()
        .map(|&e| (local(edges[e as usize].0), local(edges[e as usize].1)))
        .collect();
    let k = nodes.len().div_ceil(4);
    let mut best: Option<(usize, Vec<u8>)> = None;
    for (p, q) in [(1.0, 0.0), (0.0, 1.0), (1.0, 1.0), (1.0, -1.0)] {
        let mut order: Vec<usize> = (0..nodes.len()).collect();
        let proj = |i: usize| p * xy[nodes[i] as usize].0 + q * xy[nodes[i] as usize].1;
        order.sort_by(|&i, &j| proj(i).total_cmp(&proj(j)));
        let (size, side) = cut(
            nodes.len(),
            &ends,
            &order[..k],
            &order[nodes.len() - k..],
            model,
        );
        if best.as_ref().is_none_or(|b| size < b.0) {
            best = Some((size, side));
        }
    }
    // side: 0 source side, 1 sink side, 2 separator node (hypergraph model).
    let side = best.unwrap().1;
    let (mut a, mut b) = (Vec::new(), Vec::new());
    for (&e, &(x, y)) in piece.iter().zip(&ends) {
        match (side[x], side[y]) {
            (0, 0) | (0, 2) | (2, 0) | (2, 2) => a.push(e),
            (1, 1) | (1, 2) | (2, 1) => b.push(e),
            _ => {} // a cut edge
        }
    }
    (a, b)
}

/// Minimum cut between node sets s and t by Dinic's blocking flows (1970), with
/// unit capacities on edges (graph model) or nodes (hypergraph model, nodes split into in and out
/// copies; where a source and a sink are adjacent no node cut exists and an edge cut is taken).
/// Paths are augmented by their bottleneck. Returns the cut size and each node's side.
fn cut(
    n: usize,
    ends: &[(usize, usize)],
    s: &[usize],
    t: &[usize],
    model: Model,
) -> (usize, Vec<u8>) {
    const INF: i64 = i64::MAX / 4;
    let split = model == Model::Hypergraph;
    let io = |v: usize| if split { (2 * v, 2 * v + 1) } else { (v, v) }; // in and out copies
    let (src, snk) = (
        if split { 2 * n } else { n },
        if split { 2 * n + 1 } else { n + 1 },
    );
    // Arcs in pairs: arc i and its reverse i ^ 1.
    let (mut to, mut cap, mut adj) = (Vec::new(), Vec::new(), vec![Vec::new(); snk + 1]);
    let mut arc = |x: usize, y: usize, c: i64, r: i64| {
        for (p, q, k) in [(x, y, c), (y, x, r)] {
            adj[p].push(to.len());
            to.push(q);
            cap.push(k);
        }
    };
    let terminal: Vec<bool> = {
        let mut z = vec![false; n];
        s.iter().chain(t).for_each(|&v| z[v] = true);
        z
    };
    if split {
        for (v, &z) in terminal.iter().enumerate() {
            arc(2 * v, 2 * v + 1, if z { INF } else { 1 }, 0);
        }
    }
    for &(x, y) in ends {
        if split {
            arc(io(x).1, io(y).0, INF, 0);
            arc(io(y).1, io(x).0, INF, 0);
        } else {
            arc(x, y, 1, 1);
        }
    }
    s.iter().for_each(|&v| arc(src, io(v).0, INF, 0));
    t.iter().for_each(|&v| arc(io(v).1, snk, INF, 0));
    let mut flow = 0;
    let mut level = vec![usize::MAX; snk + 1];
    let mut cursor = vec![0; snk + 1];
    let mut path = Vec::new();
    loop {
        level.fill(usize::MAX);
        level[src] = 0;
        let mut queue = std::collections::VecDeque::from([src]);
        while let Some(x) = queue.pop_front() {
            for &i in &adj[x] {
                if cap[i] > 0 && level[to[i]] == usize::MAX {
                    level[to[i]] = level[x] + 1;
                    queue.push_back(to[i]);
                }
            }
        }
        if level[snk] == usize::MAX {
            break;
        }
        cursor.fill(0);
        // Iterative level-graph DFS avoids a call stack proportional to road-network diameter.
        loop {
            path.clear();
            let mut x = src;
            while x != snk {
                while cursor[x] < adj[x].len() {
                    let i = adj[x][cursor[x]];
                    if cap[i] > 0 && level[to[i]] == level[x] + 1 {
                        break;
                    }
                    cursor[x] += 1;
                }
                if cursor[x] == adj[x].len() {
                    if let Some(i) = path.pop() {
                        x = to[i ^ 1];
                        cursor[x] += 1;
                    } else {
                        break;
                    }
                } else {
                    let i = adj[x][cursor[x]];
                    path.push(i);
                    x = to[i];
                }
            }
            if x != snk {
                break;
            }
            let b = path.iter().map(|&i| cap[i]).min().unwrap();
            if b == INF {
                // Adjacent terminal sets cannot be separated by nonterminal nodes.
                return cut(n, ends, s, t, Model::Graph);
            }
            path.iter()
                .for_each(|&i| (cap[i], cap[i ^ 1]) = (cap[i] - b, cap[i ^ 1] + b));
            flow += b as usize;
        }
    }
    // The last BFS exhausts the source's residual component, hence identifies a minimum cut.
    let reach = |x: usize| level[x] != usize::MAX;
    let side = (0..n)
        .map(|v| match (reach(io(v).0), reach(io(v).1)) {
            (true, true) => 0,
            (true, false) => 2,
            _ => 1,
        })
        .collect();
    (flow, side)
}

/// Dijkstra with reusable buffers, restricted to the arcs `allowed` admits plus optional overlay
/// arcs at boundary nodes, stopping when t is settled. Only touched entries are reset. `settled`
/// and `scanned` count the nodes settled and arcs examined by the last search.
pub struct Search {
    pub d: Vec<f64>,
    done: Vec<bool>,
    touched: Vec<u32>,
    heap: BinaryHeap<Reverse<(Key, u32)>>,
    pub settled: usize,
    pub scanned: usize,
}

impl Search {
    pub fn new(n: usize) -> Search {
        Search {
            d: vec![f64::INFINITY; n],
            done: vec![false; n],
            touched: Vec::new(),
            heap: BinaryHeap::new(),
            settled: 0,
            scanned: 0,
        }
    }

    /// d(s, t) when t is given, else the distances from s; overlay arcs are taken at nodes with
    /// `overlay` set.
    pub fn run(
        &mut self,
        g: &Graph,
        s: usize,
        t: Option<usize>,
        allowed: impl Fn(usize) -> bool,
        o: Option<&Overlay>,
    ) -> f64 {
        for &v in &self.touched {
            (self.d[v as usize], self.done[v as usize]) = (f64::INFINITY, false);
        }
        self.touched.clear();
        (self.settled, self.scanned) = (0, 0);
        self.heap.clear();
        self.heap.push(Reverse((Key(0.0), s as u32)));
        self.d[s] = 0.0;
        self.touched.push(s as u32);
        while let Some(Reverse((Key(dx), x))) = self.heap.pop() {
            let x = x as usize;
            if std::mem::replace(&mut self.done[x], true) {
                continue;
            }
            self.settled += 1;
            if Some(x) == t {
                return dx;
            }
            let over = o.filter(|o| o.home[x] == NONE);
            let ext = over.map_or(0..0, |o| o.off[x]..o.off[x + 1]);
            self.scanned += g.off[x + 1] - g.off[x] + ext.len();
            let base = (g.off[x]..g.off[x + 1])
                .filter(|&a| allowed(a))
                .map(|a| (g.head[a], g.w[a]));
            let extra = over
                .into_iter()
                .flat_map(|o| ext.clone().map(move |a| (o.head[a], o.w[a])));
            for (y, w) in base.chain(extra) {
                let y = y as usize;
                if dx + w < self.d[y] {
                    if self.d[y] == f64::INFINITY {
                        self.touched.push(y as u32);
                    }
                    self.d[y] = dx + w;
                    self.heap.push(Reverse((Key(dx + w), y as u32)));
                }
            }
        }
        t.map_or(0.0, |t| self.d[t])
    }

    /// d(s, t) on the arcs of the cells of s and t and the overlay.
    pub fn query(&mut self, g: &Graph, o: &Overlay, s: usize, t: usize) -> f64 {
        let (cs, ct) = (o.home[s], o.home[t]);
        self.run(
            g,
            s,
            Some(t),
            |a| o.cell[a] != NONE && (o.cell[a] == cs || o.cell[a] == ct),
            Some(o),
        )
    }

    /// Nodes settled by the most recent run, in ascending ID order.
    pub fn settled_nodes(&self) -> impl Iterator<Item = usize> + '_ {
        self.done
            .iter()
            .enumerate()
            .filter_map(|(v, &done)| done.then_some(v))
    }

    /// Number of nodes whose distance state will be reset before the next run.
    pub fn touched_count(&self) -> usize {
        self.touched.len()
    }
}

#[cfg(test)]
mod tests {
    use super::{Model, Search, build, cut};
    use crate::graph::Graph;

    #[test]
    fn diagnostic_getters_match_search_state_and_overlay_partition() {
        let g = Graph::new(
            6,
            [
                (0, 1, 0.0),
                (1, 2, 2.0),
                (2, 0, 1.0),
                (2, 3, 1.0),
                (3, 4, 1.0),
            ],
        );
        let xy = vec![(0.0, 0.0); g.n];
        for model in [Model::Graph, Model::Hypergraph] {
            let o = build(&g, &xy, 2, model);
            let boundaries: Vec<_> = o.boundary_nodes().collect();
            assert_eq!(boundaries.len(), o.boundary);
            assert!(boundaries.windows(2).all(|w| w[0] < w[1]));
            assert!(
                boundaries
                    .iter()
                    .all(|&v| o.is_boundary(v) && o.interior_cell(v).is_none())
            );
            assert!((0..g.n).all(|v| !o.is_boundary(v) || o.interior_cell(v).is_none()));
            assert_eq!(o.arc_cells().count(), g.head.len());
            assert!(o.arc_cells().flatten().all(|c| (c as usize) < o.cells));
            assert!((0..g.n).any(|v| o.interior_cell(v).is_some()));
            assert!(o.interior_cell(5).is_none());

            let mut q = Search::new(g.n);
            assert_eq!(q.query(&g, &o, 0, 3), 3.0);
            let settled: Vec<_> = q.settled_nodes().collect();
            assert_eq!(settled.len(), q.settled);
            assert!(q.touched_count() >= q.settled);
            assert!(settled.contains(&3));
            assert_eq!(q.query(&g, &o, 5, 5), 0.0);
            assert_eq!(q.settled_nodes().collect::<Vec<_>>(), vec![5]);
            assert_eq!(q.touched_count(), 1);
            assert!(q.query(&g, &o, 5, 0).is_infinite());
            assert_eq!(q.settled_nodes().count(), q.settled);
        }
    }

    #[test]
    fn blocking_flow_cuts_match_exhaustive_separators() {
        for n in 2..=6 {
            let edges: Vec<_> = (0..n)
                .flat_map(|u| (u + 1..n).map(move |v| (u, v)))
                .collect();
            for seed in 0..128usize {
                let bits = if n <= 4 {
                    seed
                } else {
                    seed.wrapping_mul(0x9e3779b9)
                };
                let ends: Vec<_> = edges
                    .iter()
                    .enumerate()
                    .filter(|(i, _)| bits & (1 << i) != 0)
                    .map(|(_, e)| *e)
                    .collect();
                let terminals: Vec<_> = if n >= 4 {
                    vec![(vec![0], vec![n - 1]), (vec![0, 1], vec![n - 2, n - 1])]
                } else {
                    vec![(vec![0], vec![n - 1])]
                };
                for (s, t) in terminals {
                    let edge_cut = (0..1usize << n)
                        .filter(|mask| {
                            s.iter().all(|&v| mask & (1 << v) != 0)
                                && t.iter().all(|&v| mask & (1 << v) == 0)
                        })
                        .map(|mask| {
                            ends.iter()
                                .filter(|&&(u, v)| (mask >> u) & 1 != (mask >> v) & 1)
                                .count()
                        })
                        .min()
                        .unwrap();
                    assert_eq!(cut(n, &ends, &s, &t, Model::Graph).0, edge_cut);
                    let node_cut = (0..1usize << n)
                        .filter(|mask| s.iter().chain(&t).all(|&v| mask & (1 << v) == 0))
                        .filter(|mask| {
                            let mut seen = vec![false; n];
                            let mut stack = s.clone();
                            for &v in &s {
                                seen[v] = true;
                            }
                            while let Some(u) = stack.pop() {
                                for &(a, b) in &ends {
                                    let v = if a == u {
                                        b
                                    } else if b == u {
                                        a
                                    } else {
                                        continue;
                                    };
                                    if !seen[v] && mask & (1 << v) == 0 {
                                        seen[v] = true;
                                        stack.push(v);
                                    }
                                }
                            }
                            t.iter().all(|&v| !seen[v])
                        })
                        .map(|mask| mask.count_ones() as usize)
                        .min();
                    assert_eq!(
                        cut(n, &ends, &s, &t, Model::Hypergraph).0,
                        node_cut.unwrap_or(edge_cut),
                        "n={n}, seed={seed}"
                    );
                }
            }
        }
    }
}
