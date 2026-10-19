//! Secrets: randomness from the operating system, the key the hub and its
//! game servers share, and the proofs that show a side holds it.

use std::io;
use std::path::Path;

use bota_proto::{Nonce, Proof};
use rand::TryRng;
use rand::rngs::SysRng;

/// Thirty-two bytes from the operating system's generator.
///
/// Panics when the operating system gives none: nothing a server hands out
/// as a secret may come from anywhere else.
pub fn fresh() -> [u8; 32] {
    let mut bytes = [0u8; 32];
    SysRng
        .try_fill_bytes(&mut bytes)
        .expect("the operating system's random generator answers");
    bytes
}

/// A number from the operating system's generator.
pub fn fresh_u64() -> u64 {
    let bytes = fresh();
    let mut low = [0u8; 8];
    low.copy_from_slice(&bytes[..8]);
    u64::from_le_bytes(low)
}

/// Which side of the link a proof is made by.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Prover {
    /// The hub.
    Hub,
    /// A game server.
    Game,
}

/// A proof that whoever made it holds the key: the keyed hash of the number
/// under the key, prefixed by the side that makes it, so a proof from one
/// side never passes for the other's.
pub fn prove(key: &[u8; 32], by: Prover, nonce: &Nonce) -> Proof {
    let label: &[u8] = match by {
        Prover::Hub => b"bota hub",
        Prover::Game => b"bota game",
    };
    let mut input = Vec::with_capacity(label.len() + nonce.0.len());
    input.extend_from_slice(label);
    input.extend_from_slice(&nonce.0);
    Proof(*blake3::keyed_hash(key, &input).as_bytes())
}

/// Whether a proof is the one the key makes for that side and number.
pub fn proves(key: &[u8; 32], by: Prover, nonce: &Nonce, proof: &Proof) -> bool {
    same(&prove(key, by, nonce).0, &proof.0)
}

/// Whether two secrets are the same, compared in time that does not depend
/// on where they differ.
pub fn same(one: &[u8; 32], other: &[u8; 32]) -> bool {
    one.iter()
        .zip(other)
        .fold(0u8, |differ, (a, b)| differ | (a ^ b))
        == 0
}

/// The shared key kept in a file as 64 hexadecimal digits.
pub fn read_key(path: &Path) -> io::Result<[u8; 32]> {
    let text = std::fs::read_to_string(path)?;
    let digits = text.trim();
    let bad = || io::Error::new(io::ErrorKind::InvalidData, "a key is 64 hexadecimal digits");
    if digits.len() != 64 || !digits.is_ascii() {
        return Err(bad());
    }
    let mut key = [0u8; 32];
    for (at, byte) in key.iter_mut().enumerate() {
        *byte = u8::from_str_radix(&digits[at * 2..at * 2 + 2], 16).map_err(|_| bad())?;
    }
    Ok(key)
}

/// The shared key from a file, made fresh and written there when the file
/// is missing.
pub fn read_or_make_key(path: &Path) -> io::Result<[u8; 32]> {
    match read_key(path) {
        Err(err) if err.kind() == io::ErrorKind::NotFound => {
            let key = fresh();
            let digits: String = key.iter().map(|byte| format!("{byte:02x}")).collect();
            if let Some(parent) = path.parent() {
                std::fs::create_dir_all(parent)?;
            }
            std::fs::write(path, digits + "\n")?;
            Ok(key)
        }
        other => other,
    }
}
