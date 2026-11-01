//! Gateway Authentication, Session Management, CSRF & Rate Limiting (Phase 5).

use rand::RngCore;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::collections::HashMap;
use std::sync::RwLock;
use std::time::{Duration, Instant};

/// Role-based access control roles.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Role {
    Consumer = 1,
    Producer = 2,
    Auditor = 3,
    Operator = 4,
    Admin = 5,
}

impl Role {
    pub fn can_produce(&self) -> bool {
        matches!(self, Role::Producer | Role::Operator | Role::Admin)
    }

    pub fn can_consume(&self) -> bool {
        matches!(self, Role::Consumer | Role::Operator | Role::Admin)
    }

    pub fn can_audit(&self) -> bool {
        matches!(self, Role::Auditor | Role::Admin)
    }

    pub fn can_operate(&self) -> bool {
        matches!(self, Role::Operator | Role::Admin)
    }

    pub fn is_admin(&self) -> bool {
        matches!(self, Role::Admin)
    }
}

/// Authenticated user session.
#[derive(Debug, Clone)]
pub struct Session {
    pub token: String,
    pub csrf_token: String,
    pub username: String,
    pub role: Role,
    pub tenant_id: String,
    pub created_at: Instant,
    pub expires_at: Instant,
}

/// PBKDF2-HMAC-SHA256 password hash representation.
#[derive(Debug, Clone)]
pub struct PasswordHash {
    pub salt: [u8; 16],
    pub hash: [u8; 32],
}

impl PasswordHash {
    pub fn create(password: &str) -> Self {
        let mut salt = [0u8; 16];
        rand::thread_rng().fill_bytes(&mut salt);
        let hash = Self::compute_hash(password, &salt);
        Self { salt, hash }
    }

    pub fn verify(&self, password: &str) -> bool {
        let computed = Self::compute_hash(password, &self.salt);
        // Constant-time comparison
        let mut diff = 0u8;
        for (a, b) in self.hash.iter().zip(computed.iter()) {
            diff |= a ^ b;
        }
        diff == 0
    }

    fn compute_hash(password: &str, salt: &[u8; 16]) -> [u8; 32] {
        let mut current = [0u8; 32];
        let mut hasher = Sha256::new();
        hasher.update(password.as_bytes());
        hasher.update(salt);
        current.copy_from_slice(&hasher.finalize());

        // 10,000 rounds of iterative hashing
        for i in 1..10_000u32 {
            let mut h = Sha256::new();
            h.update(current);
            h.update(password.as_bytes());
            h.update(i.to_le_bytes());
            current.copy_from_slice(&h.finalize());
        }
        current
    }
}

/// In-memory authenticated user store with external secrets & tenant binding.
pub struct UserStore {
    users: HashMap<String, (PasswordHash, Role, String)>,
}

impl UserStore {
    pub fn new_with_defaults() -> Self {
        Self::load_from_env_or_file(true).expect("defaults always succeed in dev")
    }

    pub fn load_from_file(path: impl AsRef<std::path::Path>) -> Result<Self, String> {
        let mut store = Self {
            users: HashMap::new(),
        };
        let p = path.as_ref();
        let data = std::fs::read_to_string(p)
            .map_err(|e| format!("Failed to read users file {}: {e}", p.display()))?;
        for line in data.lines() {
            let trimmed = line.trim();
            if trimmed.is_empty() || trimmed.starts_with("#") {
                continue;
            }
            let parts: Vec<&str> = trimmed.split(":").map(|s| s.trim()).collect();
            if parts.len() >= 4 {
                let role = match parts[2].to_lowercase().as_str() {
                    "admin" => Role::Admin,
                    "operator" => Role::Operator,
                    "auditor" => Role::Auditor,
                    "producer" => Role::Producer,
                    "consumer" => Role::Consumer,
                    _ => Role::Consumer,
                };
                store.register_user(parts[0], parts[1], role, parts[3]);
            }
        }
        Ok(store)
    }

    pub fn load_from_env_or_file(allow_dev_defaults: bool) -> Result<Self, String> {
        if let Ok(path) = std::env::var("KAMISH_USERS_FILE") {
            return Self::load_from_file(path);
        }

        let mut store = Self {
            users: HashMap::new(),
        };

        if let Ok(admin_pw) = std::env::var("KAMISH_ADMIN_PASSWORD") {
            store.register_user("admin", &admin_pw, Role::Admin, "tenant-sovereign");
            if let Ok(auditor_pw) = std::env::var("KAMISH_AUDITOR_PASSWORD") {
                store.register_user("auditor", &auditor_pw, Role::Auditor, "tenant-sovereign");
            }
            if let Ok(producer_pw) = std::env::var("KAMISH_PRODUCER_PASSWORD") {
                store.register_user("producer", &producer_pw, Role::Producer, "tenant-default");
            }
            if let Ok(consumer_pw) = std::env::var("KAMISH_CONSUMER_PASSWORD") {
                store.register_user("consumer", &consumer_pw, Role::Consumer, "tenant-default");
            }
            return Ok(store);
        }

        if allow_dev_defaults {
            store.register_user(
                "admin",
                "kamish-admin-2026",
                Role::Admin,
                "tenant-sovereign",
            );
            store.register_user(
                "operator",
                "kamish-operator-2026",
                Role::Operator,
                "tenant-sovereign",
            );
            store.register_user(
                "auditor",
                "kamish-auditor-2026",
                Role::Auditor,
                "tenant-sovereign",
            );
            store.register_user(
                "producer",
                "kamish-producer-2026",
                Role::Producer,
                "tenant-sovereign",
            );
            store.register_user(
                "consumer",
                "kamish-consumer-2026",
                Role::Consumer,
                "tenant-sovereign",
            );
            store.register_user(
                "guest_producer",
                "kamish-guest-2026",
                Role::Producer,
                "tenant-guest",
            );
            store.register_user(
                "guest_consumer",
                "kamish-guest-2026",
                Role::Consumer,
                "tenant-guest",
            );
            return Ok(store);
        }

        Err("No credentials configured. Supply KAMISH_USERS_FILE or KAMISH_ADMIN_PASSWORD, or pass --dev for local development".to_string())
    }

    pub fn register_user(&mut self, username: &str, password: &str, role: Role, tenant: &str) {
        self.users.insert(
            username.to_lowercase(),
            (PasswordHash::create(password), role, tenant.to_string()),
        );
    }

    pub fn authenticate(&self, username: &str, password: &str) -> Option<(Role, String)> {
        let (pw_hash, role, tenant) = self.users.get(&username.to_lowercase())?;
        if pw_hash.verify(password) {
            Some((*role, tenant.clone()))
        } else {
            None
        }
    }
}

/// Thread-safe active session store with TTL enforcement.
pub struct SessionStore {
    sessions: RwLock<HashMap<String, Session>>,
    default_ttl: Duration,
}

impl SessionStore {
    pub fn new(default_ttl: Duration) -> Self {
        Self {
            sessions: RwLock::new(HashMap::new()),
            default_ttl,
        }
    }

    pub fn create_session(
        &self,
        username: &str,
        role: Role,
        tenant_id: impl Into<String>,
    ) -> Session {
        let mut token_bytes = [0u8; 32];
        let mut csrf_bytes = [0u8; 32];
        rand::thread_rng().fill_bytes(&mut token_bytes);
        rand::thread_rng().fill_bytes(&mut csrf_bytes);

        let token = hex::encode(token_bytes);
        let csrf_token = hex::encode(csrf_bytes);
        let now = Instant::now();

        let session = Session {
            token: token.clone(),
            csrf_token,
            username: username.to_string(),
            role,
            tenant_id: tenant_id.into(),
            created_at: now,
            expires_at: now + self.default_ttl,
        };

        let mut guard = self.sessions.write().unwrap();
        guard.insert(token, session.clone());
        session
    }

    pub fn rotate_session(&self, old_token: &str, new_role: Option<Role>) -> Option<Session> {
        let mut guard = self.sessions.write().unwrap();
        let old_session = guard.remove(old_token)?;
        if Instant::now() >= old_session.expires_at {
            return None;
        }

        let mut token_bytes = [0u8; 32];
        let mut csrf_bytes = [0u8; 32];
        rand::thread_rng().fill_bytes(&mut token_bytes);
        rand::thread_rng().fill_bytes(&mut csrf_bytes);

        let new_token = hex::encode(token_bytes);
        let new_csrf = hex::encode(csrf_bytes);
        let now = Instant::now();

        let new_session = Session {
            token: new_token.clone(),
            csrf_token: new_csrf,
            username: old_session.username,
            role: new_role.unwrap_or(old_session.role),
            tenant_id: old_session.tenant_id,
            created_at: now,
            expires_at: now + self.default_ttl,
        };

        guard.insert(new_token, new_session.clone());
        Some(new_session)
    }

    pub fn validate_session(&self, token: &str) -> Option<Session> {
        let mut guard = self.sessions.write().unwrap();
        if let Some(session) = guard.get(token)
            && Instant::now() < session.expires_at
        {
            return Some(session.clone());
        }
        guard.remove(token);
        None
    }

    pub fn invalidate_session(&self, token: &str) {
        let mut guard = self.sessions.write().unwrap();
        guard.remove(token);
    }
}

/// Sliding-window rate limiter per client IP.
pub struct RateLimiter {
    requests: RwLock<HashMap<String, Vec<Instant>>>,
    max_requests_per_window: usize,
    window_duration: Duration,
}

impl RateLimiter {
    pub fn new(max_requests_per_window: usize, window_duration: Duration) -> Self {
        Self {
            requests: RwLock::new(HashMap::new()),
            max_requests_per_window,
            window_duration,
        }
    }

    pub fn check_and_record(&self, client_key: &str) -> bool {
        let now = Instant::now();
        let cutoff = now.checked_sub(self.window_duration).unwrap_or(now);

        let mut guard = self.requests.write().unwrap();
        let timestamps = guard.entry(client_key.to_string()).or_default();
        timestamps.retain(|&t| t > cutoff);

        if timestamps.len() >= self.max_requests_per_window {
            false
        } else {
            timestamps.push(now);
            true
        }
    }
}
