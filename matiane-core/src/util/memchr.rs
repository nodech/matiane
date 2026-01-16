// MIT License
//
// Copyright (c) Tokio Contributors

// Permission is hereby granted, free of charge, to any person obtaining a copy
// of this software and associated documentation files (the "Software"), to deal
// in the Software without restriction, including without limitation the rights
// to use, copy, modify, merge, publish, distribute, sublicense, and/or sell
// copies of the Software, and to permit persons to whom the Software is
// furnished to do so, subject to the following conditions:

// The above copyright notice and this permission notice shall be included in all
// copies or substantial portions of the Software.

// THE SOFTWARE IS PROVIDED "AS IS", WITHOUT WARRANTY OF ANY KIND, EXPRESS OR
// IMPLIED, INCLUDING BUT NOT LIMITED TO THE WARRANTIES OF MERCHANTABILITY,
// FITNESS FOR A PARTICULAR PURPOSE AND NONINFRINGEMENT. IN NO EVENT SHALL THE
// AUTHORS OR COPYRIGHT HOLDERS BE LIABLE FOR ANY CLAIM, DAMAGES OR OTHER
// LIABILITY, WHETHER IN AN ACTION OF CONTRACT, TORT OR OTHERWISE, ARISING FROM,
// OUT OF OR IN CONNECTION WITH THE SOFTWARE OR THE USE OR OTHER DEALINGS IN THE
// SOFTWARE.

//! Search for a byte in a byte array using libc.
//!
//! When nothing pulls in libc, then just use a trivial implementation. Note
//! that we only depend on libc on unix.

#[cfg(not(unix))]
pub(crate) fn memchr(needle: u8, haystack: &[u8]) -> Option<usize> {
    haystack.iter().position(|val| needle == *val)
}

#[cfg(unix)]
pub(crate) fn memchr(needle: u8, haystack: &[u8]) -> Option<usize> {
    let start = haystack.as_ptr();

    // SAFETY: `start` is valid for `haystack.len()` bytes.
    let ptr =
        unsafe { libc::memchr(start.cast(), needle as _, haystack.len()) };

    if ptr.is_null() {
        None
    } else {
        Some(ptr as usize - start as usize)
    }
}

#[cfg(test)]
mod tests {
    use super::memchr;

    #[test]
    fn memchr_test() {
        let haystack = b"123abc456\0\xffabc\n";

        assert_eq!(memchr(b'1', haystack), Some(0));
        assert_eq!(memchr(b'2', haystack), Some(1));
        assert_eq!(memchr(b'3', haystack), Some(2));
        assert_eq!(memchr(b'4', haystack), Some(6));
        assert_eq!(memchr(b'5', haystack), Some(7));
        assert_eq!(memchr(b'6', haystack), Some(8));
        assert_eq!(memchr(b'7', haystack), None);
        assert_eq!(memchr(b'a', haystack), Some(3));
        assert_eq!(memchr(b'b', haystack), Some(4));
        assert_eq!(memchr(b'c', haystack), Some(5));
        assert_eq!(memchr(b'd', haystack), None);
        assert_eq!(memchr(b'A', haystack), None);
        assert_eq!(memchr(0, haystack), Some(9));
        assert_eq!(memchr(0xff, haystack), Some(10));
        assert_eq!(memchr(0xfe, haystack), None);
        assert_eq!(memchr(1, haystack), None);
        assert_eq!(memchr(b'\n', haystack), Some(14));
        assert_eq!(memchr(b'\r', haystack), None);
    }

    #[test]
    fn memchr_all() {
        let mut arr = Vec::new();
        for b in 0..=255 {
            arr.push(b);
        }
        for b in 0..=255 {
            assert_eq!(memchr(b, &arr), Some(b as usize));
        }
        arr.reverse();
        for b in 0..=255 {
            assert_eq!(memchr(b, &arr), Some(255 - b as usize));
        }
    }

    #[test]
    fn memchr_empty() {
        for b in 0..=255 {
            assert_eq!(memchr(b, b""), None);
        }
    }
}
