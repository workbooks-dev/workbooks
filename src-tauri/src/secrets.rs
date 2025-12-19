use aes_gcm::{
    aead::{Aead, KeyInit, OsRng},
    Aes256Gcm, Nonce,
};
use anyhow::{Context, Result};
use base64::{engine::general_purpose, Engine as _};
use rand::RngCore;
use rusqlite::{params, Connection};
use serde::{Deserialize, Serialize};
use std::path::Path;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

const NONCE_SIZE: usize = 12; // 96 bits for AES-GCM
const SESSION_TIMEOUT: Duration = Duration::from_secs(10 * 60); // 10 minutes

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct Secret {
    pub id: String,
    pub key: String,
    pub created_at: i64,
    pub modified_at: i64,
}

#[derive(Debug, Clone)]
struct SessionState {
    last_auth: Option<Instant>,
    is_locked: bool,
}

impl SessionState {
    fn new() -> Self {
        Self {
            last_auth: None,
            is_locked: false,
        }
    }

    fn is_valid(&self) -> bool {
        if self.is_locked {
            return false;
        }

        match self.last_auth {
            Some(last_auth) => last_auth.elapsed() < SESSION_TIMEOUT,
            None => false,
        }
    }

    fn authenticate(&mut self) {
        self.last_auth = Some(Instant::now());
        self.is_locked = false;
    }

    fn lock(&mut self) {
        self.is_locked = true;
        self.last_auth = None;
    }
}

pub struct SecretsManager {
    db_path: std::path::PathBuf,
    key_file_path: std::path::PathBuf,
    session: Arc<Mutex<SessionState>>,
}

impl SecretsManager {
    /// Create a new SecretsManager for a project
    /// Secrets are stored centrally in ~/.tether/secrets/{project-hash}/secrets.db
    /// This keeps secrets machine-specific and out of the project folder
    pub fn new(project_root: &Path) -> Result<Self> {
        // Create centralized tether directory in user's home
        let home_dir = dirs::home_dir()
            .context("Failed to get home directory")?;
        let tether_home = home_dir.join(".tether");
        std::fs::create_dir_all(&tether_home)
            .context("Failed to create ~/.tether directory")?;

        // Create unique directory for this project's secrets
        // Use the full path hash for stability
        use std::collections::hash_map::DefaultHasher;
        use std::hash::{Hash, Hasher};

        let mut hasher = DefaultHasher::new();
        project_root.to_string_lossy().hash(&mut hasher);
        let path_hash = hasher.finish();

        let project_secrets_dir = tether_home.join("secrets").join(format!("{:x}", path_hash));
        std::fs::create_dir_all(&project_secrets_dir)
            .context("Failed to create project secrets directory")?;

        let db_path = project_secrets_dir.join("secrets.db");

        // Store a reference file so we know which project this is for
        let project_ref_path = project_secrets_dir.join("project_path.txt");
        if !project_ref_path.exists() {
            std::fs::write(&project_ref_path, project_root.to_string_lossy().as_bytes())
                .context("Failed to write project reference")?;
        }

        let key_file_path = project_secrets_dir.join("encryption.key");
        println!("DEBUG: Secrets stored at: {}", db_path.display());
        println!("DEBUG: Encryption key stored at: {}", key_file_path.display());

        let manager = Self {
            db_path,
            key_file_path,
            session: Arc::new(Mutex::new(SessionState::new())),
        };

        manager.init_db()?;
        manager.ensure_encryption_key()?;

        Ok(manager)
    }

    /// Initialize the SQLite database
    fn init_db(&self) -> Result<()> {
        let conn = Connection::open(&self.db_path)
            .context("Failed to open secrets database")?;

        conn.execute(
            "CREATE TABLE IF NOT EXISTS secrets (
                id TEXT PRIMARY KEY,
                key TEXT UNIQUE NOT NULL,
                encrypted_value BLOB NOT NULL,
                created_at INTEGER NOT NULL,
                modified_at INTEGER NOT NULL
            )",
            [],
        )
        .context("Failed to create secrets table")?;

        Ok(())
    }

    /// Ensure encryption key exists in key file, or generate a new one
    fn ensure_encryption_key(&self) -> Result<()> {
        if self.key_file_path.exists() {
            // Key already exists
            Ok(())
        } else {
            // Generate new 256-bit key
            let mut key = [0u8; 32];
            OsRng.fill_bytes(&mut key);
            let key_b64 = general_purpose::STANDARD.encode(key);

            // Write to file with restricted permissions (owner read/write only)
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

            println!("DEBUG: Generated and stored new encryption key");
            Ok(())
        }
    }

    /// Check if the current session is valid
    pub fn is_session_valid(&self) -> bool {
        let session = self.session.lock().unwrap();
        session.is_valid()
    }

    /// Lock the session manually
    pub fn lock_session(&self) {
        let mut session = self.session.lock().unwrap();
        session.lock();
        println!("DEBUG: Session manually locked");
    }

    /// Authenticate with Touch ID and create a session
    fn authenticate_and_create_session(&self, reason: &str) -> Result<()> {
        println!("DEBUG: Requesting Touch ID authentication");

        // Request Touch ID authentication on macOS
        #[cfg(target_os = "macos")]
        {
            crate::local_auth_macos::authenticate_with_touch_id(reason)
                .map_err(|e| anyhow::anyhow!("Touch ID authentication failed: {}", e))?;
            println!("DEBUG: Touch ID authentication successful");
        }

        // Verify we can access the encryption key
        let _ = self.get_encryption_key()
            .context("Failed to access encryption key after authentication")?;

        // Mark session as authenticated
        let mut session = self.session.lock().unwrap();
        session.authenticate();
        println!("DEBUG: Session created, valid for {} seconds", SESSION_TIMEOUT.as_secs());

        Ok(())
    }

    /// Ensure we have a valid session, authenticate if needed
    fn ensure_session(&self, reason: &str) -> Result<()> {
        let is_valid = self.is_session_valid();
        if !is_valid {
            self.authenticate_and_create_session(reason)?;
        } else {
            println!("DEBUG: Using existing valid session");
        }
        Ok(())
    }

    /// Explicitly check that the encryption key is accessible with Touch ID authentication
    /// This triggers Touch ID authentication on macOS
    pub fn ensure_encryption_key_accessible(&self) -> Result<()> {
        self.authenticate_and_create_session("Tether needs to access your secrets")
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
            .context("Failed to get encryption key from keychain")?;
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

    /// Add a new secret (no authentication required - you're typing the value in)
    pub fn add_secret(&self, key: &str, value: &str) -> Result<Secret> {
        let encrypted = self.encrypt(value)?;
        let now = chrono::Utc::now().timestamp();
        let id = uuid::Uuid::new_v4().to_string();

        let conn = Connection::open(&self.db_path)?;
        conn.execute(
            "INSERT INTO secrets (id, key, encrypted_value, created_at, modified_at)
             VALUES (?1, ?2, ?3, ?4, ?5)",
            params![id, key, encrypted, now, now],
        )
        .context("Failed to insert secret")?;

        Ok(Secret {
            id,
            key: key.to_string(),
            created_at: now,
            modified_at: now,
        })
    }

    /// Get a secret value (decrypted) - uses session if valid
    pub fn get_secret(&self, key: &str) -> Result<String> {
        // Use existing session or authenticate
        self.ensure_session("Authenticate to access secrets")?;

        let conn = Connection::open(&self.db_path)?;
        let encrypted: Vec<u8> = conn
            .query_row(
                "SELECT encrypted_value FROM secrets WHERE key = ?1",
                params![key],
                |row| row.get(0),
            )
            .context("Secret not found")?;

        self.decrypt(&encrypted)
    }

    /// Get a secret value with explicit authentication requirement
    pub fn get_secret_authenticated(&self, key: &str) -> Result<String> {
        // Always re-authenticate for explicit requests
        self.authenticate_and_create_session("Authenticate to view this secret")?;
        self.get_secret(key)
    }

    /// List all secrets (keys only, not values) - no authentication required
    pub fn list_secrets(&self) -> Result<Vec<Secret>> {
        let conn = Connection::open(&self.db_path)?;
        let mut stmt = conn.prepare(
            "SELECT id, key, created_at, modified_at FROM secrets ORDER BY key",
        )?;

        let secrets = stmt
            .query_map([], |row| {
                Ok(Secret {
                    id: row.get(0)?,
                    key: row.get(1)?,
                    created_at: row.get(2)?,
                    modified_at: row.get(3)?,
                })
            })?
            .collect::<Result<Vec<_>, _>>()
            .context("Failed to list secrets")?;

        Ok(secrets)
    }

    /// Update a secret (requires Touch ID - always re-authenticates)
    pub fn update_secret(&self, key: &str, value: &str) -> Result<()> {
        // Always require fresh authentication for destructive operations
        self.authenticate_and_create_session("Authenticate to update this secret")?;

        let encrypted = self.encrypt(value)?;
        let now = chrono::Utc::now().timestamp();

        let conn = Connection::open(&self.db_path)?;
        let updated = conn.execute(
            "UPDATE secrets SET encrypted_value = ?1, modified_at = ?2 WHERE key = ?3",
            params![encrypted, now, key],
        )?;

        if updated == 0 {
            anyhow::bail!("Secret not found: {}", key);
        }

        Ok(())
    }

    /// Delete a secret (requires Touch ID - always re-authenticates)
    pub fn delete_secret(&self, key: &str) -> Result<()> {
        // Always require fresh authentication for destructive operations
        self.authenticate_and_create_session("Authenticate to delete this secret")?;

        let conn = Connection::open(&self.db_path)?;
        let deleted = conn.execute("DELETE FROM secrets WHERE key = ?1", params![key])?;

        if deleted == 0 {
            anyhow::bail!("Secret not found: {}", key);
        }

        Ok(())
    }

    /// Get all secrets with their values (for environment injection) - no authentication required
    /// This allows notebooks to start without Touch ID prompts
    pub fn get_all_secrets_with_values(&self) -> Result<Vec<(String, String)>> {
        let conn = Connection::open(&self.db_path)?;
        let mut stmt = conn.prepare(
            "SELECT key, encrypted_value FROM secrets ORDER BY key",
        )?;

        let secrets = stmt
            .query_map([], |row| {
                let key: String = row.get(0)?;
                let encrypted: Vec<u8> = row.get(1)?;
                Ok((key, encrypted))
            })?
            .collect::<Result<Vec<_>, _>>()
            .context("Failed to fetch secrets")?;

        let mut result = Vec::new();
        for (key, encrypted) in secrets {
            let value = self.decrypt(&encrypted)?;
            result.push((key, value));
        }

        Ok(result)
    }

    /// Import secrets from a .env file (no authentication required - you're choosing the file)
    pub fn import_from_env(&self, env_path: &Path) -> Result<Vec<String>> {
        let content = std::fs::read_to_string(env_path)
            .context("Failed to read .env file")?;

        let mut imported = Vec::new();

        for line in content.lines() {
            let line = line.trim();

            // Skip empty lines and comments
            if line.is_empty() || line.starts_with('#') {
                continue;
            }

            // Parse KEY=VALUE
            if let Some((key, value)) = line.split_once('=') {
                let key = key.trim();
                let value = value.trim().trim_matches('"').trim_matches('\'');

                let encrypted = self.encrypt(value)?;
                let now = chrono::Utc::now().timestamp();

                let conn = Connection::open(&self.db_path)?;

                // Try insert first
                match conn.execute(
                    "INSERT INTO secrets (id, key, encrypted_value, created_at, modified_at)
                     VALUES (?1, ?2, ?3, ?4, ?5)",
                    params![uuid::Uuid::new_v4().to_string(), key, encrypted, now, now],
                ) {
                    Ok(_) => imported.push(key.to_string()),
                    Err(_) => {
                        // Key exists, update it
                        conn.execute(
                            "UPDATE secrets SET encrypted_value = ?1, modified_at = ?2 WHERE key = ?3",
                            params![encrypted, now, key],
                        )?;
                        imported.push(key.to_string());
                    }
                }
            }
        }

        Ok(imported)
    }
}
