use std::cmp::Ordering;

/// Case-insensitive "natural" comparison: runs of digits compare by numeric
/// value, so `file2` sorts before `file10`.
pub fn natural_cmp(a: &str, b: &str) -> Ordering {
    natural_cmp_folded(&a.to_lowercase(), &b.to_lowercase())
}

/// [`natural_cmp`] for strings that are already lowercased. Allocation-free:
/// works on bytes, which is sound because ASCII digits never occur inside
/// multi-byte UTF-8 sequences and UTF-8 byte order is code point order.
pub fn natural_cmp_folded(a: &str, b: &str) -> Ordering {
    let (a, b) = (a.as_bytes(), b.as_bytes());
    let (mut i, mut j) = (0, 0);
    while i < a.len() && j < b.len() {
        if a[i].is_ascii_digit() && b[j].is_ascii_digit() {
            let (start_i, start_j) = (i, j);
            while i < a.len() && a[i].is_ascii_digit() {
                i += 1;
            }
            while j < b.len() && b[j].is_ascii_digit() {
                j += 1;
            }
            let ord = compare_digits(&a[start_i..i], &b[start_j..j]);
            if ord != Ordering::Equal {
                return ord;
            }
        } else {
            if a[i] != b[j] {
                return a[i].cmp(&b[j]);
            }
            i += 1;
            j += 1;
        }
    }
    (a.len() - i).cmp(&(b.len() - j))
}

/// Compares digit strings numerically without overflow: strip leading zeros,
/// then longer is larger, then lexicographic. Ties prefer fewer leading zeros.
fn compare_digits(a: &[u8], b: &[u8]) -> Ordering {
    let trim = |s: &[u8]| -> usize { s.iter().take_while(|&&c| c == b'0').count() };
    let (ta, tb) = (&a[trim(a)..], &b[trim(b)..]);
    ta.len()
        .cmp(&tb.len())
        .then_with(|| ta.cmp(tb))
        .then_with(|| a.len().cmp(&b.len()))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sorted(mut v: Vec<&str>) -> Vec<&str> {
        v.sort_by(|a, b| natural_cmp(a, b));
        v
    }

    #[test]
    fn numbers_compare_by_value() {
        assert_eq!(
            sorted(vec!["file10", "file2", "file1", "File3"]),
            ["file1", "file2", "File3", "file10"]
        );
    }

    #[test]
    fn huge_numbers_do_not_overflow() {
        assert_eq!(
            natural_cmp("x99999999999999999999999", "x100000000000000000000000"),
            Ordering::Less
        );
    }

    #[test]
    fn leading_zeros() {
        assert_eq!(natural_cmp("a007", "a7"), Ordering::Greater);
        assert_eq!(natural_cmp("a007", "a8"), Ordering::Less);
    }

    #[test]
    fn case_insensitive_and_prefixes() {
        assert_eq!(natural_cmp("ABC", "abc"), Ordering::Equal);
        assert_eq!(natural_cmp("ab", "abc"), Ordering::Less);
        assert_eq!(natural_cmp("", ""), Ordering::Equal);
    }
}
