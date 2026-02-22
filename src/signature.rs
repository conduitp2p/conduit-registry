//! Lightning-standard message signature verification (standalone, no ldk-node).
//!
//! Replicates the verification half of `lightning::util::message_signing`
//! using only `secp256k1` and `sha2`.  The signature format is:
//!
//!   1. Message digest: SHA256(SHA256("Lightning Signed Message:" || msg))
//!   2. Signature: zbase32-encoded 65-byte recoverable ECDSA signature
//!      byte[0] = recovery_id + 31, bytes[1..65] = compact (r, s)

use secp256k1::ecdsa::RecoverableSignature;
use secp256k1::{Message, Secp256k1};
use sha2::{Digest, Sha256};

// -----------------------------------------------------------------------
// zbase32 decode (RFC 6189 human-oriented encoding)
// -----------------------------------------------------------------------

const ZBASE32_ALPHABET: &[u8; 32] = b"ybndrfg8ejkmcpqxot1uwisza345h769";

fn zbase32_decode(input: &str) -> Option<Vec<u8>> {
    let mut lookup = [255u8; 128];
    for (i, &ch) in ZBASE32_ALPHABET.iter().enumerate() {
        lookup[ch as usize] = i as u8;
    }

    let mut bits: u64 = 0;
    let mut num_bits: u32 = 0;
    let mut output = Vec::with_capacity(input.len() * 5 / 8 + 1);

    for &byte in input.as_bytes() {
        if byte >= 128 {
            return None;
        }
        let val = lookup[byte as usize];
        if val == 255 {
            return None;
        }
        bits = (bits << 5) | val as u64;
        num_bits += 5;
        if num_bits >= 8 {
            num_bits -= 8;
            output.push((bits >> num_bits) as u8);
            bits &= (1u64 << num_bits) - 1;
        }
    }
    Some(output)
}

// -----------------------------------------------------------------------
// Lightning message hash: SHA256d("Lightning Signed Message:" || msg)
// -----------------------------------------------------------------------

fn lightning_message_hash(msg: &[u8]) -> [u8; 32] {
    let mut h1 = Sha256::new();
    h1.update(b"Lightning Signed Message:");
    h1.update(msg);
    let first = h1.finalize();

    let mut h2 = Sha256::new();
    h2.update(&first);
    let second = h2.finalize();

    let mut out = [0u8; 32];
    out.copy_from_slice(&second);
    out
}

// -----------------------------------------------------------------------
// Public API
// -----------------------------------------------------------------------

/// Verify a Lightning-standard ECDSA recoverable signature.
///
/// Returns `true` if the recovered public key matches `expected_pubkey_hex`
/// (the 33-byte compressed secp256k1 pubkey in hex, as returned by
/// `node.node_id().to_string()`).
pub fn verify_lightning_signature(
    msg: &[u8],
    sig_zbase32: &str,
    expected_pubkey_hex: &str,
) -> bool {
    let sig_bytes = match zbase32_decode(sig_zbase32) {
        Some(b) if b.len() == 65 => b,
        _ => {
            eprintln!("verify_lightning_signature: zbase32 decode failed or wrong length");
            return false;
        }
    };

    let recovery_id_raw = sig_bytes[0] as i32 - 31;
    let recovery_id = match secp256k1::ecdsa::RecoveryId::from_i32(recovery_id_raw) {
        Ok(id) => id,
        Err(_) => {
            eprintln!(
                "verify_lightning_signature: invalid recovery id {}",
                recovery_id_raw
            );
            return false;
        }
    };

    let sig = match RecoverableSignature::from_compact(&sig_bytes[1..65], recovery_id) {
        Ok(s) => s,
        Err(e) => {
            eprintln!("verify_lightning_signature: invalid compact sig: {}", e);
            return false;
        }
    };

    let digest = lightning_message_hash(msg);
    let message = match Message::from_digest(digest) {
        msg => msg,
    };

    let secp = Secp256k1::verification_only();
    let recovered_pk = match secp.recover_ecdsa(&message, &sig) {
        Ok(pk) => pk,
        Err(e) => {
            eprintln!("verify_lightning_signature: recovery failed: {}", e);
            return false;
        }
    };

    let recovered_hex = hex::encode(recovered_pk.serialize());
    if recovered_hex != expected_pubkey_hex {
        eprintln!(
            "verify_lightning_signature: pubkey mismatch: recovered={} expected={}",
            &recovered_hex[..16],
            &expected_pubkey_hex[..16.min(expected_pubkey_hex.len())]
        );
        return false;
    }

    true
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn zbase32_roundtrip_known() {
        // "yy" decodes to [0, 0] in zbase32
        let decoded = zbase32_decode("yy").unwrap();
        assert_eq!(decoded, vec![0]);
    }

    #[test]
    fn bad_zbase32_returns_none() {
        assert!(zbase32_decode("!!!").is_none());
    }

    #[test]
    fn lightning_hash_is_double_sha256() {
        let msg = b"test";
        let hash = lightning_message_hash(msg);
        // Manually compute: SHA256(SHA256("Lightning Signed Message:" || "test"))
        let mut h1 = Sha256::new();
        h1.update(b"Lightning Signed Message:");
        h1.update(b"test");
        let first = h1.finalize();
        let mut h2 = Sha256::new();
        h2.update(&first);
        let expected = h2.finalize();
        assert_eq!(hash, expected.as_slice());
    }
}
