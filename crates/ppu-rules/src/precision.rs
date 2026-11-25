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

/// Encode a Q10 expression in storage units, rounding down (floor): an arithmetic
/// right shift, as fixed point stores on the hardware. The caller supplies a
/// validated binary denominator in 1..=1024.
pub fn encode(raw: i32, denominator: u16) -> i32 {
    assert!(denominator.is_power_of_two() && denominator <= 1024);
    raw >> (1024 / denominator as u32).trailing_zeros()
}

/// The logical Q10 value after a store; rounding down never leaves Q21.10.
pub fn snap(raw: i32, denominator: u16) -> Option<i32> {
    encode(raw, denominator).checked_mul(1024 / denominator as i32)
}

/// An error the preview reports at a source line: what is wrong and how to write it instead.
#[derive(Clone, Debug)]
pub struct PreviewError {
    pub line: u64,
    pub message: String,
    pub help: String,
}
impl PreviewError {
    pub fn new(line: u64, message: &str, help: &str) -> Self {
        Self {
            line,
            message: message.into(),
            help: help.into(),
        }
    }
}

/// A numeric annotation comment on a binding: its bounds, array capacity and fixed-point step.
#[derive(Clone, Debug)]
pub struct Annotation {
    pub line: u64,
    pub comment: (u64, String),
    pub name: String,
    pub min: i32,
    pub max: i32,
    pub capacity: Option<usize>,
    pub fixed: bool,
    pub precision: Option<u16>,
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
        assert_eq!(encode(32, 16), 0);
        assert_eq!(encode(64, 16), 1);
        assert_eq!(encode(-32, 16), -1);
        assert_eq!(encode(-1, 16), -1);
        assert_eq!(snap(i32::MIN, 1), Some(i32::MIN));
        assert_eq!(snap(i32::MAX, 1), Some(2097151 * 1024));
    }
}

// Inspect actual short comments, skipping quoted strings and Lua long blocks.
pub fn comments(source: &str) -> Vec<(usize, &str)> {
    let b = source.as_bytes();
    let (mut i, mut line) = (0, 1);
    let mut result = vec![];
    while i < b.len() {
        let start = i;
        if b[i] == b'\'' || b[i] == b'"' {
            let quote = b[i];
            i += 1;
            while i < b.len() {
                if b[i] == b'\\' {
                    i = (i + 2).min(b.len());
                } else if b[i] == quote {
                    i += 1;
                    break;
                } else {
                    i += 1;
                }
            }
        } else {
            let comment = b[i..].starts_with(b"--");
            let open = i + if comment { 2 } else { 0 };
            let mut end = open + 1;
            if b.get(open) == Some(&b'[') {
                while b.get(end) == Some(&b'=') {
                    end += 1;
                }
            }
            if b.get(open) == Some(&b'[') && b.get(end) == Some(&b'[') {
                let close = format!("]{}]", "=".repeat(end - open - 1));
                i = source[end + 1..]
                    .find(&close)
                    .map(|n| end + 1 + n + close.len())
                    .unwrap_or(b.len());
            } else if comment {
                i = source[i..].find('\n').map(|n| i + n).unwrap_or(b.len());
                result.push((line, &source[start..i]));
            } else {
                i += 1;
            }
        }
        line += b[start..i].iter().filter(|&&c| c == b'\n').count();
    }
    result
}
