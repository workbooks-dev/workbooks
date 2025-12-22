use anyhow::{Context, Result};
use keyring::Entry;

const SERVICE_NAME: &str = "Tether";
const ANTHROPIC_KEY_ACCOUNT: &str = "anthropic_api_key";

/// Store Anthropic API key in system keychain
/// This is stored at the OS level, separate from project secrets
pub fn set_anthropic_api_key(key: &str) -> Result<()> {
    let entry = Entry::new(SERVICE_NAME, ANTHROPIC_KEY_ACCOUNT)
        .context("Failed to create keychain entry")?;

    entry.set_password(key)
        .context("Failed to store API key in keychain")?;

    Ok(())
}

/// Retrieve Anthropic API key from system keychain
pub fn get_anthropic_api_key() -> Result<Option<String>> {
    let entry = Entry::new(SERVICE_NAME, ANTHROPIC_KEY_ACCOUNT)
        .context("Failed to create keychain entry")?;

    match entry.get_password() {
        Ok(password) => Ok(Some(password)),
        Err(keyring::Error::NoEntry) => Ok(None),
        Err(e) => Err(e).context("Failed to retrieve API key from keychain"),
    }
}

/// Delete Anthropic API key from keychain
pub fn delete_anthropic_api_key() -> Result<()> {
    let entry = Entry::new(SERVICE_NAME, ANTHROPIC_KEY_ACCOUNT)
        .context("Failed to create keychain entry")?;

    match entry.delete_credential() {
        Ok(()) => Ok(()),
        Err(keyring::Error::NoEntry) => Ok(()), // Already deleted
        Err(e) => Err(e).context("Failed to delete API key from keychain"),
    }
}

/// Check if an Anthropic API key is stored (without retrieving the value)
pub fn has_anthropic_api_key() -> bool {
    get_anthropic_api_key().ok().flatten().is_some()
}

// Tauri commands
#[tauri::command]
pub fn save_anthropic_api_key(key: String) -> Result<(), String> {
    if key.trim().is_empty() {
        return Err("API key cannot be empty".to_string());
    }

    set_anthropic_api_key(&key).map_err(|e| e.to_string())
}

#[tauri::command]
pub fn load_anthropic_api_key() -> Result<Option<String>, String> {
    get_anthropic_api_key().map_err(|e| e.to_string())
}

#[tauri::command]
pub fn remove_anthropic_api_key() -> Result<(), String> {
    delete_anthropic_api_key().map_err(|e| e.to_string())
}

#[tauri::command]
pub fn check_anthropic_api_key() -> bool {
    has_anthropic_api_key()
}
