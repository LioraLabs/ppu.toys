//! Canonical `snes fixed` storage law. Expressions use Q10; only stores choose
//! a binary denominator, 1..=1024. Bounds check the rounded stored value.

/// Parse the exact `1/N` step spelling (binary N, no finer than Q10).
pub fn denominator(step: &str) -> Option<u16> {
    let digits = step.strip_prefix("1/")?;
    if digits.is_empty() || !digits.bytes().all(|b| b.is_ascii_digit()) {
        return None;
    }
    let n: u16 = digits.parse().ok()?;
    (n.is_power_of_two() && n <= 1024).then_some(n)
}

/// Encode a Q10 expression in storage units, rounding nearest, ties away from
/// zero. The caller supplies a validated binary denominator in 1..=1024.
pub fn encode(raw: i32, denominator: u16) -> i32 {
    assert!(denominator.is_power_of_two() && denominator <= 1024);
    let divisor = 1024 / denominator as i64;
    let magnitude = (raw as i64).abs();
    let rounded = (magnitude + divisor / 2) / divisor;
    (if raw < 0 { -rounded } else { rounded }) as i32
}

/// The logical Q10 value after a store; None means rounding exceeds Q10.
pub fn snap(raw: i32, denominator: u16) -> Option<i32> {
    encode(raw, denominator).checked_mul(1024 / denominator as i32)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn store_contract() {
        assert_eq!(denominator("1/16"), Some(16));
        for bad in ["1/0", "1/3", "1/2048", "2/16", "1/+16", "1/1.0"] {
            assert_eq!(denominator(bad), None, "{bad}");
        }
        assert_eq!(encode(204, 16), 3);
        assert_eq!(encode(32, 16), 1);
        assert_eq!(encode(-32, 16), -1);
        assert_eq!(encode(-31, 16), 0);
        assert_eq!(snap(i32::MIN, 1), Some(i32::MIN));
        assert_eq!(snap(i32::MAX, 1), None);
    }
}
