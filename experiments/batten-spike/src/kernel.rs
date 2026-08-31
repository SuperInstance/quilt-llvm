//! Minimal reimplementation of batten-spline's estimator, ported to Rust.
//!
//! WHY REIMPLEMENT (library-vs-reimplement call):
//!   - batten-spline is a Python package (numpy); this spike is a
//!     zero-dependency Rust crate. A path dependency is impossible across
//!     languages, and shelling out to Python per-query would dominate the
//!     measurement we are trying to take.
//!   - The library's CascadeRouter maps ONE confidence scalar to
//!     LOCAL/CASCADE/CLOUD targets. Pipeline routing needs a per-candidate
//!     estimate (one spline per pipeline per metric, then argmax), which
//!     does not fit that API shape.
//!   - The age-decay half-life is dropped deliberately: this spike runs on
//!     a static corpus with no wall-clock semantics, so all battens are
//!     "fresh". Everything else matches BattenSpline.estimate_confidence
//!     (Nadaraya-Watson, Gaussian/RBF kernel) and BattenSpline.fog_density
//!     (distance to nearest batten).

/// A verified outcome: a point in feature space with a measured value.
#[derive(Clone, Debug)]
pub struct Batten {
    pub x: Vec<f64>,
    pub v: f64, // measured value (utility or cost), not clipped: costs > 1 exist
}

/// Nadaraya-Watson estimate with RBF kernel (batten-spline minus age decay).
#[derive(Clone, Debug)]
pub struct Spline {
    pub battens: Vec<Batten>,
    pub fog_scale: f64,
}

impl Spline {
    pub fn new(fog_scale: f64) -> Self {
        Spline { battens: Vec::new(), fog_scale }
    }

    pub fn add(&mut self, x: Vec<f64>, v: f64) {
        self.battens.push(Batten { x, v });
    }

    fn dist(a: &[f64], b: &[f64]) -> f64 {
        a.iter().zip(b).map(|(p, q)| (p - q) * (p - q)).sum::<f64>().sqrt()
    }

    /// Interpolated estimate; 0.0 with no battens (complete fog).
    pub fn estimate(&self, x: &[f64]) -> f64 {
        if self.battens.is_empty() {
            return 0.0;
        }
        let two_sigma2 = 2.0 * self.fog_scale * self.fog_scale;
        let (mut wsum, mut vsum) = (0.0, 0.0);
        for b in &self.battens {
            let d = Self::dist(x, &b.x);
            let w = (-(d * d) / two_sigma2).exp();
            wsum += w;
            vsum += w * b.v;
        }
        if wsum < 1e-12 {
            0.0
        } else {
            vsum / wsum
        }
    }

    /// Distance to nearest batten. Higher = thicker fog.
    pub fn fog(&self, x: &[f64]) -> f64 {
        self.battens
            .iter()
            .map(|b| Self::dist(x, &b.x))
            .fold(f64::INFINITY, f64::min)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn no_battens_is_total_fog() {
        let s = Spline::new(1.0);
        assert_eq!(s.estimate(&[0.0, 0.0]), 0.0);
        assert!(s.fog(&[0.0, 0.0]).is_infinite());
    }

    #[test]
    fn near_batten_dominates() {
        let mut s = Spline::new(0.5);
        s.add(vec![0.0, 0.0], 1.0);
        s.add(vec![10.0, 10.0], 0.0);
        assert!(s.estimate(&[0.05, 0.0]) > 0.999);
        assert!(s.estimate(&[9.9, 10.0]) < 0.001);
    }

    #[test]
    fn interpolates_between_battens() {
        let mut s = Spline::new(1.0);
        s.add(vec![0.0], 0.0);
        s.add(vec![1.0], 1.0);
        let mid = s.estimate(&[0.5]);
        assert!((mid - 0.5).abs() < 1e-9); // symmetric weights average
    }

    #[test]
    fn fog_is_nearest_distance() {
        let mut s = Spline::new(1.0);
        s.add(vec![0.0, 0.0], 1.0);
        s.add(vec![4.0, 0.0], 1.0);
        assert!((s.fog(&[1.0, 0.0]) - 1.0).abs() < 1e-12);
        assert!((s.fog(&[3.0, 0.0]) - 1.0).abs() < 1e-12);
    }
}
