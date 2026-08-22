use alloy::primitives::Address;
use alloy::signers::local::PrivateKeySigner;
use anyhow::{anyhow, Result};
use chacha20poly1305::{
    aead::{Aead, KeyInit},
    XChaCha20Poly1305, Key, XNonce,
};
use dotenvy::dotenv_override;
use rand::{rngs::OsRng, RngCore};
use rpassword::prompt_password;
use serde::{Deserialize, Serialize};
use std::{env, fs, path::PathBuf, str::FromStr};
use zeroize::Zeroize;

use crate::utils::home_dir; // <— fixed module path

const APP_DIR: &str = ".x402wallet";
const KEYSTORE: &str = "keystore.json";

#[derive(Serialize, Deserialize)]
struct FileKeystore { salt: String, nonce: String, ct: String }

pub struct WalletContext { pub wallet: PrivateKeySigner, pub address: Address }

fn app_path() -> Result<PathBuf> {
    let mut p = home_dir()?; p.push(APP_DIR);
    fs::create_dir_all(&p)?; Ok(p)
}

pub async fn init_wallet(dotenv_path: Option<PathBuf>, keystore: bool) -> Result<()> {
    let create_new = prompt("Create new private key (y/N)? ")?;
    let pk_hex = if create_new.to_lowercase().starts_with('y') {
        let wallet = PrivateKeySigner::random();
        let secret_bytes = wallet.to_bytes();
        format!("0x{}", hex::encode(secret_bytes))
    } else {
        println!("Paste 0x-prefixed 32-byte hex private key (input hidden):");
        let pasted = prompt_password("private key: ")?; normalize_pk(&pasted)?
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

        #[cfg(unix)]
        {
            use std::io::Write as _;
            use std::os::unix::fs::{OpenOptionsExt, PermissionsExt};
            // mode(0o600) applies at creation (no world-readable window);
            // set_permissions also repairs pre-existing files with looser modes
            let mut f = fs::OpenOptions::new()
                .create(true)
                .write(true)
                .truncate(true)
                .mode(0o600)
                .open(&path)?;
            f.write_all(content.as_bytes())?;
            let mut perms = fs::metadata(&path)?.permissions();
            perms.set_mode(0o600);
            fs::set_permissions(&path, perms)?;
        }
        #[cfg(not(unix))]
        {
            fs::write(&path, &content)?;
            // std has no Windows ACL support — restrict the file to the
            // current user via icacls (best effort)
            if let Ok(user) = env::var("USERNAME") {
                let ok = std::process::Command::new("icacls")
                    .arg(&path)
                    .args(["/inheritance:r", "/grant:r"])
                    .arg(format!("{user}:F"))
                    .output()
                    .map(|o| o.status.success())
                    .unwrap_or(false);
                if !ok {
                    eprintln!("note: could not restrict .env permissions via icacls");
                }
            }
        }
        println!("Private key stored in {}", path.display());
    } else {
        let pass = prompt_password("Set keystore passphrase: ")?;
        let pass_confirm = prompt_password("Confirm passphrase: ")?;
        if pass != pass_confirm { return Err(anyhow!("passphrases do not match")); }
        let mut salt = [0u8; 16]; OsRng.fill_bytes(&mut salt);
        let mut key_bytes = [0u8; 32];
        argon2::Argon2::default()
            .hash_password_into(pass.as_bytes(), &salt, &mut key_bytes)
            .map_err(|e| anyhow!("argon2: {}", e))?; // <— map_err
        let cipher = XChaCha20Poly1305::new(Key::from_slice(&key_bytes));
        let mut nonce = [0u8; 24]; OsRng.fill_bytes(&mut nonce);
        let ct = cipher.encrypt(XNonce::from_slice(&nonce), pk_hex.as_bytes())?;
        let ks = FileKeystore { salt: hex::encode(salt), nonce: hex::encode(nonce), ct: hex::encode(ct) };
        let mut path = app_path()?; path.push(KEYSTORE);
        fs::write(path, serde_json::to_vec_pretty(&ks)?)?;
        let mut pass = pass; pass.zeroize();
    }

    let wallet = PrivateKeySigner::from_str(&pk_hex)?;
    println!("wallet address: {:#x}", wallet.address());
    Ok(())
}

fn prompt(s: &str) -> Result<String> {
    use std::io::{Write};
    print!("{s}"); std::io::stdout().flush()?;
    let mut buf = String::new(); std::io::stdin().read_line(&mut buf)?;
    Ok(buf.trim().to_string())
}

fn normalize_pk(input: &str) -> Result<String> {
    let mut s = input.trim().trim_matches('"').to_string();
    if s.starts_with("0x") || s.starts_with("0X") {
        s.make_ascii_lowercase();
        if s.len() != 66 { return Err(anyhow!("expected 32-byte hex (66 chars incl 0x)")); }
        Ok(s)
    } else {
        if s.len() != 64 { return Err(anyhow!("expected 32-byte hex (64 chars)")); }
        Ok(format!("0x{}", s.to_lowercase()))
    }
}

async fn load_private_key_hex() -> Result<String> {
    let _ = dotenv_override();
    if let Ok(m) = env::var("X402_WALLET_PRIVATE_KEY") { return normalize_pk(&m); }
    let mut path = app_path()?; path.push(KEYSTORE);
    if path.exists() {
        let data = fs::read(path)?;
        let ks: FileKeystore = serde_json::from_slice(&data)?;
        let pass = prompt_password("Unlock keystore passphrase: ")?;
        let mut key_bytes = [0u8; 32];
        argon2::Argon2::default()
            .hash_password_into(pass.as_bytes(), &hex::decode(ks.salt)?, &mut key_bytes)
            .map_err(|e| anyhow!("argon2: {}", e))?; // <— map_err
        let cipher = XChaCha20Poly1305::new(Key::from_slice(&key_bytes));
        let pt = cipher.decrypt(XNonce::from_slice(&hex::decode(ks.nonce)?), &hex::decode(ks.ct)?[..])?;
        let mut pass = pass; pass.zeroize();
        let s = String::from_utf8(pt)?;
        return normalize_pk(&s);
    }
    Err(anyhow!("No private key. Use `x402-wallet wallet-init` or set X402_WALLET_PRIVATE_KEY in .env"))
}

pub async fn load_wallet_context() -> Result<WalletContext> {
    let pk = load_private_key_hex().await?;
    let wallet = PrivateKeySigner::from_str(&pk)?;
    Ok(WalletContext { address: wallet.address(), wallet })
}