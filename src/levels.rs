//! Multilevel overlays for exact point-to-point distance queries (customizable route planning:
//! Delling, Goldberg, Pajor and Werneck, Transportation Science 51, 2017) on a nested junction-cut
//! partition.
//!
//! Problem. Answer many s–t distance queries on one road graph. Cells partition the edges; a node
//! incident to edges of two cells, or to a cut edge, is a boundary node. There are L nested levels
//! of cells; level 0 is the finest. Recursive
//! inertial-flow bisection of the edge set under the hypergraph (junction-cut) model, continued
//! until pieces have at most U₀ nodes, gives every level at once: the level-l cell of a branch is
//! its first piece with at most U_l nodes, so cells nest.
//!
//! Customization, bottom up. The clique of a level-l cell c holds the distances between its
//! level-l boundary nodes inside c: over its base arcs at level 0, and above that over the cliques
//! of its level-(l−1) subcells and the arcs cut inside c. An arc dropped by a fallback edge cut is in
//! no cell at the levels below the cut; it joins the overlay of the highest level l at which it is
//! cut, as a cut arc of the level-(l+1) cell containing it.
//!
//! Query. Dijkstra from s to t over: base arcs of the level-0 cells of s and t; at each level l, the
//! overlay arcs of cells other than the level-l cells of s and t whose level-(l+1) cell is that of s
//! or t (every cell at the top level), and cut arcs by the same rule on their level-(l+1) cell. A
//! shortest path splits into pieces each represented at exactly one of these levels, so the
//! distance is exact. A node is scanned at level l only when it is a level-l boundary node.
//!
//! Legend
//!   L, U_l      levels and the node bound of level l, ascending in l
//!   lv[l]       the level-l partition: cell of each base arc (NONE when cut), home cell of each
//!               node (NONE on the level-l boundary), boundary nodes of each cell
//!   up[l][c]    level-(l+1) cell containing level-l cell c; 0 at the top level
//!   own, group  of a level-l overlay arc: its level-l cell (CUT for a cut arc) and the
//!               level-(l+1) cell containing it

use crate::graph::{Graph, Key, Search};
use crate::partition::{Model, Partition, assemble, bisect, endpoints, undirected};
use std::cmp::Reverse;
use std::collections::BinaryHeap;

const NONE: u32 = u32::MAX;
const CUT: u32 = u32::MAX - 2;

/// A metric-independent nested partition.
pub struct Nest {
    lv: Vec<Partition>,
    up: Vec<Vec<u32>>,
}

/// The overlay arcs of one level, with their own and group cells.
struct Level {
    g: Graph,
    own: Vec<u32>,
    group: Vec<u32>,
}

/// A nested partition customized to one metric.
pub struct Multi {
    lv: Vec<Level>,
}

/// Partition the edges of g into nested cells of at most `us[l]` nodes, `us` strictly ascending,
/// by one recursive inertial-flow bisection under the junction-cut model. Reads endpoints only.
pub fn nest(g: &Graph, xy: &[(f64, f64)], us: &[usize]) -> Nest {
    assert!(!us.is_empty() && us[0] >= 2 && us.windows(2).all(|u| u[0] < u[1]));
    assert_eq!(xy.len(), g.n);
    let l = us.len();
    let (edges, edge) = undirected(g);
    let mut owner = vec![vec![NONE; edges.len()]; l];
    let (mut cells, mut up) = (vec![0u32; l], vec![Vec::new(); l]);
    let mut seen = vec![false; edges.len()];
    let mut stack = vec![((0..edges.len() as u32).collect::<Vec<u32>>(), vec![NONE; l])];
    while let Some((piece, mut id)) = stack.pop() {
        let nodes = endpoints(&edges, &piece);
        for k in (0..l).rev() {
            if id[k] == NONE && nodes.len() <= us[k] {
                id[k] = cells[k];
                cells[k] += 1;
                up[k].push(if k + 1 < l { id[k + 1] } else { 0 });
            }
        }
        if id[0] != NONE {
            for &e in &piece {
                (0..l).for_each(|k| owner[k][e as usize] = id[k]);
            }
            continue;
        }
        let (a, b) = bisect(&edges, &piece, &nodes, xy, Model::Hypergraph);
        // Edges a fallback edge cut drops stay in the cells already assigned above.
        a.iter().chain(&b).for_each(|&e| seen[e as usize] = true);
        for &e in &piece {
            if !std::mem::replace(&mut seen[e as usize], false) {
                (0..l).for_each(|k| owner[k][e as usize] = id[k]);
            }
        }
        stack.extend([(a, id.clone()), (b, id)]);
    }
    let lv = (0..l)
        .map(|k| assemble(g.n, &edges, &edge, &owner[k], cells[k]))
        .collect();
    Nest { lv, up }
}

impl Nest {
    /// Levels.
    pub fn levels(&self) -> usize {
        self.lv.len()
    }

    /// Level-l boundary nodes.
    pub fn boundary(&self, l: usize) -> usize {
        self.lv[l].home.iter().filter(|&&c| c == NONE).count()
    }
}

impl Multi {
    /// Overlay arcs at each level.
    pub fn arcs(&self) -> Vec<usize> {
        self.lv.iter().map(|v| v.g.head.len()).collect()
    }
}

/// Customize nested partition `p` to the weights of g, bottom up.
pub fn customize(g: &Graph, p: &Nest) -> Multi {
    let (n, l) = (g.n, p.levels());
    assert_eq!(p.lv[0].cell.len(), g.head.len());
    let mut search = Search::new(n);
    let mut lv: Vec<Level> = Vec::with_capacity(l);
    for k in 0..l {
        let part = &p.lv[k];
        let mut arcs = Vec::new();
        for (c, b) in part.members.iter().enumerate() {
            for &x in b {
                let c = c as u32;
                if k == 0 {
                    search.run(g, x as usize, None, |a| part.cell[a] == c);
                } else {
                    let below = &lv[k - 1];
                    search.run(&below.g, x as usize, None, |a| below.group[a] == c);
                }
                arcs.extend(
                    b.iter()
                        .filter(|&&y| y != x && search.d[y as usize].is_finite())
                        .map(|&y| (x, y, search.d[y as usize], c, p.up[k][c as usize])),
                );
            }
        }
        // Base arcs cut at level k and no higher.
        for x in 0..n {
            for a in g.off[x]..g.off[x + 1] {
                let above = if k + 1 < l { p.lv[k + 1].cell[a] } else { 0 };
                if part.cell[a] == NONE && above != NONE {
                    arcs.push((x as u32, g.head[a], g.w[a], CUT, above));
                }
            }
        }
        arcs.sort_by_key(|a| (a.0, a.1));
        lv.push(Level {
            own: arcs.iter().map(|a| a.3).collect(),
            group: arcs.iter().map(|a| a.4).collect(),
            g: Graph::new(n, arcs.into_iter().map(|a| (a.0, a.1, a.2))),
        });
    }
    Multi { lv }
}

/// Reusable Dijkstra state for multilevel queries; only touched entries are reset.
pub struct Query {
    d: Vec<f64>,
    done: Vec<bool>,
    touched: Vec<u32>,
    heap: BinaryHeap<Reverse<(Key, u32)>>,
}

impl Query {
    pub fn new(n: usize) -> Query {
        Query {
            d: vec![f64::INFINITY; n],
            done: vec![false; n],
            touched: Vec::new(),
            heap: BinaryHeap::new(),
        }
    }

    /// d(s, t) on base graph g with nested partition p customized as m; infinity if unreachable.
    pub fn run(&mut self, g: &Graph, p: &Nest, m: &Multi, s: usize, t: usize) -> f64 {
        for &v in &self.touched {
            (self.d[v as usize], self.done[v as usize]) = (f64::INFINITY, false);
        }
        self.touched.clear();
        self.heap.clear();
        let l = p.levels();
        let near = |k: usize, c: u32| c == p.lv[k].home[s] || c == p.lv[k].home[t];
        self.d[s] = 0.0;
        self.touched.push(s as u32);
        self.heap.push(Reverse((Key(0.0), s as u32)));
        while let Some(Reverse((Key(dx), x))) = self.heap.pop() {
            let x = x as usize;
            if std::mem::replace(&mut self.done[x], true) {
                continue;
            }
            if x == t {
                return dx;
            }
            let base = (g.off[x]..g.off[x + 1])
                .filter(|&a| p.lv[0].cell[a] != NONE && near(0, p.lv[0].cell[a]))
                .map(|a| (g.head[a], g.w[a]));
            let over = (0..l)
                .take_while(|&k| p.lv[k].home[x] == NONE)
                .flat_map(|k| {
                    let v = &m.lv[k];
                    (v.g.off[x]..v.g.off[x + 1])
                        .filter(move |&a| {
                            !near(k, v.own[a]) && (k + 1 == l || near(k + 1, v.group[a]))
                        })
                        .map(move |a| (v.g.head[a], v.g.w[a]))
                });
            for (y, w) in base.chain(over) {
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
        f64::INFINITY
    }
}
