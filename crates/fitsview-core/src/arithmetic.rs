use rayon::prelude::*;

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum ArithOp {
    Add,
    Sub,
    Mul,
    Div,
    /// (A − B) / B  (relative difference)
    RelDiff,
}

impl std::fmt::Display for ArithOp {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ArithOp::Add => write!(f, "A + B"),
            ArithOp::Sub => write!(f, "A − B"),
            ArithOp::Mul => write!(f, "A × B"),
            ArithOp::Div => write!(f, "A / B"),
            ArithOp::RelDiff => write!(f, "(A − B) / B"),
        }
    }
}

/// Perform pixel-wise arithmetic between two identically-sized images.
///
/// Returns `Err` if the sizes don't match.
pub fn image_arithmetic(
    a: &[f32],
    b: &[f32],
    width: usize,
    height: usize,
    op: ArithOp,
    scale: f32,
) -> anyhow::Result<Vec<f32>> {
    let n = width * height;
    anyhow::ensure!(
        a.len() >= n && b.len() >= n,
        "image size mismatch: a={}, b={}, expected {}",
        a.len(), b.len(), n
    );

    let result: Vec<f32> = a[..n]
        .par_iter()
        .zip(b[..n].par_iter())
        .map(|(&av, &bv)| {
            let v = match op {
                ArithOp::Add => av + bv,
                ArithOp::Sub => av - bv,
                ArithOp::Mul => av * bv,
                ArithOp::Div => if bv.abs() > f32::EPSILON { av / bv } else { f32::NAN },
                ArithOp::RelDiff => if bv.abs() > f32::EPSILON { (av - bv) / bv } else { f32::NAN },
            };
            v * scale
        })
        .collect();

    Ok(result)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_add() {
        let a = vec![1.0f32, 2.0, 3.0];
        let b = vec![4.0f32, 5.0, 6.0];
        let r = image_arithmetic(&a, &b, 3, 1, ArithOp::Add, 1.0).unwrap();
        assert_eq!(r, vec![5.0, 7.0, 9.0]);
    }

    #[test]
    fn test_sub() {
        let a = vec![10.0f32, 20.0];
        let b = vec![3.0f32, 7.0];
        let r = image_arithmetic(&a, &b, 2, 1, ArithOp::Sub, 1.0).unwrap();
        assert_eq!(r, vec![7.0, 13.0]);
    }

    #[test]
    fn test_div_by_zero() {
        let a = vec![1.0f32];
        let b = vec![0.0f32];
        let r = image_arithmetic(&a, &b, 1, 1, ArithOp::Div, 1.0).unwrap();
        assert!(r[0].is_nan());
    }

    #[test]
    fn test_size_mismatch() {
        let a = vec![1.0f32; 4];
        let b = vec![1.0f32; 2];
        let r = image_arithmetic(&a, &b, 2, 2, ArithOp::Add, 1.0);
        assert!(r.is_err());
    }

    #[test]
    fn test_scale() {
        let a = vec![2.0f32];
        let b = vec![1.0f32];
        let r = image_arithmetic(&a, &b, 1, 1, ArithOp::Add, 0.5).unwrap();
        assert!((r[0] - 1.5).abs() < 1e-6);
    }
}
