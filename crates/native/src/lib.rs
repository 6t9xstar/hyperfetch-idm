//! SIMD-accelerated kernels for the HyperFetch engine.
//!
//! Each kernel ships with runtime CPU dispatch and returns availability
//! flags so callers can fall back to their portable RustCrypto paths.

use std::os::raw::{c_int, c_uchar};

#[link(name = "hf_native")]
extern "C" {
    fn hf_has_aes_ni() -> c_int;
    fn hf_has_sha_ni() -> c_int;
    fn hf_aes128_cbc_decrypt(
        key: *const c_uchar,
        iv: *const c_uchar,
        input: *const c_uchar,
        input_len: usize,
        output: *mut c_uchar,
    ) -> c_int;
    fn hf_sha256_init(ctx: *mut Sha256Ctx);
    fn hf_sha256_update(ctx: *mut Sha256Ctx, data: *const c_uchar, len: usize);
    fn hf_sha256_final(ctx: *mut Sha256Ctx, out: *mut c_uchar);
}

/// Storage for the FFI-visible SHA-256 state.
#[repr(C)]
pub struct Sha256Ctx {
    state: [u32; 8],
    bit_count: u64,
    buffer_len: usize,
    buffer: [u8; 64],
}

pub fn has_aes_ni() -> bool {
    unsafe { hf_has_aes_ni() == 1 }
}

pub fn has_sha_ni() -> bool {
    unsafe { hf_has_sha_ni() == 1 }
}

fn cpu_sha256(data: &[u8]) -> [u8; 32] {
    let mut ctx = Sha256Ctx {
        state: [0; 8],
        bit_count: 0,
        buffer_len: 0,
        buffer: [0; 64],
    };
    unsafe {
        hf_sha256_init(&mut ctx);
        if !data.is_empty() {
            hf_sha256_update(&mut ctx, data.as_ptr(), data.len());
        }
        let mut out = [0u8; 32];
        hf_sha256_final(&mut ctx, out.as_mut_ptr());
        out
    }
}

pub fn sha256(data: &[u8]) -> [u8; 32] {
    cpu_sha256(data)
}

pub struct Sha256(Sha256Ctx);

impl Sha256 {
    pub fn new() -> Self {
        let mut ctx = Sha256Ctx {
            state: [0; 8],
            bit_count: 0,
            buffer_len: 0,
            buffer: [0; 64],
        };
        unsafe { hf_sha256_init(&mut ctx) };
        Self(ctx)
    }

    pub fn update(&mut self, data: &[u8]) {
        if !data.is_empty() {
            unsafe { hf_sha256_update(&mut self.0, data.as_ptr(), data.len()) };
        }
    }

    pub fn finalize(mut self) -> [u8; 32] {
        let mut out = [0u8; 32];
        unsafe { hf_sha256_final(&mut self.0, out.as_mut_ptr()) };
        out
    }
}

impl Default for Sha256 {
    fn default() -> Self {
        Self::new()
    }
}

/// Decrypt AES-128-CBC (raw, no padding stripping).
/// Returns `None` when the kernel is unavailable or the buffer is malformed.
pub fn aes128_cbc_decrypt(key: &[u8; 16], iv: &[u8; 16], data: &[u8]) -> Option<Vec<u8>> {
    if data.is_empty() || !data.len().is_multiple_of(16) {
        return None;
    }
    let mut out = vec![0u8; data.len()];
    let code = unsafe {
        hf_aes128_cbc_decrypt(
            key.as_ptr(),
            iv.as_ptr(),
            data.as_ptr(),
            data.len(),
            out.as_mut_ptr(),
        )
    };
    if code != 0 {
        return None;
    }
    Some(out)
}

/// Decrypt AES-128-CBC and strip PKCS#7 padding (HLS segment shape).
pub fn decrypt_aes128_cbc_padded(key: &[u8; 16], iv: &[u8; 16], data: &[u8]) -> Option<Vec<u8>> {
    if data.is_empty() {
        return Some(Vec::new());
    }
    if !data.len().is_multiple_of(16) {
        return None;
    }
    let mut decoded = aes128_cbc_decrypt(key, iv, data)?;
    let pad = decoded[decoded.len() - 1] as usize;
    if pad == 0 || pad > 16 || pad > decoded.len() {
        return None;
    }
    if decoded[decoded.len() - pad..]
        .iter()
        .any(|byte| *byte as usize != pad)
    {
        return None;
    }
    decoded.truncate(decoded.len() - pad);
    Some(decoded)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sha256_matches_reference() {
        assert_eq!(
            hex_sha256(b"abc"),
            "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad"
        );
        assert_eq!(
            hex_sha256(b""),
            "e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855"
        );
        let long_domain = [0x61u8; 1_000_000];
        assert_eq!(
            hex_sha256(&long_domain),
            "cdc76e5c9914fb9281a1c7e284d73e67f1809a48a497200e046d39ccc7112cd0"
        );
    }

    #[test]
    fn sha256_streaming_matches_one_shot() {
        let data: Vec<u8> = (0..100_000u32).map(|i| (i % 251) as u8).collect();
        let mut hasher = Sha256::new();
        for chunk in data.chunks(7919) {
            hasher.update(chunk);
        }
        let hashed = hasher.finalize();
        let one_shot = sha256(&data);
        assert_eq!(hashed, one_shot);
    }

    fn hex_sha256(data: &[u8]) -> String {
        sha256(data)
            .iter()
            .map(|byte| format!("{byte:02x}"))
            .collect()
    }
}
