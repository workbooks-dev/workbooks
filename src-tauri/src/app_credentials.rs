use aes_gcm::{
    aead::{Aead, KeyInit, OsRng},
    Aes256Gcm, Nonce,
};
use anyhow::{Context, Result};
use base64::{engine::general_purpose, Engine as _};
use rand::RngCore;
use rusqlite::{params, Connection};
use serde::{Deserialize, Serialize};
use std::path::PathBuf;
use std::sync::{Arc, Mutex, OnceLock};

const NONCE_SIZE: usize = 12; // 96 bits for AES-GCM

#[derive(Serialize, Deserialize)]
struct MessageRequest {
    model: String,
    max_tokens: u32,
    messages: Vec<Message>,
}

#[derive(Serialize, Deserialize)]
struct Message {
    role: String,
    content: String,
}

/// Global app credentials manager singleton
static APP_CREDENTIALS: OnceLock<Arc<Mutex<AppCredentialsManager>>> = OnceLock::new();

fn get_app_credentials() -> Arc<Mutex<AppCredentialsManager>> {
    APP_CREDENTIALS
        .get_or_init(|| {
            Arc::new(Mutex::new(
                AppCredentialsManager::new().expect("Failed to initialize app credentials manager")
            ))
        })
        .clone()
}

/// Manager for app-level encrypted credentials
pub struct AppCredentialsManager {
    db_path: PathBuf,
    key_file_path: PathBuf,
}

impl AppCredentialsManager {
    /// Create a new AppCredentialsManager
    /// Credentials are stored in ~/.workbooks/app/credentials.db
    pub fn new() -> Result<Self> {
        // Create ~/.workbooks/app directory
        let home_dir = dirs::home_dir()
            .context("Failed to get home directory")?;
        let app_dir = home_dir.join(".workbooks").join("app");
        std::fs::create_dir_all(&app_dir)
            .context("Failed to create ~/.workbooks/app directory")?;

        let db_path = app_dir.join("credentials.db");
        let key_file_path = app_dir.join("encryption.key");

        let manager = Self {
            db_path,
            key_file_path,
        };

        manager.init_db()?;
        manager.ensure_encryption_key()?;

        Ok(manager)
    }

    /// Initialize the SQLite database
    fn init_db(&self) -> Result<()> {
        let conn = Connection::open(&self.db_path)
            .context("Failed to open credentials database")?;

        conn.execute(
            "CREATE TABLE IF NOT EXISTS credentials (
                key TEXT PRIMARY KEY,
                encrypted_value BLOB NOT NULL,
                created_at INTEGER NOT NULL,
                modified_at INTEGER NOT NULL
            )",
            [],
        )
        .context("Failed to create credentials table")?;

        Ok(())
    }

    /// Ensure encryption key exists in key file, or generate a new one
    fn ensure_encryption_key(&self) -> Result<()> {
        if self.key_file_path.exists() {
            Ok(())
        } else {
            // Generate new 256-bit key
            let mut key = [0u8; 32];
            OsRng.fill_bytes(&mut key);
            let key_b64 = general_purpose::STANDARD.encode(key);

            // Write to file with restricted permissions
            std::fs::write(&self.key_file_path, key_b64.as_bytes())
                .context("Failed to write encryption key file")?;

            // Set permissions to 0600 (owner read/write only)
            #[cfg(unix)]
            {
                use std::os::unix::fs::PermissionsExt;
                let perms = std::fs::Permissions::from_mode(0o600);
                std::fs::set_permissions(&self.key_file_path, perms)
                    .context("Failed to set encryption key file permissions")?;
            }

            println!("✅ Generated new app credentials encryption key");
            Ok(())
        }
    }

    /// Get the encryption key from file
    fn get_encryption_key(&self) -> Result<[u8; 32]> {
        let key_b64 = std::fs::read_to_string(&self.key_file_path)
            .context("Failed to read encryption key file")?;

        let key_bytes = general_purpose::STANDARD
            .decode(key_b64.trim())
            .context("Failed to decode encryption key")?;

        let mut key = [0u8; 32];
        key.copy_from_slice(&key_bytes);
        Ok(key)
    }

    /// Encrypt a value
    fn encrypt(&self, plaintext: &str) -> Result<Vec<u8>> {
        let key = self.get_encryption_key()?;
        let cipher = Aes256Gcm::new(&key.into());

        // Generate random nonce
        let mut nonce_bytes = [0u8; NONCE_SIZE];
        OsRng.fill_bytes(&mut nonce_bytes);
        let nonce = Nonce::from_slice(&nonce_bytes);

        // Encrypt
        let ciphertext = cipher
            .encrypt(nonce, plaintext.as_bytes())
            .map_err(|e| anyhow::anyhow!("Encryption failed: {}", e))?;

        // Prepend nonce to ciphertext
        let mut encrypted = nonce_bytes.to_vec();
        encrypted.extend_from_slice(&ciphertext);

        Ok(encrypted)
    }

    /// Decrypt a value
    fn decrypt(&self, encrypted: &[u8]) -> Result<String> {
        if encrypted.len() < NONCE_SIZE {
            anyhow::bail!("Invalid encrypted data - data too short");
        }

        let key = self.get_encryption_key()
            .context("Failed to get encryption key")?;
        let cipher = Aes256Gcm::new(&key.into());

        // Extract nonce
        let nonce = Nonce::from_slice(&encrypted[..NONCE_SIZE]);
        let ciphertext = &encrypted[NONCE_SIZE..];

        // Decrypt
        let plaintext = cipher
            .decrypt(nonce, ciphertext)
            .map_err(|e| anyhow::anyhow!(
                "Decryption failed: {}. This usually means the encryption key has changed.",
                e
            ))?;

        String::from_utf8(plaintext).context("Invalid UTF-8 in decrypted data")
    }

    /// Set a credential
    pub fn set_credential(&self, key: &str, value: &str) -> Result<()> {
        println!("🔐 set_credential: key='{}', value_length={}", key, value.len());

        let encrypted = self.encrypt(value)?;
        let now = chrono::Utc::now().timestamp();

        let conn = Connection::open(&self.db_path)?;

        // Use INSERT OR REPLACE to handle both insert and update
        conn.execute(
            "INSERT OR REPLACE INTO credentials (key, encrypted_value, created_at, modified_at)
             VALUES (?1, ?2, COALESCE((SELECT created_at FROM credentials WHERE key = ?1), ?3), ?4)",
            params![key, encrypted, now, now],
        )
        .context("Failed to save credential")?;

        println!("✅ set_credential: Credential saved successfully");
        Ok(())
    }

    /// Get a credential (without authentication)
    pub fn get_credential(&self, key: &str) -> Result<Option<String>> {
        println!("🔍 get_credential: Looking for key='{}'", key);

        let conn = Connection::open(&self.db_path)?;

        let mut stmt = conn.prepare(
            "SELECT encrypted_value FROM credentials WHERE key = ?1"
        )?;

        let result = stmt.query_row(params![key], |row| {
            let encrypted: Vec<u8> = row.get(0)?;
            Ok(encrypted)
        });

        match result {
            Ok(encrypted) => {
                let decrypted = self.decrypt(&encrypted)?;
                println!("✅ get_credential: Found credential with length {}", decrypted.len());
                Ok(Some(decrypted))
            }
            Err(rusqlite::Error::QueryReturnedNoRows) => {
                println!("❌ get_credential: No credential found for key '{}'", key);
                Ok(None)
            }
            Err(e) => {
                println!("❌ get_credential: Database error: {}", e);
                Err(e).context("Failed to query credential")
            }
        }
    }

    /// Get a credential with Touch ID authentication
    pub fn get_credential_authenticated(&self, key: &str) -> Result<String> {
        println!("🔐 get_credential_authenticated: Requesting Touch ID for key='{}'", key);

        // Request Touch ID authentication on macOS
        #[cfg(target_os = "macos")]
        {
            crate::local_auth_macos::authenticate_with_touch_id(
                &format!("Authenticate to view {}", key)
            )
            .map_err(|e| anyhow::anyhow!("Touch ID authentication failed: {}", e))?;
            println!("✅ Touch ID authentication successful");
        }

        // Get the credential
        match self.get_credential(key)? {
            Some(value) => Ok(value),
            None => Err(anyhow::anyhow!("No credential stored for key '{}'", key)),
        }
    }

    /// Delete a credential
    pub fn delete_credential(&self, key: &str) -> Result<()> {
        println!("🗑️  delete_credential: Deleting key='{}'", key);

        let conn = Connection::open(&self.db_path)?;
        conn.execute("DELETE FROM credentials WHERE key = ?1", params![key])?;

        println!("✅ delete_credential: Credential deleted");
        Ok(())
    }

    /// Check if a credential exists
    pub fn has_credential(&self, key: &str) -> bool {
        let conn = match Connection::open(&self.db_path) {
            Ok(c) => c,
            Err(_) => return false,
        };

        let result: Result<i64, _> = conn.query_row(
            "SELECT COUNT(*) FROM credentials WHERE key = ?1",
            params![key],
            |row| row.get(0),
        );

        result.map(|count| count > 0).unwrap_or(false)
    }
}

// Tauri commands

#[tauri::command]
pub fn save_anthropic_api_key(key: String) -> Result<(), String> {
    println!("🔑 save_anthropic_api_key called with key length: {}", key.len());

    if key.trim().is_empty() {
        return Err("API key cannot be empty".to_string());
    }

    let manager = get_app_credentials();
    let manager = manager.lock().unwrap();

    manager
        .set_credential("anthropic_api_key", &key)
        .map_err(|e| e.to_string())?;

    println!("✅ Anthropic API key saved successfully");
    Ok(())
}

#[tauri::command]
pub fn load_anthropic_api_key() -> Result<Option<String>, String> {
    let manager = get_app_credentials();
    let manager = manager.lock().unwrap();

    manager
        .get_credential("anthropic_api_key")
        .map_err(|e| e.to_string())
}

#[tauri::command]
pub fn get_anthropic_api_key_authenticated() -> Result<String, String> {
    println!("🔐 get_anthropic_api_key_authenticated called");

    let manager = get_app_credentials();
    let manager = manager.lock().unwrap();

    manager
        .get_credential_authenticated("anthropic_api_key")
        .map_err(|e| e.to_string())
}

#[tauri::command]
pub fn remove_anthropic_api_key() -> Result<(), String> {
    let manager = get_app_credentials();
    let manager = manager.lock().unwrap();

    manager
        .delete_credential("anthropic_api_key")
        .map_err(|e| e.to_string())
}

#[tauri::command]
pub fn check_anthropic_api_key() -> bool {
    let manager = get_app_credentials();
    let manager = manager.lock().unwrap();

    manager.has_credential("anthropic_api_key")
}

/// Verify API key by making a test request to Anthropic API
#[tauri::command]
pub async fn verify_anthropic_api_key(key: String) -> Result<(), String> {
    // Format validation
    if !key.starts_with("sk-ant-") {
        return Err("Invalid API key format. Key should start with 'sk-ant-'".to_string());
    }

    // Make a minimal test request to verify the key works
    let client = reqwest::Client::new();

    let request_body = MessageRequest {
        model: "claude-sonnet-4-5-20250929".to_string(),
        max_tokens: 10,
        messages: vec![Message {
            role: "user".to_string(),
            content: "Hi".to_string(),
        }],
    };

    let response = client
        .post("https://api.anthropic.com/v1/messages")
        .header("x-api-key", key)
        .header("anthropic-version", "2023-06-01")
        .header("content-type", "application/json")
        .json(&request_body)
        .send()
        .await
        .map_err(|e| format!("Network error: {}", e))?;

    if response.status().is_success() {
        Ok(())
    } else {
        let status = response.status();
        let error_text = response.text().await.unwrap_or_else(|_| "Unknown error".to_string());

        if status == 401 {
            Err("Invalid API key. Please check your key and try again.".to_string())
        } else if status == 429 {
            Err("Rate limit exceeded. Please try again later.".to_string())
        } else {
            Err(format!("API error ({}): {}", status, error_text))
        }
    }
}
