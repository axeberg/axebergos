//! Checksum verification for package integrity
//!
//! Uses SHA-256 for cryptographic hashing.

use super::error::{PkgError, PkgResult};

/// A SHA-256 checksum (32 bytes)
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Checksum([u8; 32]);

impl Checksum {
    /// Create a checksum from a byte array
    pub fn new(bytes: [u8; 32]) -> Self {
        Self(bytes)
    }

    /// Create a checksum from hex string
    pub fn from_hex(s: &str) -> PkgResult<Self> {
        let s = s.trim();
        if s.len() != 64 {
            return Err(PkgError::ChecksumMismatch {
                expected: "64 hex characters".to_string(),
                actual: format!("{} characters", s.len()),
            });
        }

        let mut bytes = [0u8; 32];
        for (i, byte) in bytes.iter_mut().enumerate() {
            *byte = u8::from_str_radix(&s[i * 2..i * 2 + 2], 16).map_err(|_| {
                PkgError::ChecksumMismatch {
                    expected: "valid hex".to_string(),
                    actual: s.to_string(),
                }
            })?;
        }

        Ok(Self(bytes))
    }

    /// Convert to hex string
    pub fn to_hex(&self) -> String {
        self.0.iter().map(|b| format!("{:02x}", b)).collect()
    }

    /// Get the raw bytes
    pub fn as_bytes(&self) -> &[u8; 32] {
        &self.0
    }

    /// Compute SHA-256 checksum of data
    pub fn compute(data: &[u8]) -> Self {
        Self(sha256(data))
    }
}

impl std::fmt::Display for Checksum {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.to_hex())
    }
}

/// Verify that data matches expected checksum
pub fn verify_checksum(data: &[u8], expected: &Checksum) -> PkgResult<()> {
    let actual = Checksum::compute(data);
    if &actual != expected {
        return Err(PkgError::ChecksumMismatch {
            expected: expected.to_hex(),
            actual: actual.to_hex(),
        });
    }
    Ok(())
}

// SHA-256 implementation (pure Rust, no dependencies)
// Based on FIPS 180-4

/// SHA-256 initial hash values
const H: [u32; 8] = [
    0x6a09e667, 0xbb67ae85, 0x3c6ef372, 0xa54ff53a, 0x510e527f, 0x9b05688c, 0x1f83d9ab, 0x5be0cd19,
];

/// SHA-256 round constants
const K: [u32; 64] = [
    0x428a2f98, 0x71374491, 0xb5c0fbcf, 0xe9b5dba5, 0x3956c25b, 0x59f111f1, 0x923f82a4, 0xab1c5ed5,
    0xd807aa98, 0x12835b01, 0x243185be, 0x550c7dc3, 0x72be5d74, 0x80deb1fe, 0x9bdc06a7, 0xc19bf174,
    0xe49b69c1, 0xefbe4786, 0x0fc19dc6, 0x240ca1cc, 0x2de92c6f, 0x4a7484aa, 0x5cb0a9dc, 0x76f988da,
    0x983e5152, 0xa831c66d, 0xb00327c8, 0xbf597fc7, 0xc6e00bf3, 0xd5a79147, 0x06ca6351, 0x14292967,
    0x27b70a85, 0x2e1b2138, 0x4d2c6dfc, 0x53380d13, 0x650a7354, 0x766a0abb, 0x81c2c92e, 0x92722c85,
    0xa2bfe8a1, 0xa81a664b, 0xc24b8b70, 0xc76c51a3, 0xd192e819, 0xd6990624, 0xf40e3585, 0x106aa070,
    0x19a4c116, 0x1e376c08, 0x2748774c, 0x34b0bcb5, 0x391c0cb3, 0x4ed8aa4a, 0x5b9cca4f, 0x682e6ff3,
    0x748f82ee, 0x78a5636f, 0x84c87814, 0x8cc70208, 0x90befffa, 0xa4506ceb, 0xbef9a3f7, 0xc67178f2,
];

/// Right rotate
#[inline]
fn rotr(x: u32, n: u32) -> u32 {
    (x >> n) | (x << (32 - n))
}

/// SHA-256 Ch function
#[inline]
fn ch(x: u32, y: u32, z: u32) -> u32 {
    (x & y) ^ (!x & z)
}

/// SHA-256 Maj function
#[inline]
fn maj(x: u32, y: u32, z: u32) -> u32 {
    (x & y) ^ (x & z) ^ (y & z)
}

/// SHA-256 Sigma0 function
#[inline]
fn sigma0(x: u32) -> u32 {
    rotr(x, 2) ^ rotr(x, 13) ^ rotr(x, 22)
}

/// SHA-256 Sigma1 function
#[inline]
fn sigma1(x: u32) -> u32 {
    rotr(x, 6) ^ rotr(x, 11) ^ rotr(x, 25)
}

/// SHA-256 sigma0 function (message schedule)
#[inline]
fn lsigma0(x: u32) -> u32 {
    rotr(x, 7) ^ rotr(x, 18) ^ (x >> 3)
}

/// SHA-256 sigma1 function (message schedule)
#[inline]
fn lsigma1(x: u32) -> u32 {
    rotr(x, 17) ^ rotr(x, 19) ^ (x >> 10)
}

/// Compute SHA-256 hash of data
pub fn sha256(data: &[u8]) -> [u8; 32] {
    // Initialize hash values
    let mut h = H;

    // Pre-processing: adding padding bits
    let msg_len = data.len();
    let bit_len = (msg_len as u64) * 8;

    // Calculate padded length (multiple of 64 bytes)
    let pad_len = if msg_len % 64 < 56 {
        56 - (msg_len % 64)
    } else {
        120 - (msg_len % 64)
    };

    let total_len = msg_len + pad_len + 8;
    let mut padded = vec![0u8; total_len];
    padded[..msg_len].copy_from_slice(data);
    padded[msg_len] = 0x80; // Append bit '1'

    // Append length in bits as big-endian 64-bit integer
    padded[total_len - 8..].copy_from_slice(&bit_len.to_be_bytes());

    // Process each 64-byte block
    for block in padded.chunks(64) {
        // Prepare message schedule
        let mut w = [0u32; 64];
        for (i, chunk) in block.chunks(4).enumerate() {
            w[i] = u32::from_be_bytes([chunk[0], chunk[1], chunk[2], chunk[3]]);
        }
        for i in 16..64 {
            w[i] = lsigma1(w[i - 2])
                .wrapping_add(w[i - 7])
                .wrapping_add(lsigma0(w[i - 15]))
                .wrapping_add(w[i - 16]);
        }

        // Initialize working variables
        let (mut a, mut b, mut c, mut d, mut e, mut f, mut g, mut hh) =
            (h[0], h[1], h[2], h[3], h[4], h[5], h[6], h[7]);

        // Main loop
        for i in 0..64 {
            let t1 = hh
                .wrapping_add(sigma1(e))
                .wrapping_add(ch(e, f, g))
                .wrapping_add(K[i])
                .wrapping_add(w[i]);
            let t2 = sigma0(a).wrapping_add(maj(a, b, c));

            hh = g;
            g = f;
            f = e;
            e = d.wrapping_add(t1);
            d = c;
            c = b;
            b = a;
            a = t1.wrapping_add(t2);
        }

        // Add to hash values
        h[0] = h[0].wrapping_add(a);
        h[1] = h[1].wrapping_add(b);
        h[2] = h[2].wrapping_add(c);
        h[3] = h[3].wrapping_add(d);
        h[4] = h[4].wrapping_add(e);
        h[5] = h[5].wrapping_add(f);
        h[6] = h[6].wrapping_add(g);
        h[7] = h[7].wrapping_add(hh);
    }

    // Produce final hash
    let mut result = [0u8; 32];
    for (i, &val) in h.iter().enumerate() {
        result[i * 4..(i + 1) * 4].copy_from_slice(&val.to_be_bytes());
    }

    result
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_sha256_empty() {
        // SHA-256 of empty string
        let hash = sha256(b"");
        let expected = Checksum::from_hex(
            "e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855",
        )
        .unwrap();
        assert_eq!(hash, *expected.as_bytes());
    }

    #[test]
    fn test_sha256_hello() {
        // SHA-256 of "hello"
        let hash = sha256(b"hello");
        let expected = Checksum::from_hex(
            "2cf24dba5fb0a30e26e83b2ac5b9e29e1b161e5c1fa7425e73043362938b9824",
        )
        .unwrap();
        assert_eq!(hash, *expected.as_bytes());
    }

    #[test]
    fn test_sha256_hello_world() {
        // SHA-256 of "Hello, World!"
        let hash = sha256(b"Hello, World!");
        let expected = Checksum::from_hex(
            "dffd6021bb2bd5b0af676290809ec3a53191dd81c7f70a4b28688a362182986f",
        )
        .unwrap();
        assert_eq!(hash, *expected.as_bytes());
    }

    #[test]
    fn test_sha256_long_message() {
        // Test with a message longer than one block
        let data = b"The quick brown fox jumps over the lazy dog";
        let hash = sha256(data);
        let expected = Checksum::from_hex(
            "d7a8fbb307d7809469ca9abcb0082e4f8d5651e46d3cdb762d02d0bf37c9e592",
        )
        .unwrap();
        assert_eq!(hash, *expected.as_bytes());
    }

    #[test]
    fn test_checksum_from_hex() {
        let hex = "2cf24dba5fb0a30e26e83b2ac5b9e29e1b161e5c1fa7425e73043362938b9824";
        let checksum = Checksum::from_hex(hex).unwrap();
        assert_eq!(checksum.to_hex(), hex);
    }

    #[test]
    fn test_checksum_to_hex() {
        let data = b"hello";
        let checksum = Checksum::compute(data);
        assert_eq!(
            checksum.to_hex(),
            "2cf24dba5fb0a30e26e83b2ac5b9e29e1b161e5c1fa7425e73043362938b9824"
        );
    }

    #[test]
    fn test_verify_checksum_success() {
        let data = b"hello";
        let expected = Checksum::from_hex(
            "2cf24dba5fb0a30e26e83b2ac5b9e29e1b161e5c1fa7425e73043362938b9824",
        )
        .unwrap();
        assert!(verify_checksum(data, &expected).is_ok());
    }

    #[test]
    fn test_verify_checksum_failure() {
        let data = b"hello";
        let wrong = Checksum::from_hex(
            "0000000000000000000000000000000000000000000000000000000000000000",
        )
        .unwrap();
        assert!(verify_checksum(data, &wrong).is_err());
    }

    #[test]
    fn test_checksum_compute_eq() {
        let data = b"test data";
        let c1 = Checksum::compute(data);
        let c2 = Checksum::compute(data);
        assert_eq!(c1, c2);
    }

    #[test]
    fn test_checksum_display() {
        let checksum = Checksum::compute(b"hello");
        assert_eq!(
            format!("{}", checksum),
            "2cf24dba5fb0a30e26e83b2ac5b9e29e1b161e5c1fa7425e73043362938b9824"
        );
    }
}
