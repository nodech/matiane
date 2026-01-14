#[cfg(not(unix))]
pub(crate) fn memrchr(needle: u8, haystack: &[u8]) -> Option<usize> {
    haystack.iter().rev().position(|val| needle == *val)
}

#[cfg(unix)]
pub(crate) fn memrchr(needle: u8, haystack: &[u8]) -> Option<usize> {
    let start = haystack.as_ptr();

    // SAFETY: `start` is valid for `haystack.len()` bytes.
    let ptr = unsafe { libc::memrchr(start.cast(), needle as _, haystack.len()) };

    if ptr.is_null() {
        None
    } else {
        Some(ptr as usize - start as usize)
    }
}

mod tests {
    use super::memrchr;

    #[test]
    fn memrchr_test() {
        let haystack = b"123abc456\0\xffabc1\n";

        assert_eq!(memrchr(b'1', haystack), Some(14));
        assert_eq!(memrchr(b'2', haystack), Some(1));
        assert_eq!(memrchr(b'3', haystack), Some(2));
        assert_eq!(memrchr(b'4', haystack), Some(6));
        assert_eq!(memrchr(b'5', haystack), Some(7));
        assert_eq!(memrchr(b'6', haystack), Some(8));
        assert_eq!(memrchr(b'7', haystack), None);
        assert_eq!(memrchr(b'a', haystack), Some(11));
        assert_eq!(memrchr(b'b', haystack), Some(12));
        assert_eq!(memrchr(b'c', haystack), Some(13));
        assert_eq!(memrchr(b'd', haystack), None);
        assert_eq!(memrchr(b'A', haystack), None);
        assert_eq!(memrchr(0, haystack), Some(9));
        assert_eq!(memrchr(0xff, haystack), Some(10));
        assert_eq!(memrchr(0xfe, haystack), None);
        assert_eq!(memrchr(1, haystack), None);
        assert_eq!(memrchr(b'\n', haystack), Some(15));
        assert_eq!(memrchr(b'\r', haystack), None);
    }

    #[test]
    fn memrchr_all() {
        let mut arr = Vec::new();
        for b in 0..=255 {
            arr.push(b);
        }
        for b in 0..=255 {
            assert_eq!(memrchr(b, &arr), Some(b as usize));
        }
        arr.reverse();
        for b in 0..=255 {
            assert_eq!(memrchr(b, &arr), Some(255 - b as usize));
        }
    }

    #[test]
    fn memrchr_empty() {
        for b in 0..=255 {
            assert_eq!(memrchr(b, b""), None);
        }
    }
}
