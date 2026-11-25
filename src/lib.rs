//! Exact point-to-point road routing with certified answers.
//!
//! `graph`: sparse directed graphs, Dijkstra and shortest-path-tree certificates.
//! `query`: bidirectional Dijkstra.
//! `ch`: the `fast_paths` contraction hierarchy behind a validated adapter.
//! `cch`: customizable contraction hierarchies: one weight-free preparation, one customization per
//! metric, serial, threaded, partial or perfect.
//! `levels`: multilevel overlays over nested junction-cut cells.
//! `road`: the road binary and its travel-time metrics.

pub mod cch;
pub mod ch;
pub mod graph;
pub mod levels;
mod partition;
pub mod query;
pub mod road;
