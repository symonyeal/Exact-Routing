//! Exact shortest paths on four-neighbour grid mazes, each with an optimality certificate.
//!
//! Problem. A grid of H rows and W columns holds walls `#`, open cells ` `, one start `S` and one
//! goal `G`. A move goes to an orthogonally adjacent non-wall cell at unit cost. Return a shortest
//! S–G path, or prove that none exists. Among shortest paths the one returned is the
//! lexicographically smallest move sequence under D < L < R < U; on the Sapient Maze-Hard data this
//! is the reference answer (docs/grid.md).
//!
//! Method. Breadth-first layers from G (Lee 1961), one word-parallel step per layer. Each reached
//! cell keeps its distance modulo 3 in two bits, which identifies its predecessors (Akers 1967); a
//! walk from S then takes the first move in D, L, R, U order that lowers the distance.
//!
//! Legend
//!   H, W   rows, columns
//!   ρ      row stride in bits: 32 when W < 32 (two rows per word), else 64·(W/64 + 1); bit ρ − 1
//!          of every row is a non-cell
//!   M      margin words, ⌊ρ/64⌋ + 1, the reach of one move in words
//!   b      bit index of cell (r, c) in a plane: 64M + rρ + c; M zero words pad each end, and B
//!          spare words follow so that step windows round up to whole blocks
//!   B      words per block, the width of the step's vector operations
//!   v      cell index r·W + c, used by paths and potentials
//!   F      plane of open cells (S and G included)
//!   s, t   bit indices of S and G
//!   L_k    layer {b : d(b, t) = k}
//!   λ      label 1 + d(b, t) mod 3 of a reached cell, 0 for an unreached one; two planes
//!   D      d(s, t), the length of a shortest path in moves

use std::mem::swap;

const B: usize = 4;

/// A parsed grid: the open-cell plane F and the endpoints s, t.
pub struct Maze {
    pub h: usize,
    pub w: usize,
    rho: usize,
    f: Vec<u64>,
    s: usize,
    t: usize,
}

impl Maze {
    /// Parse a row-major H×W grid over `# SG` holding exactly one S and one G.
    ///
    /// Eight cells per word: e(x, y) marks the bytes of x equal to y in their high bits (the exact
    /// zero-byte test) and a multiply gathers those bits into a byte, giving the open-cell bits o
    /// in cell order; each row of o is then shifted into its plane row.
    pub fn parse(q: &[u8], h: usize, w: usize) -> Result<Maze, String> {
        if q.len() != h * w || q.is_empty() {
            return Err(format!("{} cells for a {h}×{w} grid", q.len()));
        }
        const ONE: u64 = 0x0101_0101_0101_0101;
        const HI: u64 = ONE * 0x80;
        let e = |x: u64, y: u8| {
            let v = x ^ (ONE * y as u64);
            !(((v & !HI) + ONE * 0x7f) | v) & HI
        };
        let gather = |x: u64| (x >> 7).wrapping_mul(0x0102_0408_1020_4080) >> 56;
        let chunks = q.chunks_exact(8);
        let mut tail = [b'#'; 8];
        tail[..chunks.remainder().len()].copy_from_slice(chunks.remainder());
        let words = chunks.map(|c| u64::from_le_bytes(c.try_into().unwrap()));
        let mut o = vec![0u64; q.len().div_ceil(64) + 1];
        let (mut s, mut t, mut bad) = ([0, 0], [0, 0], 0);
        for (j, x) in words.chain([u64::from_le_bytes(tail)]).enumerate() {
            let (wall, xs, xt) = (e(x, b'#'), e(x, b'S'), e(x, b'G'));
            bad |= !(wall | xs | xt | e(x, b' ')) & HI;
            o[j / 8] |= gather(!wall & HI) << (8 * (j % 8));
            for (end, y) in [(&mut s, xs), (&mut t, xt)] {
                if y != 0 {
                    *end = [
                        end[0] + y.count_ones() as usize,
                        8 * j + y.trailing_zeros() as usize / 8,
                    ];
                }
            }
        }
        if bad != 0 {
            let x = q.iter().find(|x| !b"# SG".contains(x)).unwrap();
            return Err(format!("invalid cell {:?}", *x as char));
        }
        let rho = if w < 32 { 32 } else { 64 * (w / 64 + 1) };
        let mm = rho / 64 + 1;
        let mut f = vec![0; 2 * mm + (h * rho).div_ceil(64) + B];
        for r in 0..h {
            for j in 0..w.div_ceil(64) {
                let (p, n) = (r * w + 64 * j, (w - 64 * j).min(64));
                let (i, sh) = (p / 64, p % 64);
                let x = (o[i] >> sh | (o[i + 1] << 1) << (63 - sh)) & (!0 >> (64 - n));
                let b = 64 * mm + r * rho + 64 * j;
                f[b / 64] |= x << (b % 64);
            }
        }
        match (s, t) {
            ([1, s], [1, t]) => {
                let mut m = Maze { h, w, rho, f, s, t };
                (m.s, m.t) = (m.bit(s), m.bit(t));
                Ok(m)
            }
            _ => Err(format!(
                "{} S and {} G cells, expected one each",
                s[0], t[0]
            )),
        }
    }

    fn margin(&self) -> usize {
        self.rho / 64 + 1
    }

    fn bit(&self, v: usize) -> usize {
        64 * self.margin() + v / self.w * self.rho + v % self.w
    }

    fn cell(&self, b: usize) -> usize {
        let b = b - 64 * self.margin();
        b / self.rho * self.w + b % self.rho
    }

    fn open(&self, v: usize) -> bool {
        get(&self.f, self.bit(v))
    }
}

fn get(x: &[u64], b: usize) -> bool {
    x[b / 64] >> (b % 64) & 1 == 1
}

/// Breadth-first layers from t, one word-parallel step per layer (Lee 1961):
///
///   L_0 = {t},  L_{k+1} = N(L_k) ∩ F ∩ {λ = 0},  N(X) = X ∪ X≪1 ∪ X≫1 ∪ X↑ ∪ X↓,
///
/// labelling L_{k+1} with λ = 1 + (k + 1) mod 3. Calls `visit(k, i, x)` with x = words i, i + 1, …
/// of L_k (k ≥ 1), covering L_k, and stops when s ∈ L_k with D = k or when L_k = ∅ with None.
/// Returns D and a buffer whose first two planes are λ's bits. The step window, the words within
/// one row of L_0 ∪ … ∪ L_{k−1}, only grows, so it covers every stale word of the buffer being
/// overwritten. A step costs O(words of the window) branch-free word operations.
#[inline(always)]
fn wave(m: &Maze, mut visit: impl FnMut(usize, usize, &[u64])) -> (Option<usize>, Vec<u64>) {
    let (n, mm) = (m.f.len(), m.margin());
    let mut buf = vec![0; 4 * n];
    let (lam, xy) = buf.split_at_mut(2 * n);
    let (l0, l1) = lam.split_at_mut(n);
    let (mut x, mut y) = xy.split_at_mut(n); // L_k, then L_{k+1}
    let (mut i0, mut i1) = (m.t / 64, m.t / 64 + 1);
    x[i0] = 1 << (m.t % 64);
    l0[i0] = x[i0];
    let step = kernel();
    let mut d = None;
    for k in 1.. {
        i0 = i0.saturating_sub(mm).max(mm);
        i1 = i0 + ((i1 + mm).min(n - B - mm) - i0).next_multiple_of(B);
        let lam = [&mut l0[i0..i1], &mut l1[i0..i1]];
        // SAFETY: `kernel` returns a version the processor supports; the window is whole blocks
        // and lies at least M words inside the planes.
        let any = unsafe {
            step(
                &mut y[i0..i1],
                lam,
                &m.f[i0..i1],
                &x[i0 - mm..i1 + mm],
                m.rho,
                k,
            )
        };
        if any == 0 {
            break;
        }
        swap(&mut x, &mut y);
        visit(k, i0, &x[i0..i1]);
        if get(x, m.s) {
            d = Some(k);
            break;
        }
    }
    (d, buf)
}

/// One step on a window of whole blocks: e = N(x) ∩ F ∩ {λ = 0}, then λ = 1 + k mod 3 on e, where
/// x carries M words of margin on each side and the vertical moves shift by ρ bits. Returns the union of the words of e, zero when e = ∅.
type Step = unsafe fn(&mut [u64], [&mut [u64]; 2], &[u64], &[u64], usize, usize) -> u64;

/// The AVX2 step when the processor has it, else the portable one.
fn kernel() -> Step {
    #[cfg(target_arch = "x86_64")]
    if is_x86_feature_detected!("avx2") {
        return step_avx2;
    }
    step
}

/// λ's bit masks for layer k: all ones where bit j of 1 + k mod 3 is set.
fn masks(k: usize) -> [u64; 2] {
    let j = (k % 3 + 1) as u64;
    [(j & 1).wrapping_neg(), (j >> 1).wrapping_neg()]
}

fn step(
    e: &mut [u64],
    [l0, l1]: [&mut [u64]; 2],
    f: &[u64],
    x: &[u64],
    rho: usize,
    k: usize,
) -> u64 {
    let [m0, m1] = masks(k);
    let (q, sh) = (rho / 64, (rho % 64) as u32);
    let mut any = 0;
    for i in 0..e.len() {
        let j = i + q + 1;
        let (z, l, r) = (x[j], x[j - 1], x[j + 1]);
        let up = x[j - q] << sh | (x[j - q - 1] >> (63 - sh)) >> 1;
        let down = x[j + q] >> sh | (x[j + q + 1] << (63 - sh)) << 1;
        e[i] = (z | z << 1 | z >> 1 | l >> 63 | r << 63 | up | down) & f[i] & !(l0[i] | l1[i]);
        l0[i] |= e[i] & m0;
        l1[i] |= e[i] & m1;
        any |= e[i];
    }
    any
}

/// `step` on B = 4 words per instruction.
///
/// # Safety
/// The processor supports AVX2; all slices hold at least e.len() words, a multiple of B, and x
/// holds 2M more.
#[cfg(target_arch = "x86_64")]
#[target_feature(enable = "avx2")]
unsafe fn step_avx2(
    e: &mut [u64],
    [l0, l1]: [&mut [u64]; 2],
    f: &[u64],
    x: &[u64],
    rho: usize,
    k: usize,
) -> u64 {
    use std::arch::x86_64::*;
    let ld = |s: &[u64], i: usize| unsafe { _mm256_loadu_si256(s.as_ptr().add(i).cast()) };
    let st = |s: &mut [u64], i: usize, v| unsafe {
        _mm256_storeu_si256(s.as_mut_ptr().add(i).cast(), v)
    };
    let (q, sh) = (rho / 64, (rho % 64) as i64);
    let (sl, sr) = (_mm_cvtsi64_si128(sh), _mm_cvtsi64_si128(63 - sh));
    let [m0, m1] = masks(k).map(|m| _mm256_set1_epi64x(m as i64));
    let or = |a, b| _mm256_or_si256(a, b);
    let mut any = _mm256_setzero_si256();
    for i in (0..e.len()).step_by(B) {
        let j = i + q + 1;
        let z = ld(x, j);
        let h = or(or(z, _mm256_slli_epi64(z, 1)), _mm256_srli_epi64(z, 1));
        let lr = or(
            _mm256_srli_epi64(ld(x, j - 1), 63),
            _mm256_slli_epi64(ld(x, j + 1), 63),
        );
        let up = or(
            _mm256_sll_epi64(ld(x, j - q), sl),
            _mm256_srli_epi64(_mm256_srl_epi64(ld(x, j - q - 1), sr), 1),
        );
        let down = or(
            _mm256_srl_epi64(ld(x, j + q), sl),
            _mm256_slli_epi64(_mm256_sll_epi64(ld(x, j + q + 1), sr), 1),
        );
        let (a0, a1) = (ld(l0, i), ld(l1, i));
        let n = _mm256_and_si256(or(or(h, lr), or(up, down)), ld(f, i));
        let n = _mm256_andnot_si256(or(a0, a1), n);
        st(e, i, n);
        st(l0, i, or(a0, _mm256_and_si256(n, m0)));
        st(l1, i, or(a1, _mm256_and_si256(n, m1)));
        any = or(any, n);
    }
    (_mm256_testz_si256(any, any) == 0) as u64
}

/// The lexicographically smallest shortest s–t path under D < L < R < U, as cells from S to G;
/// None if G is unreachable from S.
///
/// The reached neighbours of a cell at distance k lie at k − 1, k or k + 1, so λ names the layer
/// below. Taking the first lowering move in D, L, R, U order from s is lexicographically minimal
/// because every lowering move extends to a shortest path (docs/grid.md, Proposition 1).
pub fn solve(m: &Maze) -> Option<Vec<usize>> {
    let (d, buf) = wave(m, |_, _, _| {});
    let (d, n) = (d?, m.f.len());
    let lambda = |b: usize| get(&buf[..n], b) as usize | (get(&buf[n..2 * n], b) as usize) << 1;
    let (row, w) = (m.rho, m.w);
    let (mut b, mut v) = (m.s, m.cell(m.s));
    let mut path = Vec::with_capacity(d + 1);
    path.push(v);
    for k in (0..d).rev() {
        // D, L, R, U as (bit step, cell step); unreached non-cells have λ = 0.
        let (db, dv) = [
            (row, w),
            (usize::MAX, usize::MAX),
            (1, 1),
            (row.wrapping_neg(), w.wrapping_neg()),
        ]
        .into_iter()
        .find(|&(db, _)| lambda(b.wrapping_add(db)) == k % 3 + 1)
        .expect("a cell of L_{k+1} borders L_k");
        (b, v) = (b.wrapping_add(db), v.wrapping_add(dv));
        path.push(v);
    }
    Some(path)
}

/// The certificate potential, indexed by cell (∞ = u32::MAX): p_v = min(d(v, t), D) when G is
/// reachable, and p_v = d(v, t) otherwise, so p_s = ∞ exactly when no path exists.
pub fn potential(m: &Maze) -> Vec<u32> {
    let mut p = vec![u32::MAX; m.h * m.w];
    p[m.cell(m.t)] = 0;
    let (d, _) = wave(m, |k, i, x| {
        for (i, &e) in (i..).zip(x) {
            let mut e = e;
            while e != 0 {
                p[m.cell(64 * i + e.trailing_zeros() as usize)] = k as u32;
                e &= e - 1;
            }
        }
    });
    if let Some(d) = d {
        p.iter_mut().for_each(|x| *x = (*x).min(d as u32));
    }
    p
}

/// Verify an outcome against its potential (LP duality for the unit-flow formulation).
///
/// Requires p_t = 0 and |p_u − p_v| ≤ 1 on every open edge, with ∞ only beside ∞. Then every s–t
/// path has length at least p_s − p_t, so a legal path of n moves with p_s = n is shortest; and
/// p_s = ∞ proves that no path exists, since t's finite region has no open edge leaving it.
pub fn check(m: &Maze, path: Option<&[usize]>, p: &[u32]) -> bool {
    let (w, n) = (m.w, m.h * m.w);
    if p.len() != n {
        return false;
    }
    let (s, t) = (m.cell(m.s), m.cell(m.t));
    // An open edge {u, v} with u < v; false for any cell outside the grid.
    let edge = |u: usize, v: usize| {
        v < n && m.open(u) && m.open(v) && (v == u + w || (v == u + 1 && !v.is_multiple_of(w)))
    };
    let lipschitz = (0..n).filter(|&v| m.open(v)).all(|v| {
        [v + 1, v + w]
            .into_iter()
            .filter(|&u| edge(v, u))
            .all(|u| p[u].abs_diff(p[v]) <= 1)
    });
    let outcome = match path {
        Some(x) => {
            x.first() == Some(&s)
                && x.last() == Some(&t)
                && x.windows(2).all(|e| edge(e[0].min(e[1]), e[0].max(e[1])))
                && p[s] as usize == x.len() - 1
        }
        None => p[s] == u32::MAX,
    };
    p[t] == 0 && lipschitz && outcome
}

/// The answer grid: the question with the interior cells of the path marked `o`.
pub fn render(q: &[u8], path: &[usize]) -> Vec<u8> {
    let mut a = q.to_vec();
    if let [_, inner @ .., _] = path {
        inner.iter().for_each(|&v| a[v] = b'o');
    }
    a
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The AVX2 and portable steps agree word for word on random windows and both row strides.
    #[test]
    #[cfg(target_arch = "x86_64")]
    fn kernels_agree() {
        if !is_x86_feature_detected!("avx2") {
            return;
        }
        let mut z = 1u64;
        let mut r = || {
            z ^= z << 13;
            z ^= z >> 7;
            z ^= z << 17;
            z
        };
        for (rho, len) in [(32, 16), (128, 24), (64, 8)] {
            let margin = rho / 64 + 1;
            for k in 1..20 {
                let x: Vec<u64> = (0..len + 2 * margin).map(|_| r() & r()).collect();
                let f: Vec<u64> = (0..len).map(|_| r() | r()).collect();
                let lam: Vec<u64> = (0..2 * len).map(|_| r() & r() & r()).collect();
                let run = |step: Step| {
                    let (mut e, mut l) = (vec![0; len], lam.clone());
                    let (l0, l1) = l.split_at_mut(len);
                    // SAFETY: AVX2 is present; the slices have the documented lengths.
                    let any = unsafe { step(&mut e, [l0, l1], &f, &x, rho, k) };
                    (any != 0, e, l)
                };
                assert_eq!(run(step), run(step_avx2), "rho {rho}, k {k}");
            }
        }
    }
}
