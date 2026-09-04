use alloy::primitives::Address;
use alloy::signers::local::PrivateKeySigner;
use anyhow::{anyhow, Result};
use argon2::{Algorithm, Argon2, Params, Version};
use chacha20poly1305::{
    aead::{Aead, KeyInit},
    Key, XChaCha20Poly1305, XNonce,
};
use dotenvy::dotenv_override;
use rand::{rngs::OsRng, RngCore};
use rpassword::prompt_password;
use serde::{Deserialize, Serialize};
use std::{env, fs, path::PathBuf, str::FromStr};
use zeroize::Zeroizing;

use crate::utils::home_dir; // <— fixed module path

const APP_DIR: &str = ".x402wallet";
const KEYSTORE: &str = "keystore.json";

/// Argon2id parameters for NEW keystores: RFC 9106 §4 recommendation for
/// high-value key material when large memory is unavailable (m = 64 MiB,
/// t = 3, p = 4). `Argon2::default()` (19 MiB / t=2 / p=1) is only the
/// OWASP minimum — too weak for key material that directly controls funds.
const KDF_M_COST: u32 = 65536; // KiB (= 64 MiB)
const KDF_T_COST: u32 = 3;
const KDF_P_COST: u32 = 4;

/// Legacy defaults: what `Argon2::default()` produced before this hardening.
/// Pre-hardening keystore files carry no parameter fields and must keep
/// decrypting with exactly these values.
const LEGACY_M_COST: u32 = 19456; // KiB (= 19 MiB)
const LEGACY_T_COST: u32 = 2;
const LEGACY_P_COST: u32 = 1;

#[derive(Serialize, Deserialize)]
struct FileKeystore {
    salt: String,
    nonce: String,
    ct: String,
    /// Argon2 parameters this file was encrypted with. Absent in
    /// pre-hardening files → serde defaults to the legacy values.
    #[serde(default = "legacy_m_cost")]
    m_cost: u32,
    #[serde(default = "legacy_t_cost")]
    t_cost: u32,
    #[serde(default = "legacy_p_cost")]
    p_cost: u32,
}

fn legacy_m_cost() -> u32 {
    LEGACY_M_COST
}
fn legacy_t_cost() -> u32 {
    LEGACY_T_COST
}
fn legacy_p_cost() -> u32 {
    LEGACY_P_COST
}

/// Derives the 32-byte encryption key from the passphrase via Argon2id
/// with explicitly given cost parameters.
fn derive_key(pass: &[u8], salt: &[u8], m_cost: u32, t_cost: u32, p_cost: u32) -> Result<[u8; 32]> {
    let params = Params::new(m_cost, t_cost, p_cost, Some(32))
        .map_err(|e| anyhow!("argon2 params: {}", e))?;
    let argon = Argon2::new(Algorithm::Argon2id, Version::V0x13, params);
    let mut key = [0u8; 32];
    argon
        .hash_password_into(pass, salt, &mut key)
        .map_err(|e| anyhow!("argon2: {}", e))?;
    Ok(key)
}

/// Writes a file containing secret material with owner-only permissions:
/// 0600 on Unix (set at creation AND repaired for pre-existing files) and a
/// best-effort ACL restriction to the current user on Windows.
fn write_private(path: &std::path::Path, content: &[u8]) -> Result<()> {
    #[cfg(unix)]
    {
        use std::io::Write as _;
        use std::os::unix::fs::{OpenOptionsExt, PermissionsExt};
        let mut f = fs::OpenOptions::new()
            .create(true)
            .write(true)
            .truncate(true)
            .mode(0o600)
            .open(path)?;
        f.write_all(content)?;
        let mut perms = fs::metadata(path)?.permissions();
        perms.set_mode(0o600);
        fs::set_permissions(path, perms)?;
    }
    #[cfg(not(unix))]
    {
        fs::write(path, content)?;
        // std has no Windows ACL support — restrict the file to the
        // current user via icacls (best effort)
        if let Ok(user) = env::var("USERNAME") {
            let ok = std::process::Command::new("icacls")
                .arg(path)
                .args(["/inheritance:r", "/grant:r"])
                .arg(format!("{user}:F"))
                .output()
                .map(|o| o.status.success())
                .unwrap_or(false);
            if !ok {
                eprintln!(
                    "note: could not restrict file permissions via icacls for {}",
                    path.display()
                );
            }
        }
    }
    Ok(())
}

pub struct WalletContext {
    pub wallet: PrivateKeySigner,
    pub address: Address,
}

fn app_path() -> Result<PathBuf> {
    let mut p = home_dir()?;
    p.push(APP_DIR);
    fs::create_dir_all(&p)?;
    Ok(p)
}

pub async fn init_wallet(dotenv_path: Option<PathBuf>, keystore: bool) -> Result<()> {
    let create_new = prompt("Create new private key (y/N)? ")?;
    let pk_hex = if create_new.to_lowercase().starts_with('y') {
        let wallet = PrivateKeySigner::random();
        let secret_bytes = wallet.to_bytes();
        format!("0x{}", hex::encode(secret_bytes))
    } else {
        println!("Paste 0x-prefixed 32-byte hex private key (input hidden):");
        let pasted = prompt_password("private key: ")?;
        normalize_pk(&pasted)?
    };

    if !keystore {
        let path = dotenv_path.unwrap_or_else(|| PathBuf::from(".env"));
        // Read existing content: keep unrelated variables, drop old key
        // lines, then rewrite the file. Appending instead would leave
        // replaced plaintext keys on disk (dotenv takes last-line-wins,
        // so a stale key would linger unread but readable forever).
        let existing = fs::read_to_string(&path).unwrap_or_default();
        let had_key = existing
            .lines()
            .any(|l| l.trim_start().starts_with("X402_WALLET_PRIVATE_KEY"));
        if had_key {
            eprintln!(
                "WARNING: replacing X402_WALLET_PRIVATE_KEY in {} — the old key is removed \
                 from this file. If you did not back it up, funds on it are now inaccessible.",
                path.display()
            );
        }
        let mut content = String::new();
        for line in existing.lines() {
            if line.trim_start().starts_with("X402_WALLET_PRIVATE_KEY") {
                continue;
            }
            content.push_str(line);
            content.push('\n');
        }
        content.push_str("X402_WALLET_PRIVATE_KEY=\"");
        content.push_str(&pk_hex);
        content.push_str("\"\n");

        write_private(&path, content.as_bytes())?;
        println!("Private key stored in {}", path.display());
    } else {
        let pass = Zeroizing::new(prompt_password("Set keystore passphrase: ")?);
        let pass_confirm = Zeroizing::new(prompt_password("Confirm passphrase: ")?);
        if *pass != *pass_confirm {
            return Err(anyhow!("passphrases do not match"));
        }
        let mut salt = [0u8; 16];
        OsRng.fill_bytes(&mut salt);
        let key_bytes = Zeroizing::new(derive_key(pass.as_bytes(), &salt, KDF_M_COST, KDF_T_COST, KDF_P_COST)?);
        let cipher = XChaCha20Poly1305::new(Key::from_slice(key_bytes.as_ref()));
        let mut nonce = [0u8; 24];
        OsRng.fill_bytes(&mut nonce);
        let ct = cipher.encrypt(XNonce::from_slice(&nonce), pk_hex.as_bytes())?;
        let ks = FileKeystore {
            salt: hex::encode(salt),
            nonce: hex::encode(nonce),
            ct: hex::encode(ct),
            m_cost: KDF_M_COST,
            t_cost: KDF_T_COST,
            p_cost: KDF_P_COST,
        };
        let mut path = app_path()?;
        path.push(KEYSTORE);
        write_private(&path, &serde_json::to_vec_pretty(&ks)?)?;
    }

    let wallet = PrivateKeySigner::from_str(&pk_hex)?;
    println!("wallet address: {:#x}", wallet.address());
    Ok(())
}

fn prompt(s: &str) -> Result<String> {
    use std::io::Write;
    print!("{s}");
    std::io::stdout().flush()?;
    let mut buf = String::new();
    std::io::stdin().read_line(&mut buf)?;
    Ok(buf.trim().to_string())
}

fn normalize_pk(input: &str) -> Result<String> {
    let mut s = input.trim().trim_matches('"').to_string();
    if s.starts_with("0x") || s.starts_with("0X") {
        s.make_ascii_lowercase();
        if s.len() != 66 {
            return Err(anyhow!("expected 32-byte hex (66 chars incl 0x)"));
        }
        Ok(s)
    } else {
        if s.len() != 64 {
            return Err(anyhow!("expected 32-byte hex (64 chars)"));
        }
        Ok(format!("0x{}", s.to_lowercase()))
    }
}

async fn load_private_key_hex() -> Result<String> {
    let _ = dotenv_override();
    if let Ok(m) = env::var("X402_WALLET_PRIVATE_KEY") {
        return normalize_pk(&m);
    }
    let mut path = app_path()?;
    path.push(KEYSTORE);
    if path.exists() {
        let data = fs::read(path)?;
        let ks: FileKeystore = serde_json::from_slice(&data)?;
        let pass = Zeroizing::new(if let Ok(p) = env::var("X402_KEYSTORE_PASSWORD") {
            p
        } else {
            prompt_password("Unlock keystore passphrase: ")?
        });
        // Use the parameters recorded in the file — pre-hardening files
        // deserialize with the legacy defaults, so they keep decrypting
        let key_bytes = Zeroizing::new(derive_key(
            pass.as_bytes(),
            &hex::decode(ks.salt)?,
            ks.m_cost,
            ks.t_cost,
            ks.p_cost,
        )?);
        let cipher = XChaCha20Poly1305::new(Key::from_slice(key_bytes.as_ref()));
        let pt = cipher.decrypt(
            XNonce::from_slice(&hex::decode(ks.nonce)?),
            &hex::decode(ks.ct)?[..],
        )?;
        // Convert to string and wrap in Zeroizing, zeroizing the original Vec as well
        let s = Zeroizing::new(String::from_utf8(pt)?);
        return normalize_pk(&s);
    }
    Err(anyhow!(
        "No private key. Use `x402-wallet wallet-init` or set X402_WALLET_PRIVATE_KEY in .env"
    ))
}

pub async fn load_wallet_context() -> Result<WalletContext> {
    let pk = load_private_key_hex().await?;
    let wallet = PrivateKeySigner::from_str(&pk)?;
    Ok(WalletContext {
        address: wallet.address(),
        wallet,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn legacy_keystore_without_params_deserializes_to_legacy_kdf() {
        // Pre-hardening files only carry salt/nonce/ct — the parameter
        // fields must default to the legacy Argon2::default() values so
        // those files keep decrypting.
        let ks: FileKeystore =
            serde_json::from_str(r#"{"salt":"00","nonce":"00","ct":"00"}"#).unwrap();
        assert_eq!((ks.m_cost, ks.t_cost, ks.p_cost), (19456, 2, 1));
    }

    #[test]
    fn kdf_params_change_the_derived_key() {
        // Proves the recorded parameters actually flow into the derivation;
        // if they didn't, a param mismatch would silently "work" (wrong key
        // would surface only as an opaque AEAD failure at decrypt time).
        let a = derive_key(b"pass", b"0123456789abcdef", 64, 1, 1).unwrap();
        let b = derive_key(b"pass", b"0123456789abcdef", 64, 2, 1).unwrap();
        let c = derive_key(b"pass", b"0123456789abcdef", 64, 1, 2).unwrap();
        assert_ne!(a, b);
        assert_ne!(a, c);
    }

    #[test]
    fn rfc9106_params_derive_deterministically() {
        let a = derive_key(
            b"pass",
            b"0123456789abcdef",
            KDF_M_COST,
            KDF_T_COST,
            KDF_P_COST,
        )
        .unwrap();
        let b = derive_key(
            b"pass",
            b"0123456789abcdef",
            KDF_M_COST,
            KDF_T_COST,
            KDF_P_COST,
        )
        .unwrap();
        assert_eq!(a, b);
    }

    #[test]
    fn new_keystore_format_roundtrips_params() {
        let ks = FileKeystore {
            salt: "00".into(),
            nonce: "00".into(),
            ct: "00".into(),
            m_cost: KDF_M_COST,
            t_cost: KDF_T_COST,
            p_cost: KDF_P_COST,
        };
        let json = serde_json::to_string(&ks).unwrap();
        let back: FileKeystore = serde_json::from_str(&json).unwrap();
        assert_eq!((back.m_cost, back.t_cost, back.p_cost), (65536, 3, 4));
    }

    /// Encrypts like init_wallet does, with explicit KDF parameters
    fn seal(plain: &[u8], pass: &[u8], m: u32, t: u32, p: u32) -> FileKeystore {
        let salt = [7u8; 16];
        let nonce = [9u8; 24];
        let key = derive_key(pass, &salt, m, t, p).unwrap();
        let cipher = XChaCha20Poly1305::new(Key::from_slice(&key));
        let ct = cipher.encrypt(XNonce::from_slice(&nonce), plain).unwrap();
        FileKeystore {
            salt: hex::encode(salt),
            nonce: hex::encode(nonce),
            ct: hex::encode(ct),
            m_cost: m,
            t_cost: t,
            p_cost: p,
        }
    }

    /// Decrypts like load_private_key_hex does: with the parameters
    /// recorded in the file (after JSON roundtrip)
    fn unseal(ks: &FileKeystore, pass: &[u8]) -> Result<Vec<u8>> {
        let re: FileKeystore = serde_json::from_str(&serde_json::to_string(ks).unwrap()).unwrap();
        let key = derive_key(
            pass,
            &hex::decode(re.salt)?,
            re.m_cost,
            re.t_cost,
            re.p_cost,
        )?;
        let cipher = XChaCha20Poly1305::new(Key::from_slice(&key));
        Ok(cipher.decrypt(
            XNonce::from_slice(&hex::decode(re.nonce)?),
            &hex::decode(re.ct)?[..],
        )?)
    }

    #[test]
    fn keystore_roundtrip_with_rfc9106_params() {
        let ks = seal(b"0xdeadbeef", b"pw", KDF_M_COST, KDF_T_COST, KDF_P_COST);
        assert_eq!(unseal(&ks, b"pw").unwrap(), b"0xdeadbeef");
    }

    #[test]
    fn pre_hardening_file_without_params_still_decrypts() {
        // Simulates a pre-hardening keystore: encrypted with the legacy
        // Argon2::default() parameters, parameter fields absent from JSON
        let ks = seal(
            b"0xlegacy",
            b"pw",
            LEGACY_M_COST,
            LEGACY_T_COST,
            LEGACY_P_COST,
        );
        let mut v = serde_json::to_value(&ks).unwrap();
        let obj = v.as_object_mut().unwrap();
        obj.remove("m_cost");
        obj.remove("t_cost");
        obj.remove("p_cost");
        let stripped: FileKeystore = serde_json::from_value(v).unwrap();
        assert_eq!(
            (stripped.m_cost, stripped.t_cost, stripped.p_cost),
            (19456, 2, 1)
        );
        assert_eq!(unseal(&stripped, b"pw").unwrap(), b"0xlegacy");
    }

    #[test]
    fn new_params_file_stripped_of_params_must_not_decrypt() {
        // A file sealed with RFC 9106 params but treated as legacy (fields
        // stripped) derives the WRONG key → AEAD failure, not silent success
        let ks = seal(b"0xnew", b"pw", KDF_M_COST, KDF_T_COST, KDF_P_COST);
        let mut v = serde_json::to_value(&ks).unwrap();
        let obj = v.as_object_mut().unwrap();
        obj.remove("m_cost");
        obj.remove("t_cost");
        obj.remove("p_cost");
        let stripped: FileKeystore = serde_json::from_value(v).unwrap();
        assert!(unseal(&stripped, b"pw").is_err());
    }
}
