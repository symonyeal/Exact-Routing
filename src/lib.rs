//! Exact shortest paths with optimality certificates.
//!
//! `grid`: four-neighbour grid mazes, solved by word-parallel breadth-first search.
//! `graph`: sparse directed graphs: breadth-first search, Dijkstra and tree certificates.
//! `overlay`: exact point-to-point queries over cells from a graph or hypergraph partition.
//! `levels`: the same over nested junction-cut cells on several levels.
//! `cch`: customizable contraction hierarchies: one weight-free preparation, one customization per metric.

pub mod cch;
pub mod ch;
pub mod graph;
pub mod grid;
pub mod levels;
pub mod overlay;
pub mod query;
pub mod road;
