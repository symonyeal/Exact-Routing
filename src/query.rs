//! Exact directed point-to-point distances by bidirectional Dijkstra.
//!
//! Given forward graph `g` and its reverse `r` on the same nodes, return `d(s,t)` for finite,
//! nonnegative arc weights. Search state and heap storage are reused; only touched distances reset.
//! The search stops when the two valid queue minima sum to at least the best joined path.

use crate::graph::{Graph, Key};
use std::cmp::Reverse;
use std::collections::BinaryHeap;

/// Reusable workspace for bidirectional point-to-point queries.
pub struct BiSearch {
    df: Vec<f64>,
    dr: Vec<f64>,
    tf: Vec<u32>,
    tr: Vec<u32>,
    hf: BinaryHeap<Reverse<(Key, u32)>>,
    hr: BinaryHeap<Reverse<(Key, u32)>>,
}

impl BiSearch {
    /// Workspace for graphs with `n` nodes.
    pub fn new(n: usize) -> BiSearch {
        BiSearch {
            df: vec![f64::INFINITY; n],
            dr: vec![f64::INFINITY; n],
            tf: Vec::new(),
            tr: Vec::new(),
            hf: BinaryHeap::new(),
            hr: BinaryHeap::new(),
        }
    }

    /// Return the exact distance from `s` to `t`; unreachable pairs have infinite distance.
    /// `g` and `r` must be the forward and reverse CSR graphs with the same node count.
    pub fn query(&mut self, g: &Graph, r: &Graph, s: usize, t: usize) -> f64 {
        for &v in &self.tf {
            self.df[v as usize] = f64::INFINITY;
        }
        for &v in &self.tr {
            self.dr[v as usize] = f64::INFINITY;
        }
        self.tf.clear();
        self.tr.clear();
        self.hf.clear();
        self.hr.clear();
        if s == t {
            return 0.0;
        }

        self.df[s] = 0.0;
        self.dr[t] = 0.0;
        self.tf.push(s as u32);
        self.tr.push(t as u32);
        self.hf.push(Reverse((Key(0.0), s as u32)));
        self.hr.push(Reverse((Key(0.0), t as u32)));
        let mut best = f64::INFINITY;

        loop {
            let (mf, mr) = (
                minimum(&mut self.hf, &self.df),
                minimum(&mut self.hr, &self.dr),
            );
            if mf + mr >= best {
                break;
            }
            let forward = mf <= mr;
            let (heap, d, other, graph) = if forward {
                (&mut self.hf, &mut self.df, &self.dr, g)
            } else {
                (&mut self.hr, &mut self.dr, &self.df, r)
            };
            let Reverse((Key(dx), x)) = heap.pop().unwrap();
            let x = x as usize;
            if other[x].is_finite() {
                best = best.min(dx + other[x]);
            }
            let start = graph.off[x];
            let end = graph.off[x + 1];
            for a in start..end {
                let y = graph.head[a] as usize;
                let dy = dx + graph.w[a];
                if dy < d[y] {
                    if d[y] == f64::INFINITY {
                        if forward {
                            self.tf.push(y as u32)
                        } else {
                            self.tr.push(y as u32)
                        }
                    }
                    d[y] = dy;
                    heap.push(Reverse((Key(dy), y as u32)));
                    if other[y].is_finite() {
                        best = best.min(dy + other[y]);
                    }
                }
            }
        }
        best
    }
}

/// Remove lazy stale entries and return the minimum current label, or infinity if empty.
fn minimum(heap: &mut BinaryHeap<Reverse<(Key, u32)>>, d: &[f64]) -> f64 {
    while let Some(Reverse((Key(dx), x))) = heap.peek() {
        if *dx == d[*x as usize] {
            return *dx;
        }
        heap.pop();
    }
    f64::INFINITY
}
