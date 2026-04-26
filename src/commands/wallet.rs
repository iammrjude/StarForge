use crate::utils::{config, crypto, hardware_wallet, horizon, multisig, print as p};
use anyhow::Result;
use chrono::Utc;
use clap::Subcommand;
use colored::*;
use ed25519_dalek::{Signer, SigningKey};
use rand::RngCore;
use stellar_strkey::ed25519::{PrivateKey as StellarPrivateKey, PublicKey as StellarPublicKey};
use std::path::PathBuf;

#[derive(Subcommand)]
pub enum WalletCommands {
    /// Create a new keypair and save it locally
    Create {
        /// A friendly name for the wallet (e.g. "alice", "deployer")
        name: String,
        /// Fund the wallet via Friendbot immediately (testnet only)
        #[arg(long, default_value = "false")]
        fund: bool,
        /// Network to associate with this wallet (overrides global config)
        #[arg(long, value_parser = ["testnet", "mainnet"])]
        network: Option<String>,
        /// Encrypt the secret key with a passphrase at rest
        #[arg(long, default_value = "false")]
        encrypt: bool,
    },
    /// List all saved wallets
    List,
    /// Show details of a saved wallet including live balance
    Show {
        /// Wallet name
        name: String,
        /// Reveal the secret key in plaintext
        #[arg(long, default_value = "false")]
        reveal: bool,
    },
    /// Fund a wallet via Friendbot (testnet only)
    Fund {
        /// Wallet name to fund
        name: String,
    },
    /// Remove a wallet from local storage
    Remove {
        /// Wallet name to remove
        name: String,
    },
    /// Rename a wallet
    Rename {
        old_name: String,
        new_name: String,
    },

    /// Connect to a hardware wallet (Ledger/Trezor)
    Connect {
        #[arg(value_enum)]
        device: hardware_wallet::HardwareWalletKind,
    },

    /// Sign an arbitrary message using a local or hardware-backed key
    Sign {
        /// Wallet name to use (for local signing)
        name: String,
        /// Message to sign (utf-8)
        message: String,
        /// Use a hardware wallet instead of a local secret key
        #[arg(long, value_enum)]
        hardware: Option<hardware_wallet::HardwareWalletKind>,
    },
    /// Multi-signature account management
    #[command(subcommand)]
    Multisig(MultisigCommands),
}

#[derive(Subcommand)]
pub enum MultisigCommands {
    /// Create a multi-sig config for an existing wallet
    ///
    /// Example:
    /// starforge wallet multisig create treasury --threshold 2 --signers alice,bob,charlie
    Create {
        /// Wallet name to treat as the multi-sig account (e.g. "treasury")
        name: String,
        /// Required weight threshold to submit
        #[arg(long)]
        threshold: u8,
        /// Comma-separated wallet names to act as signers (e.g. alice,bob,charlie)
        #[arg(long)]
        signers: String,
        /// Override network for this config
        #[arg(long)]
        network: Option<String>,
    },
    /// Sign a multi-sig transaction JSON with all available local signer keys
    ///
    /// Example:
    /// starforge wallet multisig sign treasury --transaction tx.json
    Sign {
        /// Multi-sig account name (created via `multisig create`)
        name: String,
        /// Path to a MultiSigTransaction JSON file
        #[arg(long)]
        transaction: PathBuf,
        /// Output file (defaults to in-place update)
        #[arg(long)]
        output: Option<PathBuf>,
    },
    /// List multi-sig accounts stored locally
    List,
    /// Show a stored multi-sig account
    Show { name: String },
}

pub fn handle(cmd: WalletCommands) -> Result<()> {
    match cmd {
        WalletCommands::Create { name, fund, network, encrypt } => create(name, fund, network, encrypt),
        WalletCommands::List                  => list(),
        WalletCommands::Show { name, reveal } => show(name, reveal),
        WalletCommands::Fund { name } => fund_wallet(name),
        WalletCommands::Remove { name } => remove(name),
        WalletCommands::Rename { old_name, new_name } => rename(old_name, new_name),
        WalletCommands::Connect { device } => connect_hardware(device),
        WalletCommands::Sign { name, message, hardware } => sign_message(name, message, hardware),
        WalletCommands::Multisig(cmd) => handle_multisig(cmd),
    }
}

fn connect_hardware(device: hardware_wallet::HardwareWalletKind) -> Result<()> {
    p::header("Hardware Wallet");
    p::step(1, 2, &format!("Connecting to {:?}…", device));
    hardware_wallet::connect(device)?;
    p::step(2, 2, "Device detected");
    println!();
    p::success(&format!("{:?} connected (device detection only).", device));
    Ok(())
}

fn sign_message(
    name: String,
    message: String,
    hardware: Option<hardware_wallet::HardwareWalletKind>,
) -> Result<()> {
    p::header("Sign Message");
    p::kv("Wallet", &name);

    let msg_bytes = message.as_bytes();

    if let Some(kind) = hardware {
        p::kv("Signer", &format!("{:?}", kind));
        let sig = hardware_wallet::sign(kind, msg_bytes)?;
        p::separator();
        p::kv_accent("Message", &message);
        p::kv("Signature (hex)", &hex::encode(sig));
        p::separator();
        return Ok(());
    }

    let cfg = config::load()?;
    let w = cfg
        .wallets
        .iter()
        .find(|w| w.name == name)
        .ok_or_else(|| anyhow::anyhow!("Wallet '{}' not found", name))?;

    let sk = w
        .secret_key
        .as_ref()
        .ok_or_else(|| anyhow::anyhow!("Wallet '{}' has no local secret key", name))?;

    let plain_sk = if !sk.contains(':') && sk.starts_with('S') && sk.len() == 56 {
        sk.clone()
    } else {
        let pwd = crypto::prompt_password(&format!("Enter password for wallet '{}'", name), false)?;
        crypto::decrypt_secret(&pwd, sk).map_err(|_| anyhow::anyhow!("Incorrect password or unable to decrypt."))?
    };

    let decoded_secret = StellarPrivateKey::from_string(&plain_sk)?;
    let signing_key = SigningKey::from_bytes(&decoded_secret.0);
    let sig = signing_key.sign(msg_bytes);

    p::separator();
    p::kv_accent("Message", &message);
    p::kv("Signature (hex)", &hex::encode(sig.to_bytes()));
    p::separator();
    Ok(())
}

fn generate_keypair() -> (String, String) {
    let mut rng = rand::thread_rng();
    let mut seed = [0u8; 32];
    rng.fill_bytes(&mut seed);

    let signing_key = SigningKey::from_bytes(&seed);
    let verifying_key = signing_key.verifying_key();

    let public_key = StellarPublicKey(verifying_key.to_bytes()).to_string();
    let secret_key = StellarPrivateKey(seed).to_string();

    (public_key, secret_key)
}

fn create(name: String, fund: bool, network_override: Option<String>, encrypt: bool) -> Result<()> {
    let mut cfg = config::load()?;

    config::validate_wallet_name(&name)?;

    if cfg.wallets.iter().any(|w| w.name == name) {
        anyhow::bail!("A wallet named '{}' already exists.", name);
    }

    let network = network_override.unwrap_or_else(|| cfg.network.clone());

    let steps = if fund { 3 } else { 2 };
    p::header(&format!("Creating wallet '{}'", name));

    p::step(1, steps, "Generating keypair…");
    let (public_key, secret_key) = generate_keypair();
    println!();
    p::kv_accent("Public Key", &public_key);

    println!();
    let secret_to_store = if encrypt {
        let pwd = crypto::prompt_password("Set a secure passphrase to encrypt this wallet", true)?;
        crypto::encrypt_secret(&pwd, &secret_key)?
    } else {
        secret_key.clone()
    };

    let status = if encrypt { "Encrypted and safely stored." } else { "Stored in plaintext (not recommended for mainnet)." };
    p::kv("Secret Key", status);
    println!();

    p::step(2, steps, "Saving to ~/.starforge/config.toml…");
    let wallet = config::WalletEntry {
        name: name.clone(),
        public_key: public_key.clone(),
        secret_key: Some(secret_to_store),
        network: network.clone(),
        created_at: Utc::now().to_rfc3339(),
        funded: false,
    };
    cfg.wallets.push(wallet);

    if fund {
        if network == "mainnet" {
            p::warn("Friendbot is not available on Mainnet. Skipping fund step.");
        } else {
            p::step(3, steps, "Funding via Friendbot…");
            match horizon::fund_account(&public_key) {
                Ok(_) => {
                    if let Some(w) = cfg.wallets.iter_mut().find(|w| w.name == name) {
                        w.funded = true;
                    }
                    p::success("Funded with 10,000 XLM on testnet");
                }
                Err(e) => p::warn(&format!("Funding failed: {}", e)),
            }
        }
    }

    config::save(&cfg)?;
    println!();
    p::success(&format!("Wallet '{}' created and saved!", name));
    p::info(&format!(
        "View it with: {}",
        format!("starforge wallet show {}", name).cyan()
    ));
    Ok(())
}

fn list() -> Result<()> {
    let cfg = config::load()?;

    p::header("Saved Wallets");

    if cfg.wallets.is_empty() {
        p::info(&format!(
            "No wallets yet on {}. Run `starforge wallet create <name>` to get started.",
            cfg.network
        ));
        return Ok(());
    }

    p::separator();

    for (i, w) in cfg.wallets.iter().enumerate() {
        let status = if w.funded {
            "funded".green()
        } else {
            "unfunded".dimmed()
        };

        println!("  {:>2}. {} [{}]", i + 1, w.name.bold(), status);
        p::kv("Key", &w.public_key);
        p::kv("Net", &w.network);

        if i < cfg.wallets.len() - 1 {
            println!();
        }
    }

    p::separator();
    p::kv(
        &format!("{} wallet(s)", cfg.wallets.len()),
        &format!("on {} — {}", cfg.network, config::config_path().display()),
    );

    Ok(())
}

fn show(name: String, reveal: bool) -> Result<()> {
    let cfg = config::load()?;
    let w = cfg
        .wallets
        .iter()
        .find(|w| w.name == name)
        .ok_or_else(|| anyhow::anyhow!("Wallet '{}' not found", name))?;

    p::header(&format!("Wallet: {}", w.name));
    p::separator();
    p::kv_accent("Public Key", &w.public_key);

    if reveal {
        if let Some(sk) = &w.secret_key {
            // Check if it's plaintext
            if !sk.contains(':') && sk.starts_with('S') && sk.len() == 56 {
                p::warn("Warning: This wallet is using an unencrypted legacy key!");
                p::kv("Secret Key", sk);
            } else {
                let pwd = crypto::prompt_password(&format!("Enter password for wallet '{}'", name), false)?;
                match crypto::decrypt_secret(&pwd, sk) {
                    Ok(plain_sk) => p::kv("Secret Key", &plain_sk),
                    Err(_) => anyhow::bail!("Incorrect password or unable to decrypt."),
                }
            }
        }
    } else {
        p::kv(
            "Secret Key",
            &format!("{} (--reveal to show)", "*".repeat(20)),
        );
    }

    p::kv("Network", &w.network);
    p::kv("Funded", if w.funded { "yes" } else { "no" });
    p::kv("Created", &w.created_at);
    p::separator();

    p::info(&format!("Fetching live balance on {}…", w.network));
    match horizon::fetch_account(&w.public_key, &w.network) {
        Ok(account) => {
            println!();
            for bal in &account.balances {
                let asset = bal.asset_code.as_deref().unwrap_or("XLM");
                p::kv_accent(asset, &format!("{} {}", bal.balance, asset));
            }
        }
        Err(_) => {
            p::warn("Account not yet active on-chain. Fund it with `starforge wallet fund`");
        }
    }
    Ok(())
}

fn fund_wallet(name: String) -> Result<()> {
    config::validate_wallet_name(&name)?;
    let mut cfg = config::load()?;

    if cfg.network == "mainnet" {
        anyhow::bail!("Friendbot is not available on Mainnet.");
    }

    let public_key = cfg
        .wallets
        .iter()
        .find(|w| w.name == name)
        .map(|w| w.public_key.clone())
        .ok_or_else(|| anyhow::anyhow!("Wallet '{}' not found", name))?;

    p::info(&format!("Funding '{}' via Friendbot…", name));
    horizon::fund_account(&public_key)?;

    if let Some(w) = cfg.wallets.iter_mut().find(|w| w.name == name) {
        w.funded = true;
    }
    config::save(&cfg)?;

    println!();
    p::success("Account funded with 10,000 XLM on testnet!");
    p::kv_accent("Public Key", &public_key);
    Ok(())
}

fn remove(name: String) -> Result<()> {
    config::validate_wallet_name(&name)?;
    let mut cfg = config::load()?;
    let before = cfg.wallets.len();
    cfg.wallets.retain(|w| w.name != name);

    if cfg.wallets.len() == before {
        anyhow::bail!("No wallet named '{}' found", name);
    }

    config::save(&cfg)?;
    p::success(&format!("Wallet '{}' removed", name));
    Ok(())
}
fn rename(old_name: String, new_name: String) -> Result<()> {
    config::validate_wallet_name(&old_name)?;
    config::validate_wallet_name(&new_name)?;
    
    let mut cfg = config::load()?;
    if !cfg.wallets.iter().any(|w| w.name == old_name) {
        anyhow::bail!("No wallet named '{}' found", old_name);
    }

    if cfg.wallets.iter().any(|w| w.name == new_name) {
        anyhow::bail!("A wallet named '{}' already exists", new_name);
    }
    if let Some(w) = cfg.wallets.iter_mut().find(|w| w.name == old_name) {
        w.name = new_name.clone();
    }

    config::save(&cfg)?;
    println!();
    p::success(&format!("Wallet renamed: '{}' → '{}'", old_name, new_name));
    p::info(&format!(
        "View it with: {}",
        format!("starforge wallet show {}", new_name).cyan()
    ));
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::generate_keypair;
    use ed25519_dalek::{Signer, SigningKey, Verifier, VerifyingKey};
    use std::collections::HashSet;
    use stellar_strkey::ed25519::{PrivateKey as StellarPrivateKey, PublicKey as StellarPublicKey};

    #[test]
    fn generates_valid_unique_stellar_ed25519_keypairs() {
        let mut public_keys = HashSet::new();
        let mut secret_keys = HashSet::new();
        let message = b"starforge wallet keypair validation";

        for _ in 0..1000 {
            let (public_key, secret_key) = generate_keypair();

            assert!(public_key.starts_with('G'));
            assert!(secret_key.starts_with('S'));
            assert!(public_keys.insert(public_key.clone()));
            assert!(secret_keys.insert(secret_key.clone()));

            let decoded_public = StellarPublicKey::from_string(&public_key).unwrap();
            let decoded_secret = StellarPrivateKey::from_string(&secret_key).unwrap();

            assert_eq!(decoded_public.to_string(), public_key);
            assert_eq!(decoded_secret.to_string(), secret_key);

            let signing_key = SigningKey::from_bytes(&decoded_secret.0);
            let verifying_key = VerifyingKey::from_bytes(&decoded_public.0).unwrap();

            assert_eq!(signing_key.verifying_key().to_bytes(), decoded_public.0);

            let signature = signing_key.sign(message);
            verifying_key.verify(message, &signature).unwrap();
        }
    }
}

fn handle_multisig(cmd: MultisigCommands) -> Result<()> {
    match cmd {
        MultisigCommands::Create { name, threshold, signers, network } => multisig_create(name, threshold, signers, network),
        MultisigCommands::Sign { name, transaction, output } => multisig_sign(name, transaction, output),
        MultisigCommands::List => multisig_list(),
        MultisigCommands::Show { name } => multisig_show(name),
    }
}

fn multisig_create(name: String, threshold: u8, signers: String, network: Option<String>) -> Result<()> {
    config::validate_wallet_name(&name)?;
    multisig::validate_threshold(threshold)?;

    let cfg = config::load()?;
    let wallet = cfg
        .wallets
        .iter()
        .find(|w| w.name == name)
        .ok_or_else(|| anyhow::anyhow!("Wallet '{}' not found. Create it first with `starforge wallet create {}`", name, name))?;

    let network = network.unwrap_or_else(|| wallet.network.clone());
    config::validate_network(&network)?;

    let signer_names: Vec<String> = signers
        .split(',')
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty())
        .collect();

    if signer_names.is_empty() {
        anyhow::bail!("Provide at least one signer wallet via --signers alice,bob,...");
    }

    let mut signer_entries = Vec::new();
    for signer_name in signer_names {
        config::validate_wallet_name(&signer_name)?;
        let signer_wallet = cfg.wallets.iter().find(|w| w.name == signer_name).ok_or_else(|| {
            anyhow::anyhow!("Signer wallet '{}' not found in local config", signer_name)
        })?;
        signer_entries.push(multisig::Signer {
            public_key: signer_wallet.public_key.clone(),
            weight: 1,
            name: Some(signer_wallet.name.clone()),
        });
    }

    let total_weight = multisig::calculate_total_weight(&signer_entries);
    let thresholds = multisig::Thresholds { low: threshold, medium: threshold, high: threshold };
    multisig::validate_thresholds(&thresholds, total_weight)?;

    let account = multisig::MultiSigAccount {
        name: name.clone(),
        account_id: wallet.public_key.clone(),
        signers: signer_entries,
        thresholds,
        created_at: chrono::Utc::now().to_rfc3339(),
    };

    multisig::save_account(&account)?;

    println!();
    p::header(&format!("Multi-sig: {}", name));
    p::success("Multi-sig config saved");
    p::kv_accent("Account ID", &account.account_id);
    p::kv("Network", &network);
    p::kv("Threshold", &threshold.to_string());
    p::kv("Signers", &account.signers.len().to_string());
    p::info("Sign with: starforge wallet multisig sign <name> --transaction tx.json");
    Ok(())
}

fn multisig_sign(name: String, transaction: PathBuf, output: Option<PathBuf>) -> Result<()> {
    config::validate_wallet_name(&name)?;
    config::validate_file_path(&transaction, Some("json"))?;

    let account = multisig::load_account(&name)?;
    let cfg = config::load()?;

    let mut tx = multisig::load_transaction(&transaction)?;

    p::header(&format!("Multi-sig Sign: {}", name));
    p::kv("Account", &account.account_id);
    p::kv("Transaction", &transaction.display().to_string());

    // Attempt to sign with every configured signer that we have a local secret key for.
    let mut signed = 0u32;
    for s in &account.signers {
        let wallet_name = s.name.clone().unwrap_or_else(|| s.public_key.clone());
        let Some(w) = cfg.wallets.iter().find(|w| w.public_key == s.public_key) else { continue };
        let Some(sk) = &w.secret_key else { continue };

        let plain_sk = if !sk.contains(':') && sk.starts_with('S') && sk.len() == 56 {
            sk.clone()
        } else {
            let pwd = crypto::prompt_password(
                &format!("Enter password for signer wallet '{}'", w.name),
                false,
            )?;
            crypto::decrypt_secret(&pwd, sk)
                .map_err(|_| anyhow::anyhow!("Incorrect password or unable to decrypt."))?
        };

        let sig = multisig::sign_transaction_partial(&tx.transaction_xdr, &plain_sk, "testnet")?;
        if multisig::add_signature_to_transaction(&mut tx, &wallet_name, sig).is_ok() {
            signed += 1;
        }
    }

    tx.threshold_required = account.thresholds.high;
    tx.current_weight = tx.signatures.len().min(u8::MAX as usize) as u8;
    if multisig::check_transaction_ready(&tx) {
        tx.status = multisig::TransactionStatus::ReadyToSubmit;
    }

    let out_path = output.unwrap_or_else(|| transaction.clone());
    multisig::save_transaction(&out_path, &tx)?;

    println!();
    p::success("Signatures updated");
    p::kv("Signatures added", &signed.to_string());
    p::kv("Total signatures", &tx.signatures.len().to_string());
    p::kv("Output", &out_path.display().to_string());

    if tx.status == multisig::TransactionStatus::ReadyToSubmit {
        p::info("Transaction meets threshold and is ready to submit.");
    } else {
        p::warn("Transaction does not yet meet threshold.");
    }

    Ok(())
}

fn multisig_list() -> Result<()> {
    p::header("Multi-Signature Accounts");
    let accounts = multisig::list_accounts().unwrap_or_default();
    if accounts.is_empty() {
        p::info("No multi-sig accounts found. Create one with: starforge wallet multisig create");
        return Ok(());
    }

    p::separator();
    for (i, acct) in accounts.iter().enumerate() {
        println!("  {:>2}. {}", i + 1, acct.name.bold());
        p::kv("Account ID", &acct.account_id);
        p::kv("Signers", &acct.signers.len().to_string());
        p::kv("Threshold", &acct.thresholds.high.to_string());
        if i < accounts.len() - 1 {
            println!();
        }
    }
    p::separator();
    Ok(())
}

fn multisig_show(name: String) -> Result<()> {
    let multisig_account = multisig::load_account(&name)?;

    p::header(&format!("Multi-Sig Account: {}", name));
    p::separator();
    p::kv_accent("Account ID", &multisig_account.account_id);
    p::kv("Created", &multisig_account.created_at);
    
    println!();
    p::info("Thresholds:");
    p::kv("  Low", &multisig_account.thresholds.low.to_string());
    p::kv("  Medium", &multisig_account.thresholds.medium.to_string());
    p::kv("  High", &multisig_account.thresholds.high.to_string());

    println!();
    p::info(&format!("Signers ({}):", multisig_account.signers.len()));
    
    if multisig_account.signers.is_empty() {
        p::warn("  No signers configured yet");
    } else {
        for (i, signer) in multisig_account.signers.iter().enumerate() {
            println!();
            p::kv(&format!("  [{}] Key", i + 1), &signer.public_key);
            p::kv(&format!("  [{}] Weight", i + 1), &signer.weight.to_string());
            if let Some(ref signer_name) = signer.name {
                p::kv(&format!("  [{}] Name", i + 1), signer_name);
            }
        }
    }

    let total_weight = multisig::calculate_total_weight(&multisig_account.signers);
    println!();
    p::kv_accent("Total Weight", &total_weight.to_string());
    
    p::separator();
    Ok(())
}
