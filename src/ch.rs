//! Point-to-point distances with `fast_paths` contraction hierarchies.
//!
//! `Ch::new` requires finite positive integer arc weights and a conservative bound
//! `max(w) * n < min(usize::MAX / 4, 2^52)`. This keeps every simple-path cost exact
//! in both the crate's integer arithmetic and the returned `f64`. `fast_paths`
//! removes self-loops and keeps the lightest of parallel arcs while preparing.

use crate::graph::Graph;

/// Prepared contraction hierarchy and its number of represented nodes.
pub struct Ch {
    graph: fast_paths::FastGraph,
    n: usize,
    prepared_nodes: usize,
}

impl Ch {
    /// Prepare a hierarchy from `g`, asserting the documented weight and graph bounds.
    pub fn new(g: &Graph) -> Ch {
        assert!(
            g.n as u128 <= u32::MAX as u128 + 1,
            "node IDs exceed Graph's u32 format"
        );
        let mut input = fast_paths::InputGraph::new();
        let mut maxw = 0usize;
        for u in 0..g.n {
            for (v, w) in g.arcs(u) {
                assert!(
                    w.is_finite() && w >= 1.0 && w.fract() == 0.0,
                    "CH weights must be positive integers"
                );
                assert!(
                    w < (1u64 << 52) as f64,
                    "CH weight exceeds exact f64 integer range"
                );
                assert!((w as u128) <= usize::MAX as u128, "CH weight exceeds usize");
                let weight = w as usize;
                maxw = maxw.max(weight);
                input.add_edge(u, v as usize, weight);
            }
        }
        let bound = maxw
            .checked_mul(g.n)
            .expect("max weight times node count overflows usize");
        assert!(
            (bound as u128) < (usize::MAX as u128 / 4).min(1u128 << 52),
            "simple-path weight bound exceeds exact range"
        );
        input.freeze();
        let nodes = input.get_num_nodes();
        let graph = if input.get_num_edges() == 0 {
            fast_paths::FastGraph::new(0)
        } else {
            fast_paths::prepare(&input)
        };
        Ch {
            graph,
            n: g.n,
            prepared_nodes: nodes,
        }
    }

    /// Create reusable query state. Use one calculator per concurrent query worker.
    pub fn calculator(&self) -> fast_paths::PathCalculator {
        fast_paths::create_calculator(&self.graph)
    }

    /// Return the exact distance; return infinity when no path exists.
    pub fn query(&self, calc: &mut fast_paths::PathCalculator, s: usize, t: usize) -> f64 {
        assert!(s < self.n && t < self.n, "query node is outside the graph");
        if s == t {
            return 0.0;
        }
        if s >= self.prepared_nodes || t >= self.prepared_nodes {
            return f64::INFINITY;
        }
        calc.calc_path(&self.graph, s, t)
            .map_or(f64::INFINITY, |path| path.get_weight() as f64)
    }

    /// Number of forward and backward arcs in the prepared hierarchy.
    pub fn arcs(&self) -> usize {
        self.graph.get_num_out_edges() + self.graph.get_num_in_edges()
    }
}
