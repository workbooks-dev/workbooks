//! Workbook signing + verification (#37 signed workbooks, #40 signing, #47
//! signed attestations).
//!
//! Ed25519 detached signatures over a workbook's content (like minisign /
//! signify). `wb keygen` makes a keypair; `wb sign` writes a `<file>.sig`
//! binding the signature to `sha256(content)`; `wb verify-sig` (and
//! `wb run --verify-sig`) check it. The crypto is `ed25519-dalek` (a
//! well-audited library) used in the standard way — we don't roll our own.
//!
//! Honest scope: this proves *integrity + authorship* of a workbook against a
//! public key the verifier already trusts. It is not a PKI / web-of-trust; key
//! distribution and a hosted registry of trusted signers are the larger #40
//! story. Private keys are stored `0600`.

use std::path::{Path, PathBuf};

use ed25519_dalek::{Signature, Signer, SigningKey, Verifier, VerifyingKey};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

/// A detached signature file (`<file>.sig`), JSON.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SignatureFile {
    pub alg: String,
    /// Hex-encoded 32-byte public key.
    pub pubkey: String,
    /// Hex-encoded 64-byte signature over the signed bytes.
    pub signature: String,
    /// Hex sha256 of the content that was signed (for a fast mismatch message).
    pub sha256: String,
}

fn to_hex(bytes: &[u8]) -> String {
    bytes.iter().map(|b| format!("{b:02x}")).collect()
}

fn from_hex(s: &str) -> Result<Vec<u8>, String> {
    // Byte-index slicing below assumes one byte per char; a non-ASCII char would
    // panic on a non-char-boundary. Reject up front (sig fields are attacker-
    // supplied JSON).
    if !s.is_ascii() {
        return Err("non-ascii hex".to_string());
    }
    if !s.len().is_multiple_of(2) {
        return Err("odd-length hex".to_string());
    }
    (0..s.len())
        .step_by(2)
        .map(|i| u8::from_str_radix(&s[i..i + 2], 16).map_err(|e| e.to_string()))
        .collect()
}

/// Default key paths under `~/.wb/keys/` (override the dir with `$WB_KEYS_DIR`).
pub fn default_key_path() -> PathBuf {
    let base = std::env::var_os("WB_KEYS_DIR")
        .map(PathBuf::from)
        .or_else(|| std::env::var_os("HOME").map(|h| PathBuf::from(h).join(".wb").join("keys")))
        .unwrap_or_else(std::env::temp_dir);
    base.join("wb_signing_key")
}

/// Generate a new ed25519 keypair, writing `<prefix>` (private, hex, mode 0600)
/// and `<prefix>.pub` (public, hex). Returns the public key hex.
pub fn keygen(prefix: &Path) -> Result<String, String> {
    let mut seed = [0u8; 32];
    getrandom::getrandom(&mut seed).map_err(|e| format!("rng: {e}"))?;
    let signing = SigningKey::from_bytes(&seed);
    let pub_hex = to_hex(signing.verifying_key().as_bytes());

    if let Some(parent) = prefix.parent() {
        std::fs::create_dir_all(parent).map_err(|e| format!("keys dir: {e}"))?;
        restrict_dir_permissions(parent)?;
    }
    write_private_key(prefix, to_hex(&seed).as_bytes())?;
    let pub_path = PathBuf::from(format!("{}.pub", prefix.display()));
    std::fs::write(&pub_path, &pub_hex).map_err(|e| format!("write pubkey: {e}"))?;
    Ok(pub_hex)
}

/// Write the private key file. On unix the file is created atomically with mode
/// 0600 (`create_new` + `mode`), so there is never a world-readable window — and
/// a permission failure is surfaced, not swallowed. Refuses to clobber an
/// existing key file.
#[cfg(unix)]
fn write_private_key(path: &Path, bytes: &[u8]) -> Result<(), String> {
    use std::io::Write;
    use std::os::unix::fs::OpenOptionsExt;
    // Remove any stale key file first so create_new can establish a fresh 0600
    // file rather than inheriting a pre-existing (possibly lax) mode. Only a
    // regular file is removed — a directory at this path is left for create_new
    // to reject.
    if path.is_file() {
        std::fs::remove_file(path).map_err(|e| format!("remove stale key: {e}"))?;
    }
    let mut f = std::fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .mode(0o600)
        .open(path)
        .map_err(|e| format!("create key: {e}"))?;
    f.write_all(bytes).map_err(|e| format!("write key: {e}"))?;
    Ok(())
}

#[cfg(not(unix))]
fn write_private_key(path: &Path, bytes: &[u8]) -> Result<(), String> {
    std::fs::write(path, bytes).map_err(|e| format!("write key: {e}"))
}

/// Restrict the keys directory to owner-only (0700) on unix; surface failures.
#[cfg(unix)]
fn restrict_dir_permissions(dir: &Path) -> Result<(), String> {
    use std::os::unix::fs::PermissionsExt;
    std::fs::set_permissions(dir, std::fs::Permissions::from_mode(0o700))
        .map_err(|e| format!("restrict keys dir: {e}"))
}

#[cfg(not(unix))]
fn restrict_dir_permissions(_dir: &Path) -> Result<(), String> {
    Ok(())
}

/// Sign `content` with the private key at `key_path`, returning the signature
/// file. The signed message is the raw content bytes; `sha256` is recorded for
/// diagnostics.
pub fn sign(key_path: &Path, content: &[u8]) -> Result<SignatureFile, String> {
    let key_hex = std::fs::read_to_string(key_path)
        .map_err(|e| format!("read key {}: {e}", key_path.display()))?;
    let seed = from_hex(key_hex.trim())?;
    let seed: [u8; 32] = seed
        .try_into()
        .map_err(|_| "private key must be 32 bytes (64 hex chars)".to_string())?;
    let signing = SigningKey::from_bytes(&seed);
    let sig = signing.sign(content);
    Ok(SignatureFile {
        alg: "ed25519".to_string(),
        pubkey: to_hex(signing.verifying_key().as_bytes()),
        signature: to_hex(&sig.to_bytes()),
        sha256: to_hex(&Sha256::digest(content)),
    })
}

/// Verify a signature file against `content`. If `expected_pubkey` is given, the
/// signature's pubkey must match it (else any self-signed sig would "verify").
pub fn verify(
    sigfile: &SignatureFile,
    content: &[u8],
    expected_pubkey: Option<&str>,
) -> Result<(), String> {
    if sigfile.alg != "ed25519" {
        return Err(format!("unsupported algorithm '{}'", sigfile.alg));
    }
    if let Some(want) = expected_pubkey {
        if !want.eq_ignore_ascii_case(&sigfile.pubkey) {
            return Err("signature is by a different key than --pubkey".to_string());
        }
    }
    let pub_bytes: [u8; 32] = from_hex(&sigfile.pubkey)?
        .try_into()
        .map_err(|_| "pubkey must be 32 bytes".to_string())?;
    let sig_bytes: [u8; 64] = from_hex(&sigfile.signature)?
        .try_into()
        .map_err(|_| "signature must be 64 bytes".to_string())?;
    let vk = VerifyingKey::from_bytes(&pub_bytes).map_err(|e| format!("bad pubkey: {e}"))?;
    let sig = Signature::from_bytes(&sig_bytes);
    vk.verify(content, &sig)
        .map_err(|_| "signature does not verify against the content".to_string())
}

/// Default sig path for a workbook: `<file>.sig`.
pub fn sig_path(file: &str, explicit: Option<&str>) -> PathBuf {
    match explicit {
        Some(p) => PathBuf::from(p),
        None => PathBuf::from(format!("{file}.sig")),
    }
}

pub fn load_sig(path: &Path) -> Result<SignatureFile, String> {
    let bytes = std::fs::read(path).map_err(|e| format!("read {}: {e}", path.display()))?;
    serde_json::from_slice(&bytes).map_err(|e| format!("parse {}: {e}", path.display()))
}

pub fn save_sig(path: &Path, sig: &SignatureFile) -> Result<(), String> {
    let json = serde_json::to_string_pretty(sig).map_err(|e| e.to_string())?;
    std::fs::write(path, json).map_err(|e| format!("write {}: {e}", path.display()))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sign_then_verify_roundtrip() {
        let dir = std::env::temp_dir().join(format!("wb-sign-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let key = dir.join("k");
        let pubhex = keygen(&key).unwrap();

        let content = b"---\nruntime: bash\n---\n```bash\necho hi\n```\n";
        let sig = sign(&key, content).unwrap();
        assert_eq!(sig.pubkey, pubhex);
        // Verifies against the right content + pubkey.
        assert!(verify(&sig, content, Some(&pubhex)).is_ok());
        // Tampered content fails.
        assert!(verify(&sig, b"tampered", Some(&pubhex)).is_err());
        // Wrong pubkey fails.
        assert!(verify(&sig, content, Some("00")).is_err());

        std::fs::remove_dir_all(&dir).ok();
    }

    #[cfg(unix)]
    #[test]
    fn keygen_writes_private_key_mode_0600_and_dir_0700() {
        use std::os::unix::fs::PermissionsExt;
        let dir = tempfile::tempdir().unwrap();
        let keys_dir = dir.path().join("keys");
        let key = keys_dir.join("wb_signing_key");
        keygen(&key).unwrap();

        let key_mode = std::fs::metadata(&key).unwrap().permissions().mode() & 0o777;
        assert_eq!(
            key_mode, 0o600,
            "private key must be 0600, got {key_mode:o}"
        );

        let dir_mode = std::fs::metadata(&keys_dir).unwrap().permissions().mode() & 0o777;
        assert_eq!(dir_mode, 0o700, "keys dir must be 0700, got {dir_mode:o}");

        // Pubkey is fine to be readable; just confirm it was written.
        assert!(keys_dir.join("wb_signing_key.pub").exists());
    }

    #[cfg(unix)]
    #[test]
    fn keygen_replaces_stale_key_with_fresh_0600() {
        use std::os::unix::fs::PermissionsExt;
        let dir = tempfile::tempdir().unwrap();
        let key = dir.path().join("k");
        // Pre-existing world-readable file at the key path.
        std::fs::write(&key, b"old").unwrap();
        std::fs::set_permissions(&key, std::fs::Permissions::from_mode(0o644)).unwrap();

        keygen(&key).unwrap();
        let mode = std::fs::metadata(&key).unwrap().permissions().mode() & 0o777;
        assert_eq!(mode, 0o600, "regenerated key must be 0600, got {mode:o}");
    }

    #[test]
    fn hex_roundtrip() {
        assert_eq!(from_hex(&to_hex(&[0, 255, 16])).unwrap(), vec![0, 255, 16]);
        assert!(from_hex("xyz").is_err());
    }

    #[test]
    fn verify_rejects_unsupported_algorithm() {
        // Covers the `alg != "ed25519"` early-return branch (line 110).
        let dir = tempfile::tempdir().unwrap();
        let key = dir.path().join("k");
        keygen(&key).unwrap();
        let mut sig = sign(&key, b"hello").unwrap();
        sig.alg = "rsa".to_string();
        let err = verify(&sig, b"hello", None).unwrap_err();
        assert!(err.contains("unsupported algorithm"), "got: {err}");
    }

    #[test]
    fn verify_with_wrong_key_fails() {
        // Sign with key A, then swap in key B's pubkey: the signature no longer
        // verifies against the content.
        let dir = tempfile::tempdir().unwrap();
        let key_a = dir.path().join("a");
        let key_b = dir.path().join("b");
        keygen(&key_a).unwrap();
        let pub_b = keygen(&key_b).unwrap();

        let content = b"signed-by-a";
        let mut sig = sign(&key_a, content).unwrap();
        // Tampered pubkey: well-formed (32 bytes) but the wrong key.
        sig.pubkey = pub_b;
        let err = verify(&sig, content, None).unwrap_err();
        assert!(err.contains("does not verify"), "got: {err}");
    }

    #[test]
    fn verify_tampered_signature_bytes_fail() {
        let dir = tempfile::tempdir().unwrap();
        let key = dir.path().join("k");
        let pubhex = keygen(&key).unwrap();
        let content = b"abc";
        let mut sig = sign(&key, content).unwrap();
        // Flip the first hex nibble of the signature so it stays 64 bytes but
        // no longer matches.
        let mut chars: Vec<char> = sig.signature.chars().collect();
        chars[0] = if chars[0] == 'a' { 'b' } else { 'a' };
        sig.signature = chars.into_iter().collect();
        assert!(verify(&sig, content, Some(&pubhex)).is_err());
    }

    #[test]
    fn keygen_fails_when_parent_cannot_be_created() {
        // Covers the create_dir_all error branch (line 66): a regular file sits
        // where the key's parent directory would need to be.
        let dir = tempfile::tempdir().unwrap();
        let blocker = dir.path().join("blocker");
        std::fs::write(&blocker, b"i am a file").unwrap();
        // parent would be `blocker/nested`, but `blocker` is a file.
        let prefix = blocker.join("nested").join("key");
        let err = keygen(&prefix).unwrap_err();
        assert!(err.contains("keys dir"), "got: {err}");
    }

    #[test]
    fn keygen_with_parentless_prefix_skips_create_dir() {
        // A prefix with no parent ("/") takes the `if let Some(parent)` false
        // arm, skipping create_dir_all; creating the key file at root then
        // fails (root is a directory, so create_new can't make a file there).
        let err = keygen(Path::new("/")).unwrap_err();
        assert!(
            err.contains("create key") || err.contains("write key"),
            "got: {err}"
        );
    }

    #[test]
    fn sig_path_explicit_overrides_default() {
        // Explicit path branch (line 132) vs the default `<file>.sig` branch.
        assert_eq!(
            sig_path("deploy.md", Some("custom.sig")),
            PathBuf::from("custom.sig")
        );
        assert_eq!(sig_path("deploy.md", None), PathBuf::from("deploy.md.sig"));
    }

    #[test]
    fn save_then_load_sig_roundtrip() {
        let dir = tempfile::tempdir().unwrap();
        let key = dir.path().join("k");
        keygen(&key).unwrap();
        let sig = sign(&key, b"payload").unwrap();
        let path = dir.path().join("out.sig");
        save_sig(&path, &sig).unwrap();
        let loaded = load_sig(&path).unwrap();
        assert_eq!(loaded.signature, sig.signature);
        assert_eq!(loaded.pubkey, sig.pubkey);
    }
}
