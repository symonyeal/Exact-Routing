//! Decode the little-endian road format emitted by the pinned dataset converters.

use crate::graph::Graph;

/// A validated directed network with projected longitude/latitude coordinates for partitioning.
pub struct Road {
    pub g: Graph,
    pub xy: Vec<(f64, f64)>,
}

impl Road {
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
