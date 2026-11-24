//! Decode the little-endian road format emitted by the pinned dataset converters.

use crate::graph::Graph;

/// A validated directed network with projected longitude/latitude coordinates for partitioning.
pub struct Road {
    pub g: Graph,
    pub xy: Vec<(f64, f64)>,
}

impl Road {
    /// Keep native seconds at scale 0, or replace each cost by max(1, round(scale * seconds)).
    pub fn with_scale(mut self, scale: usize) -> Result<Road, &'static str> {
        if scale > 0 {
            if self.g.w.iter().any(|w| !(w * scale as f64).is_finite()) {
                return Err("scaled road cost overflows");
            }
            self.g = Graph::new(
                self.g.n,
                (0..self.g.n).flat_map(|u| {
                    self.g
                        .arcs(u)
                        .map(move |(v, w)| (u as u32, v, (w * scale as f64).round().max(1.0)))
                }),
            );
        }
        Ok(self)
    }

    /// Require n,m:u64; lat,lon:f64[n]; tail,head:u32[m]; nonnegative weight:f64[m].
    /// Coordinates must be finite geographic degrees and every endpoint must belong to the graph.
    pub fn decode(b: &[u8]) -> Result<Road, &'static str> {
        if b.len() < 16 {
            return Err("short road header");
        }
        let word = |i: usize| u64::from_le_bytes(b[i..i + 8].try_into().unwrap());
        let n = usize::try_from(word(0)).map_err(|_| "node count overflow")?;
        let m = usize::try_from(word(8)).map_err(|_| "arc count overflow")?;
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
}
