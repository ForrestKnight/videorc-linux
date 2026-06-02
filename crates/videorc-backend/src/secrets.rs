use anyhow::{Context, Result};
use keyring_core::{Entry, Error};

const SERVICE: &str = "videorc";

pub fn init_native_secret_store() {
    init_platform_secret_store();
}

#[cfg(target_os = "macos")]
fn init_platform_secret_store() {
    match apple_native_keyring_store::keychain::Store::new() {
        Ok(store) => {
            let store: std::sync::Arc<keyring_core::CredentialStore> = store;
            keyring_core::set_default_store(store);
        }
        Err(error) => {
            tracing::warn!("Could not initialize macOS keychain store: {error}");
        }
    }
}

#[cfg(not(target_os = "macos"))]
fn init_platform_secret_store() {
    tracing::warn!("Native credential store is not configured for this platform yet.");
}

#[allow(dead_code)]
pub fn put_secret(secret_ref: &str, value: &str) -> Result<()> {
    Entry::new(SERVICE, secret_ref)
        .with_context(|| format!("Could not open secret ref {secret_ref}"))?
        .set_password(value)
        .with_context(|| format!("Could not write secret ref {secret_ref}"))
}

#[allow(dead_code)]
pub fn get_secret(secret_ref: &str) -> Result<String> {
    Entry::new(SERVICE, secret_ref)
        .with_context(|| format!("Could not open secret ref {secret_ref}"))?
        .get_password()
        .with_context(|| format!("Could not read secret ref {secret_ref}"))
}

pub fn delete_secret(secret_ref: &str) -> Result<()> {
    let entry = Entry::new(SERVICE, secret_ref)
        .with_context(|| format!("Could not open secret ref {secret_ref}"))?;
    match entry.delete_credential() {
        Ok(()) | Err(Error::NoEntry) => Ok(()),
        Err(error) => {
            Err(error).with_context(|| format!("Could not delete secret ref {secret_ref}"))
        }
    }
}
