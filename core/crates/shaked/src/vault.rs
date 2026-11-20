//! Secrets at rest (your model keys): AES-256-GCM with this machine's vault key, which lives
//! outside the database: `SHAKEDOWN_VAULT_KEY` (64 hex characters), or the file
//! `SHAKEDOWN_VAULT_KEY_FILE` names, else `<repo>/core/vault.key`, made (mode 0600) the first time
//! it is needed. A shaked that serves others needs one of the two variables, and refuses to start
//! without it. Each sealed value is bound to its row, so a value copied to another row does not
//! open. A lost vault key makes saved keys unreadable: add them again. The format is CopperTide
//! Cloud's (`version ‖ nonce ‖ ciphertext+tag`, version 1).

use std::path::{Path, PathBuf};

use ring::aead::{Aad, LessSafeKey, Nonce, UnboundKey, AES_256_GCM, NONCE_LEN};
use ring::rand::{SecureRandom, SystemRandom};
use uuid::Uuid;

const VERSION: u8 = 1;
const TAG_LEN: usize = 16;

/// The vault context of provider key `id`'s secret.
pub fn key_context(id: Uuid) -> String {
    format!("provider_key:{id}:key")
}

/// Bytes that are zeroed when dropped.
pub struct Zeroed(Vec<u8>);

impl std::ops::Deref for Zeroed {
    type Target = [u8];
    fn deref(&self) -> &[u8] {
        &self.0
    }
}

impl Drop for Zeroed {
    fn drop(&mut self) {
        for b in self.0.iter_mut() {
            // SAFETY: `b` is a valid, aligned, exclusive reference into the vector.
            unsafe { std::ptr::write_volatile(b, 0) };
        }
    }
}

impl std::fmt::Debug for Zeroed {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("Zeroed([redacted])")
    }
}

pub struct Vault {
    key: LessSafeKey,
}

impl std::fmt::Debug for Vault {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("Vault([redacted])")
    }
}

/// 64 hex characters as 32 bytes.
fn decode_hex(h: &str) -> Option<Zeroed> {
    let h = h.trim().as_bytes();
    if h.len() != 64 {
        return None;
    }
    let digit = |c: u8| (c as char).to_digit(16).map(|d| d as u8);
    let bytes: Option<Vec<u8>> = h
        .chunks(2)
        .map(|p| Some(digit(p[0])? << 4 | digit(p[1])?))
        .collect();
    bytes.map(Zeroed)
}

impl Vault {
    /// From 64 hex characters (32 bytes), as `openssl rand -hex 32` makes.
    pub fn from_hex(h: &str) -> Result<Vault, String> {
        let bytes = decode_hex(h).ok_or("the vault key must be 64 hex characters (32 bytes)")?;
        let key = UnboundKey::new(&AES_256_GCM, &bytes)
            .map_err(|_| "the vault key is not a usable key".to_string())?;
        Ok(Vault {
            key: LessSafeKey::new(key),
        })
    }

    /// The vault for a shaked under `policy`, from the environment ([`Vault::from_lookup`]).
    pub fn for_policy(policy: &crate::layer::Policy) -> Result<Vault, String> {
        Vault::from_lookup(|k| parts::settings::var(k).ok(), policy.serves_others)
    }

    /// The vault `get` (one env var) describes: `SHAKEDOWN_VAULT_KEY`, else the file
    /// `SHAKEDOWN_VAULT_KEY_FILE` names, else `<repo>/core/vault.key` (made if missing). A shaked that `serves_others` needs the key named
    /// explicitly: it never falls back to (or makes) the source checkout's `core/vault.key`.
    pub fn from_lookup(
        get: impl Fn(&str) -> Option<String>,
        serves_others: bool,
    ) -> Result<Vault, String> {
        let var = |k: &str| get(k).filter(|v| !v.trim().is_empty());
        if let Some(k) = var("SHAKEDOWN_VAULT_KEY") {
            return Vault::from_hex(&k).map_err(|e| format!("SHAKEDOWN_VAULT_KEY: {e}"));
        }
        match var("SHAKEDOWN_VAULT_KEY_FILE").map(PathBuf::from) {
            Some(file) => Vault::from_file(&file),
            None if serves_others => Err(NO_EXPLICIT_KEY.into()),
            None => Vault::from_file(&default_key_file()),
        }
    }

    /// The vault whose key `file` holds; a missing file is made with a fresh key, readable only
    /// by this user.
    pub fn from_file(file: &Path) -> Result<Vault, String> {
        let shown = file.display();
        if !file.exists() {
            make_key_file(file).map_err(|e| format!("cannot make the vault key {shown}: {e}"))?;
        }
        let text = std::fs::read_to_string(file)
            .map_err(|e| format!("cannot read the vault key {shown}: {e}"))?;
        let text = Zeroed(text.into_bytes());
        let text = std::str::from_utf8(&text).map_err(|_| format!("{shown} is not hex"))?;
        Vault::from_hex(text).map_err(|e| format!("{shown}: {e}"))
    }

    /// `version ‖ nonce ‖ ciphertext+tag`, bound to `context` (e.g. [`key_context`]).
    pub fn seal(&self, context: &str, plain: &[u8]) -> Vec<u8> {
        let mut nonce = [0u8; NONCE_LEN];
        SystemRandom::new()
            .fill(&mut nonce)
            .expect("the system's random source works");
        let mut buf = plain.to_vec();
        self.key
            .seal_in_place_append_tag(
                Nonce::assume_unique_for_key(nonce),
                Aad::from(context.as_bytes()),
                &mut buf,
            )
            .expect("sealing fits in memory");
        let mut out = Vec::with_capacity(1 + NONCE_LEN + buf.len());
        out.push(VERSION);
        out.extend_from_slice(&nonce);
        out.extend_from_slice(&buf);
        drop(Zeroed(buf));
        out
    }

    /// The plain value; `Err` when it was sealed with another key, for another context, or altered.
    pub fn open(&self, context: &str, sealed: &[u8]) -> Result<Zeroed, String> {
        let refused = || {
            "a saved key could not be opened (this machine's vault key changed?): add it again"
                .to_string()
        };
        if sealed.len() < 1 + NONCE_LEN + TAG_LEN || sealed[0] != VERSION {
            return Err(refused());
        }
        let nonce =
            Nonce::try_assume_unique_for_key(&sealed[1..1 + NONCE_LEN]).map_err(|_| refused())?;
        let mut buf = Zeroed(sealed[1 + NONCE_LEN..].to_vec());
        let plain = self
            .key
            .open_in_place(nonce, Aad::from(context.as_bytes()), &mut buf.0)
            .map_err(|_| refused())?;
        Ok(Zeroed(plain.to_vec()))
    }
}

/// Why a shaked that serves others has no vault.
const NO_EXPLICIT_KEY: &str = "a shaked that serves others needs its vault key named: set \
     SHAKEDOWN_VAULT_KEY (64 hex characters) or SHAKEDOWN_VAULT_KEY_FILE; it never uses the source \
     checkout's core/vault.key";

/// `<repo>/core/vault.key` (listed in `.gitignore`).
fn default_key_file() -> PathBuf {
    let core = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..");
    core.canonicalize().unwrap_or(core).join("vault.key")
}

/// Writes 32 fresh random bytes as hex to `file`, mode 0600, refusing to replace one.
fn make_key_file(file: &Path) -> std::io::Result<()> {
    use std::io::Write;
    let mut bytes = Zeroed(vec![0u8; 32]);
    SystemRandom::new()
        .fill(&mut bytes.0)
        .map_err(|_| std::io::Error::other("no system randomness"))?;
    let hex = Zeroed(
        bytes
            .iter()
            .map(|b| format!("{b:02x}"))
            .collect::<String>()
            .into_bytes(),
    );
    let mut opts = std::fs::OpenOptions::new();
    opts.write(true).create_new(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        opts.mode(0o600);
    }
    let mut f = opts.open(file)?;
    f.write_all(&hex)?;
    f.write_all(b"\n")
}

#[cfg(test)]
mod tests {
    use super::*;
    const K: &str = "0001020304050607080910111213141516171819202122232425262728293031";

    #[test]
    fn a_sealed_secret_opens_only_with_its_key_and_context() {
        let v = Vault::from_hex(K).unwrap();
        let sealed = v.seal("provider_key:a:key", b"hunter2-secret");
        assert!(!sealed.windows(6).any(|w| w == b"hunter"));
        assert_eq!(
            &*v.open("provider_key:a:key", &sealed).unwrap(),
            b"hunter2-secret"
        );
        assert!(
            v.open("provider_key:b:key", &sealed).is_err(),
            "bound to its row"
        );
        let other = Vault::from_hex(&K.replace("00", "ff")).unwrap();
        assert!(other.open("provider_key:a:key", &sealed).is_err());
        let mut altered = sealed.clone();
        *altered.last_mut().unwrap() ^= 1;
        assert!(v.open("provider_key:a:key", &altered).is_err());
        assert_ne!(
            v.seal("c", b"x"),
            v.seal("c", b"x"),
            "a fresh nonce each time"
        );
    }

    #[test]
    fn the_key_must_be_32_bytes_of_hex_and_debug_never_prints_it() {
        assert!(Vault::from_hex("abc").is_err());
        assert!(Vault::from_hex(&"zz".repeat(32)).is_err());
        assert!(Vault::from_hex(&"ab".repeat(16)).is_err());
        assert_eq!(
            format!("{:?}", Vault::from_hex(K).unwrap()),
            "Vault([redacted])"
        );
    }

    #[test]
    fn a_shaked_that_serves_others_needs_an_explicit_vault_key() {
        let hex = "00".repeat(32);
        let none = |_: &str| None;
        let err = Vault::from_lookup(none, true).err().unwrap_or_default();
        assert!(err.contains("SHAKEDOWN_VAULT_KEY_FILE"), "{err}");
        let given = |k: &str| (k == "SHAKEDOWN_VAULT_KEY").then(|| hex.clone());
        assert!(Vault::from_lookup(given, true).is_ok());
        let dir = std::env::temp_dir().join(format!("vault-explicit-{}", Uuid::new_v4().simple()));
        std::fs::create_dir_all(&dir).unwrap();
        let file = dir.join("vault.key");
        std::fs::write(&file, &hex).unwrap();
        let named = |k: &str| (k == "SHAKEDOWN_VAULT_KEY_FILE").then(|| file.display().to_string());
        assert!(Vault::from_lookup(named, true).is_ok());
        std::fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn a_missing_key_file_is_made_private_and_then_reused() {
        let dir = std::env::temp_dir().join(format!("shaked-vault-{}", Uuid::new_v4().simple()));
        std::fs::create_dir_all(&dir).unwrap();
        let file = dir.join("vault.key");
        let first = Vault::from_file(&file).unwrap();
        let sealed = first.seal("c", b"x");
        let again = Vault::from_file(&file).unwrap();
        assert_eq!(
            &*again.open("c", &sealed).unwrap(),
            b"x",
            "the same key the second time"
        );
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let mode = std::fs::metadata(&file).unwrap().permissions().mode();
            assert_eq!(mode & 0o777, 0o600);
        }
        std::fs::remove_dir_all(&dir).unwrap();
    }
}
