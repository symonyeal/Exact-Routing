# Method

## Problem

A maze is an H×W grid of walls `#`, open cells, one start `S` and one goal `G`. The graph has the
non-wall cells as vertices and orthogonal adjacencies as edges, each of unit cost. Write
d(v) = d(v, t) for the distance from cell v to the goal t, and D = d(s) for the shortest path length in
moves. The Sapient Maze-Hard answer grid marks one shortest path with `o`. Every one of the 2,000
mazes has several shortest paths, so a correct path matches the reference only if it breaks ties the
same way.

## The reference tie rule

Name the moves D (down), L (left), R (right), U (up), ordered D < L < R < U, and read a path from S as
a word over this alphabet. Words of equal length compare lexicographically.

**Lemma.** A walk of D moves from s is a shortest s–t path exactly when every move lowers d by one.
*Proof.* Across an edge d changes by at most one. d(s) = D and d(t) = 0, so D moves reach t only if
each lowers d by one; conversely, such a walk reaches d = 0, which is t. ∎

**Proposition 1.** The walk from s that repeatedly takes the least move lowering d gives the
lexicographically least word among shortest s–t paths.
*Proof.* Induction on D. A lowering move exists whenever d ≥ 1 (the second cell of any shortest path).
By the lemma every shortest path begins with a lowering move, so any path whose first move differs
from the least lowering move has a larger first letter. The paths that share it continue with a
shortest path from the new cell, where the induction hypothesis applies. ∎

**Proposition 2.** Breadth-first search from s that scans neighbours in the order D, L, R, U and keeps
each cell's first discoverer as its parent yields the same path, traced back from t.
*Proof.* Induction on the layer k = d(s, ·). Cells of layer k + 1 are enqueued in order of (rank of
the parent, move), which is the lexicographic order of their tree words because parent ranks follow
their words by hypothesis. The least shortest word to a cell w is the least predecessor word followed by
one move; distinct predecessors have distinct words, since a word fixes its endpoint, so the least is
the earliest dequeued predecessor, the first discoverer. ∎

**Evidence.** On the train split only, 336 candidate rules were scored: parent pointers and distance
descent from either endpoint, A* with five heap keys, each with all 24 neighbour orders. Exactly the two
rules of Propositions 1 and 2 under D < L < R < U reproduced all 1,000 answer grids; the next best rule
reproduced 956. The rule was fixed before the test split was downloaded and then scored once: 1,000 of
1,000 test answers match.

## Word-parallel breadth-first search

Cells are bits. Each grid row occupies ρ bits of a plane, ρ = 32 when W < 32 (two rows per 64-bit word)
and ρ = 64(⌊W/64⌋ + 1) otherwise, so the last bit of every row is never a cell and margin words pad both
ends. With F the plane of open cells, the layers from t follow Lee's wave expansion (Lee 1961):

    L_0 = {t},   L_{k+1} = N(L_k) ∩ F ∖ (L_0 ∪ … ∪ L_k),   N(X) = X ∪ X≪1 ∪ X≫1 ∪ X≪ρ ∪ X≫ρ.

A horizontal shift that leaves its row lands on a non-cell bit and is removed by F, so no masks are
needed. One step is a fixed sequence of shifts, ORs and ANDs per word, executed four words at a time
with AVX2 when the processor has it. The step runs over a window of words that grows by one move's
reach per layer, rounded to whole four-word blocks, and stops when s enters a layer (D found) or a
layer is empty (G unreachable).

Each reached cell stores λ = 1 + d mod 3 in two bit planes, λ = 0 meaning unreached. The reached
neighbours of a cell at distance k lie at k − 1, k or k + 1, so λ identifies the layer below; Akers
(1967) observed that such periodic labels suffice for Lee's trace-back. The walk of Proposition 1 then
reads λ at the four neighbours in D, L, R, U order and takes the first with λ = 1 + (k − 1) mod 3.

Cost: the wave takes D steps over at most ⌈Hρ/64⌉ + 4 words each, O(D·H·⌈ρ/64⌉) word operations, which
is O(D·HW/64) for W ≥ 32 but up to 32 times more for very narrow grids; the walk costs O(D).
Breadth-first search with a queue costs O(HW). On the Maze-Hard grids (30×30, D ≈ 114) a
step covers at most 16 words and the wave wins by constant factors: branch-free code, four words per
instruction and five 168-byte planes in place of per-cell arrays. On larger percolation-like grids D
grows with the side; in one run the queue search was faster at n = 1024, measured on 5 mazes
(`results/bench.md`).

## Certificate

The unit-flow linear program min Σ x_a, subject to unit flow conservation from s to t and x ≥ 0, has the
dual max p_s − p_t subject to p_u − p_v ≤ 1 on every arc. `potential` returns p_v = min(d(v), D), or
d(v) with ∞ off G's component when no path exists; `check`
verifies, independently of the search (it shares only the parser, which the tests check against the
raw grid):

- p_t = 0 and |p_u − p_v| ≤ 1 across every open edge, ∞ only beside ∞;
- for a returned path: it starts at S, ends at G, moves between adjacent open cells, and has p_s moves.
  Any s–t path P satisfies |P| ≥ Σ_{(u,v)∈P} (p_u − p_v) = p_s − p_t, so the path is shortest;
- for an unreachable goal: p_s = ∞. The finite region contains t but not s and no open edge leaves it.

Checking costs O(HW). Tests mutate paths and potentials and confirm that `check` rejects them.

## Relation to recent shortest-path results

The deterministic single-source bounds below Dijkstra's (Duan et al., STOC 2025 and ICALP 2026) and the
undirected bound of Kadria and Roditty (2026) concern real weights in the comparison-addition model. Unit weights already admit O(n + m) breadth-first search, and reading an
arbitrary grid takes Ω(HW), so these results leave no asymptotic room on this task; the remaining gains
are constant factors in time and memory. Exact reductions such as dead-end pruning and corridor
contraction were not implemented: the wave's cost depends on D and the window, which pruning does not
change, and for the queue search pruning is itself a full pass. They are not measured.

## Sources

- C. Y. Lee, "An Algorithm for Path Connections and Its Applications", *IRE Trans. Electronic
  Computers* EC-10(3), 1961.
- S. B. Akers, "A Modification of Lee's Path Connection Routing Algorithm", *IEEE Trans. Electronic
  Computers* EC-16(1), 1967.
- R. Duan et al., "Breaking the Sorting Barrier for Directed Single-Source Shortest Paths",
  [arXiv 2504.17033](https://arxiv.org/abs/2504.17033), STOC 2025; the deterministic follow-up in
  [ICALP 2026](https://drops.dagstuhl.de/entities/document/10.4230/LIPIcs.ICALP.2026.81).
- A. Kadria, L. Roditty, weighted undirected shortest paths,
  [arXiv 2609.15247](https://arxiv.org/abs/2609.15247), 2026.
- G. Steele, D. Lea, C. Flood, "Fast Splittable Pseudorandom Number Generators", OOPSLA 2014
  (splitmix64, used by the tests and benchmark generator).
- S. E. Anderson, *Bit Twiddling Hacks*, "Determine if a word has a byte equal to n" (the parser's
  byte test).
- Dataset: [sapientinc/maze-30x30-hard-1k](https://huggingface.co/datasets/sapientinc/maze-30x30-hard-1k),
  revision `549754de3d67eebe57b6b22c7d226b9412d55b07`; builder in
  [sapientinc/HRM](https://github.com/sapientinc/HRM/blob/main/dataset/build_maze_dataset.py).
