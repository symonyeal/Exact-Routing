//! Recursive inertial-flow bisection of a road graph's edges (Schild and Sommer 2015), the
//! separator routine behind the CCH order and the multilevel overlay cells.
//!
//! Each step projects the nodes on four directions, takes the first and last quarters as sources
//! and sinks, and cuts a minimum set of nodes between them (Dinic's blocking flows on split nodes).
//! Separating nodes is bisection of the dual hypergraph (one hyperedge per node, over its incident
//! edges) under the connectivity metric, so a cut counts boundary junctions directly. Where a
//! source and a sink are adjacent no node cut exists, and a minimum edge cut is taken instead.
//!
//! Legend
//!   U      maximum nodes per cell
//!   e      undirected edge {x, y}, x < y, of the road graph
//!   B      boundary nodes: incident to edges of two cells, or to a cut edge

use crate::graph::Graph;

const NONE: u32 = u32::MAX;

/// The cut model of one minimum cut: nodes separate the sides, or (fallback) edges join them.
#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) enum Model {
    Graph,
    Hypergraph,
}

/// A metric-independent partition: the cell of each arc in CSR order (NONE when cut), the cell of
/// each interior node (NONE for a boundary node), and the boundary nodes of each cell.
pub(crate) struct Partition {
    pub(crate) cell: Vec<u32>,
    pub(crate) home: Vec<u32>,
    pub(crate) members: Vec<Vec<u32>>,
}

/// The partition whose undirected edge e lies in cell owner[e] (NONE when cut), of `cells` cells.
pub(crate) fn assemble(
    n: usize,
    edges: &[(u32, u32)],
    edge: &[u32],
    owner: &[u32],
    cells: u32,
) -> Partition {
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
    Partition {
        cell,
        home,
        members,
    }
}

/// The undirected edges {x, y}, x < y, of g, ascending, and for each arc in CSR order its edge.
pub(crate) fn undirected(g: &Graph) -> (Vec<(u32, u32)>, Vec<u32>) {
    let mut keys: Vec<(u32, u32, usize)> = (0..g.n)
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
    (edges, edge)
}

/// Nodes incident to the edges of a piece, ascending.
pub(crate) fn endpoints(edges: &[(u32, u32)], piece: &[u32]) -> Vec<u32> {
    let mut v: Vec<u32> = piece
        .iter()
        .flat_map(|&e| [edges[e as usize].0, edges[e as usize].1])
        .collect();
    v.sort_unstable();
    v.dedup();
    v
}

/// Split a piece's edges in two by the best of four inertial-flow cuts (see `separate`); graph-model
/// cut edges are dropped.
pub(crate) fn bisect(
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
    let side = separate(nodes, &ends, xy, model);
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

/// The side of each node of `nodes` (local edges `ends`) in the best of four inertial-flow cuts:
/// project the nodes on a direction, take the first and last quarters as sources and sinks, and cut
/// a minimum set of edges (graph model) or nodes (hypergraph model) between them. Side 0 is the
/// source side, 1 the sink side, 2 a separator node (hypergraph model).
pub(crate) fn separate(
    nodes: &[u32],
    ends: &[(usize, usize)],
    xy: &[(f64, f64)],
    model: Model,
) -> Vec<u8> {
    let k = nodes.len().div_ceil(4);
    let mut best: Option<(usize, Vec<u8>)> = None;
    for (p, q) in [(1.0, 0.0), (0.0, 1.0), (1.0, 1.0), (1.0, -1.0)] {
        let mut order: Vec<usize> = (0..nodes.len()).collect();
        let proj = |i: usize| p * xy[nodes[i] as usize].0 + q * xy[nodes[i] as usize].1;
        order.sort_by(|&i, &j| proj(i).total_cmp(&proj(j)));
        let (size, side) = cut(
            nodes.len(),
            ends,
            &order[..k],
            &order[nodes.len() - k..],
            model,
        );
        if best.as_ref().is_none_or(|b| size < b.0) {
            best = Some((size, side));
        }
    }
    best.unwrap().1
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
            if !flow_path(src, snk, (&adj, &to, &cap), &level, &mut cursor, &mut path) {
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

/// Find one path in the current level graph, retaining the per-node cursors.
fn flow_path(
    src: usize,
    snk: usize,
    network: (&[Vec<usize>], &[usize], &[i64]),
    level: &[usize],
    cursor: &mut [usize],
    path: &mut Vec<usize>,
) -> bool {
    let (adj, to, cap) = network;
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
    x == snk
}

#[cfg(test)]
mod tests {
    use super::{Model, cut};

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
