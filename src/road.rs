//! Decode the little-endian road format emitted by the pinned dataset converters.

use crate::graph::Graph;

/// A validated directed network with projected longitude/latitude coordinates for partitioning.
pub struct Road {
    pub g: Graph,
    pub xy: Vec<(f64, f64)>,
}

/// Keep native seconds at scale 0, or replace each cost by max(1, round(scale * seconds)).
/// Rebuilding from the CSR rows keeps the arc order, so graphs scaled alike stay arc-aligned.
pub fn scaled(g: Graph, scale: usize) -> Result<Graph, &'static str> {
    if scale == 0 {
        return Ok(g);
    }
    if g.w.iter().any(|w| !(w * scale as f64).is_finite()) {
        return Err("scaled road cost overflows");
    }
    Ok(Graph::new(
        g.n,
        (0..g.n).flat_map(|u| {
            g.arcs(u)
                .map(move |(v, w)| (u as u32, v, (w * scale as f64).round().max(1.0)))
        }),
    ))
}

/// The graphs of k travel-time columns over the arcs of road binary `road`, from metrics file `b`:
/// k,m:u64; w:f64[k][m] seconds, nonnegative and finite, column j in the binary's arc order. Every
/// graph is built from the same arc sequence, so arc a of one graph's CSR is arc a of each other's.
pub fn metrics(road: &[u8], b: &[u8], scale: usize) -> Result<Vec<Graph>, &'static str> {
    let r = Road::decode(road)?;
    let (n, m) = (r.g.n, word(road, 8) as usize);
    if b.len() < 16 || word(b, 8) as usize != m {
        return Err("metrics header does not match the road binary");
    }
    let k = word(b, 0) as usize;
    if k == 0 || k.checked_mul(8 * m).and_then(|v| v.checked_add(16)) != Some(b.len()) {
        return Err("invalid metrics dimensions or byte count");
    }
    let at = 16 + 16 * n;
    let node = |i: usize| u32::from_le_bytes(road[i..i + 4].try_into().unwrap());
    (0..k)
        .map(|j| {
            let w =
                |a: usize| f64::from_le_bytes(b[16 + 8 * (j * m + a)..][..8].try_into().unwrap());
            if (0..m).any(|a| !w(a).is_finite() || w(a) < 0.0) {
                return Err("invalid metric weight");
            }
            let g = Graph::new(
                n,
                (0..m).map(|a| (node(at + 4 * a), node(at + 4 * m + 4 * a), w(a))),
            );
            scaled(g, scale)
        })
        .collect()
}

/// The frozen query workload: q pairs from seed 20261123, alternating 8-arc random walks, 32-arc
/// random walks and two uniform node pairs.
pub fn pairs(g: &Graph, q: usize) -> Vec<(usize, usize)> {
    let n = g.n;
    let mut z = 20261123;
    (0..q)
        .map(|i| {
            let s = mix(&mut z) as usize % n;
            let mut t = mix(&mut z) as usize % n;
            if i % 4 < 2 {
                t = s;
                for _ in 0..if i % 4 == 0 { 8 } else { 32 } {
                    let arcs: Vec<_> = g.arcs(t).collect();
                    if arcs.is_empty() {
                        break;
                    }
                    t = arcs[mix(&mut z) as usize % arcs.len()].0 as usize;
                }
            }
            (s, t)
        })
        .collect()
}

/// SplitMix64 (Steele, Lea and Flood 2014): advance z and return the next 64 random bits.
pub fn mix(z: &mut u64) -> u64 {
    *z = z.wrapping_add(0x9e3779b97f4a7c15);
    let mut x = *z;
    x = (x ^ x >> 30).wrapping_mul(0xbf58476d1ce4e5b9);
    x = (x ^ x >> 27).wrapping_mul(0x94d049bb133111eb);
    x ^ x >> 31
}

fn word(b: &[u8], i: usize) -> u64 {
    u64::from_le_bytes(b[i..i + 8].try_into().unwrap())
}

impl Road {
    /// Keep native seconds at scale 0, or replace each cost by max(1, round(scale * seconds)).
    pub fn with_scale(mut self, scale: usize) -> Result<Road, &'static str> {
        self.g = scaled(self.g, scale)?;
        Ok(self)
    }

    /// Require n,m:u64; lat,lon:f64[n]; tail,head:u32[m]; nonnegative weight:f64[m].
    /// Coordinates must be finite geographic degrees and every endpoint must belong to the graph.
    pub fn decode(b: &[u8]) -> Result<Road, &'static str> {
        if b.len() < 16 {
            return Err("short road header");
        }
        let n = usize::try_from(word(b, 0)).map_err(|_| "node count overflow")?;
        let m = usize::try_from(word(b, 8)).map_err(|_| "arc count overflow")?;
        let size = n
            .checked_add(m)
            .and_then(|v| v.checked_mul(16))
            .and_then(|v| v.checked_add(16));
        if n == 0 || n > u32::MAX as usize || size != Some(b.len()) {
            return Err("invalid road dimensions or byte count");
        }
        let real = |i: usize| f64::from_le_bytes(b[i..i + 8].try_into().unwrap());
        let lat: Vec<f64> = (0..n).map(|v| real(16 + 8 * v)).collect();
        let lon: Vec<f64> = (0..n).map(|v| real(16 + 8 * n + 8 * v)).collect();
        if !lat.iter().all(|v| v.is_finite() && v.abs() <= 90.0)
            || !lon.iter().all(|v| v.is_finite() && v.abs() <= 180.0)
        {
            return Err("invalid road coordinates");
        }
        let at = 16 + 16 * n;
        let node = |i: usize| u32::from_le_bytes(b[i..i + 4].try_into().unwrap());
        let mut arcs = Vec::with_capacity(m);
        for a in 0..m {
            let (u, v, w) = (
                node(at + 4 * a),
                node(at + 4 * m + 4 * a),
                real(at + 8 * m + 8 * a),
            );
            if u as usize >= n || v as usize >= n || !w.is_finite() || w < 0.0 {
                return Err("invalid road arc");
            }
            arcs.push((u, v, w));
        }
        let scale = (lat.iter().sum::<f64>() / n as f64).to_radians().cos();
        let xy = lon
            .into_iter()
            .zip(lat)
            .map(|(x, y)| (x * scale, y))
            .collect();
        Ok(Road {
            g: Graph::new(n, arcs),
            xy,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn road() -> Road {
        Road {
            g: Graph::new(4, [(0, 1, 0.0), (1, 2, 0.0006), (2, 3, 2.25)]),
            xy: vec![(0.0, 0.0); 4],
        }
    }

    #[test]
    fn metric_preserves_native_costs_and_rounds_with_a_positive_floor() {
        let native = road().with_scale(0).unwrap();
        assert_eq!(native.g.arcs(1).next().unwrap().1, 0.0006);
        let rounded = road().with_scale(1000).unwrap();
        assert_eq!(rounded.g.arcs(0).next().unwrap().1, 1.0);
        assert_eq!(rounded.g.arcs(1).next().unwrap().1, 1.0);
        assert_eq!(rounded.g.arcs(2).next().unwrap().1, 2250.0);
        let huge = Road {
            g: Graph::new(2, [(0, 1, f64::MAX)]),
            xy: vec![(0.0, 0.0); 2],
        };
        assert!(huge.with_scale(1000).is_err());
    }

    #[test]
    fn metrics_align_with_the_road_arcs_and_reject_bad_files() {
        // Two nodes, arcs 1->0 then 0->1 (out of CSR order), two metric columns.
        let mut road = Vec::new();
        road.extend(2u64.to_le_bytes());
        road.extend(2u64.to_le_bytes());
        [0.0f64, 0.0, 0.0, 0.0]
            .iter()
            .for_each(|x| road.extend(x.to_le_bytes()));
        [1u32, 0, 0, 1]
            .iter()
            .for_each(|x| road.extend(x.to_le_bytes()));
        [5.0f64, 7.0]
            .iter()
            .for_each(|x| road.extend(x.to_le_bytes()));
        let mut b = Vec::new();
        b.extend(2u64.to_le_bytes());
        b.extend(2u64.to_le_bytes());
        [1.0f64, 2.0, 3.5, 4.0]
            .iter()
            .for_each(|x| b.extend(x.to_le_bytes()));
        let g = metrics(&road, &b, 0).unwrap();
        assert_eq!(g[0].arcs(0).collect::<Vec<_>>(), [(1, 2.0)]);
        assert_eq!(g[0].arcs(1).collect::<Vec<_>>(), [(0, 1.0)]);
        assert_eq!(g[1].arcs(0).collect::<Vec<_>>(), [(1, 4.0)]);
        assert_eq!(
            metrics(&road, &b, 1000).unwrap()[1].arcs(1).next(),
            Some((0, 3500.0))
        );
        assert!(metrics(&road, &b[..b.len() - 8], 0).is_err());
        let mut bad = b.clone();
        bad[16..24].copy_from_slice(&(-1.0f64).to_le_bytes());
        assert!(metrics(&road, &bad, 0).is_err());
    }
}
