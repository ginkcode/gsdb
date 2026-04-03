use std::sync::Arc;

use russh::client;
use russh::keys::{decode_secret_key, key::PrivateKeyWithHashAlg};
use tokio::net::TcpListener;

use super::types::SshConfig;

// ── SSH client handler ────────────────────────────────────────────────────────

struct AcceptAllKeys;

impl client::Handler for AcceptAllKeys {
    type Error = russh::Error;

    async fn check_server_key(
        &mut self,
        _key: &russh::keys::ssh_key::PublicKey,
    ) -> Result<bool, Self::Error> {
        // Accept all host keys.
        // TODO: validate against known_hosts for production security.
        Ok(true)
    }
}

// ── Public tunnel handle ──────────────────────────────────────────────────────

pub struct SshTunnel {
    local_port: u16,
}

impl SshTunnel {
    /// Connects to the SSH server, authenticates, binds a local listener, and
    /// spawns an async task that forwards each accepted TCP connection through
    /// a direct-tcpip SSH channel to `target_host:target_port`.
    ///
    /// Pure async — no blocking threads, no non-blocking mode switching.
    /// Works correctly on Windows because russh is pure Rust with tokio I/O.
    pub async fn create(
        ssh: &SshConfig,
        target_host: &str,
        target_port: u16,
    ) -> Result<Self, String> {
        let config = Arc::new(client::Config {
            nodelay: true,
            ..Default::default()
        });

        // Connect to the SSH server
        let mut handle = client::connect(config, (ssh.host.as_str(), ssh.port), AcceptAllKeys)
            .await
            .map_err(|e| format!("SSH connect failed: {}", e))?;

        // Authenticate
        if let Some(password) = &ssh.password {
            let result = handle
                .authenticate_password(&ssh.username, password)
                .await
                .map_err(|e| format!("SSH password authentication failed: {}", e))?;
            if !result.success() {
                return Err("SSH password authentication rejected by server".to_string());
            }
        } else if let Some(key_str) = &ssh.private_key {
            let passphrase = ssh.private_key_passphrase.as_deref();
            let key = decode_secret_key(key_str, passphrase)
                .map_err(|e| format!("SSH key parse failed: {}", e))?;
            // For RSA keys the server advertises preferred hash algorithm;
            // Ed25519/ECDSA ignore this field.
            let hash_alg = handle
                .best_supported_rsa_hash()
                .await
                .map_err(|e| format!("SSH RSA hash negotiation failed: {}", e))?
                .flatten();
            let result = handle
                .authenticate_publickey(
                    &ssh.username,
                    PrivateKeyWithHashAlg::new(Arc::new(key), hash_alg),
                )
                .await
                .map_err(|e| format!("SSH key authentication failed: {}", e))?;
            if !result.success() {
                return Err("SSH key authentication rejected by server".to_string());
            }
        } else {
            return Err(
                "SSH authentication requires either a password or a private key".to_string(),
            );
        }

        // Bind local listener on an OS-assigned port
        let listener = TcpListener::bind("127.0.0.1:0")
            .await
            .map_err(|e| format!("Failed to bind local port: {}", e))?;
        let local_port = listener
            .local_addr()
            .map_err(|e| format!("Failed to get local port: {}", e))?
            .port();

        let target_host = target_host.to_string();

        // Spawn the accept loop — `handle` lives here, keeping the SSH session open.
        // Each accepted connection gets its own task with a fresh direct-tcpip channel.
        // No blocking/non-blocking mode switching; tokio handles all I/O.
        tokio::spawn(async move {
            loop {
                let (mut local_stream, orig_addr) = match listener.accept().await {
                    Ok(pair) => pair,
                    Err(_) => break,
                };

                // Open a direct-tcpip channel for this connection.
                // channel_open_direct_tcpip takes &self so handle stays in the loop.
                let channel = match handle
                    .channel_open_direct_tcpip(
                        &target_host,
                        target_port as u32,
                        orig_addr.ip().to_string(),
                        orig_addr.port() as u32,
                    )
                    .await
                {
                    Ok(ch) => ch,
                    Err(_) => continue, // channel open failed; drop connection
                };

                // Bidirectional copy between local TCP stream and SSH channel.
                tokio::spawn(async move {
                    let mut ssh_stream = channel.into_stream();
                    tokio::io::copy_bidirectional(&mut local_stream, &mut ssh_stream)
                        .await
                        .ok();
                });
            }
        });

        Ok(SshTunnel { local_port })
    }

    pub fn local_port(&self) -> u16 {
        self.local_port
    }
}
