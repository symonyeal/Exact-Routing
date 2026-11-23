//! Exact shortest paths with optimality certificates.
//!
//! `grid`: four-neighbour grid mazes, solved by word-parallel breadth-first search.
//! `graph`: sparse directed graphs: breadth-first search, Dijkstra and tree certificates.
//! `overlay`: exact point-to-point queries over cells from a graph or hypergraph partition.

pub mod graph;
pub mod grid;
pub mod overlay;
