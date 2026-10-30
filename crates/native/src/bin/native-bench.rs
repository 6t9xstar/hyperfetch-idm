//! Benchmark + correctness gate for the native kernels.
//!
//! Run with:
//!   cargo run -p hyperfetch-native --release --features bench --bin native-bench
//!
//! Exit code 0 means every available kernel is at least 15% faster than its
//! RustCrypto reference, which keeps the `native-accel` feature justified.
//! Exit code 1 means the native module should stay disabled (no gate pass).

use cbc::cipher::{BlockDecryptMut, BlockEncryptMut, KeyIvInit};
use sha2::Digest;
use std::time::Instant;

const ROUND_PAYLOAD_BYTES: usize = 128 * 1024 * 1024;
const ROUNDS: u32 = 3;
const GATE_FACTOR: f64 = 1.15;

fn mbps(bytes: u64, duration: std::time::Duration) -> f64 {
    (bytes as f64 / 1_048_576.0) / duration.as_secs_f64()
}

fn main() {
    let payload = vec![7u8; ROUND_PAYLOAD_BYTES];

    println!("hyperfetch-native gate");
    println!("aes-ni available: {}", hyperfetch_native::has_aes_ni());
    println!("sha-ni  available: {}", hyperfetch_native::has_sha_ni());
    println!();

    let mut gate_pass = true;

    if hyperfetch_native::has_aes_ni() {
        let key = [7u8; 16];
        let iv = [9u8; 16];

        let cipher = encrypt_padded(&key, &iv, &payload);
        let decrypted = hyperfetch_native::decrypt_aes128_cbc_padded(&key, &iv, &cipher);
        match decrypted {
            Some(plain) if plain == payload => println!("AES correctness: OK"),
            _ => {
                println!("AES correctness: FAILED — native kernel is broken");
                std::process::exit(1);
            }
        }

        let mut start = Instant::now();
        let mut total = 0u64;
        for _ in 0..ROUNDS {
            let plain = hyperfetch_native::decrypt_aes128_cbc_padded(&key, &iv, &cipher);
            debug_assert!(plain.is_some());
            total += cipher.len() as u64;
        }
        let native_mbps = mbps(total, start.elapsed());

        start = Instant::now();
        total = 0;
        for _ in 0..ROUNDS {
            let mut buffer = cipher.clone();
            let _ = cbc::Decryptor::<aes::Aes128>::new_from_slices(&key, &iv)
                .unwrap()
                .decrypt_padded_mut::<cbc::cipher::block_padding::Pkcs7>(&mut buffer)
                .unwrap();
            total += cipher.len() as u64;
        }
        let reference_mbps = mbps(total, start.elapsed());

        let speedup = native_mbps / reference_mbps;
        println!("AES-128-CBC decrypt");
        println!("  native    : {native_mbps:.0} MiB/s");
        println!("  rustcrypto: {reference_mbps:.0} MiB/s");
        println!("  speedup   : {speedup:.2}x");
        if speedup < GATE_FACTOR {
            println!("  BELOW {GATE_FACTOR:.2}x gate — drop the AES-NI kernel.");
            gate_pass = false;
        } else {
            println!("  gate OK");
        }
        println!();
    } else {
        println!("AES-NI unavailable on this CPU/build — kernel cannot be gated here.");
        println!();
    }

    if hyperfetch_native::has_sha_ni() {
        let native_digest = hyperfetch_native::sha256(&payload);
        let reference_digest: [u8; 32] = sha2::Sha256::digest(&payload).into();
        if native_digest == reference_digest {
            println!("SHA-256 correctness: OK");
        } else {
            println!("SHA-256 correctness: FAILED — native kernel is broken");
            std::process::exit(1);
        }

        let mut start = Instant::now();
        let mut total = 0u64;
        for _ in 0..ROUNDS {
            let _ = hyperfetch_native::sha256(&payload);
            total += payload.len() as u64;
        }
        let native_mbps = mbps(total, start.elapsed());

        start = Instant::now();
        total = 0;
        for _ in 0..ROUNDS {
            let _ = sha2::Sha256::digest(&payload);
            total += payload.len() as u64;
        }
        let reference_mbps = mbps(total, start.elapsed());

        let speedup = native_mbps / reference_mbps;
        println!("SHA-256");
        println!("  native    : {native_mbps:.0} MiB/s");
        println!("  rustcrypto: {reference_mbps:.0} MiB/s");
        println!("  speedup   : {speedup:.2}x");
        if speedup < GATE_FACTOR {
            println!("  BELOW {GATE_FACTOR:.2}x gate — drop the SHA-NI kernel.");
            gate_pass = false;
        } else {
            println!("  gate OK");
        }
        println!();
    } else {
        println!("SHA-NI unavailable in this build — kernel cannot be gated here.");
        println!();
    }

    if gate_pass {
        println!("gate verdict: PASS — native-accel feature is justified");
    } else {
        println!("gate verdict: FAIL — keep native-accel disabled");
        std::process::exit(1);
    }
}

fn encrypt_padded(key: &[u8; 16], iv: &[u8; 16], data: &[u8]) -> Vec<u8> {
    type Aes128CbcEnc = cbc::Encryptor<aes::Aes128>;
    let mut buffer = vec![0u8; data.len() + 16];
    buffer[..data.len()].copy_from_slice(data);
    let length = Aes128CbcEnc::new_from_slices(key, iv)
        .unwrap()
        .encrypt_padded_mut::<cbc::cipher::block_padding::Pkcs7>(&mut buffer, data.len())
        .unwrap()
        .len();
    buffer.truncate(length);
    buffer
}
