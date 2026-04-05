use std::sync::Arc;

use russh::client;
use russh::keys::{decode_secret_key, key::PrivateKeyWithHashAlg};
use tokio::net::TcpListener;
use tokio::task::JoinHandle;

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

/// Owns the accept-loop task. Dropping this struct aborts the task, which
/// closes the TcpListener and disconnects the SSH session cleanly.
pub struct SshTunnel {
    local_port: u16,
    _task: JoinHandle<()>,
}

impl Drop for SshTunnel {
    fn drop(&mut self) {
        self._task.abort();
    }
}

impl SshTunnel {
    /// Connects to the SSH server, authenticates, binds a local listener, and
    /// spawns an async task that forwards each accepted TCP connection through
    /// a direct-tcpip SSH channel to `target_host:target_port`.
    ///
    /// Dropping the returned `SshTunnel` shuts the tunnel down (aborts the task,
    /// closes the listener, and disconnects the SSH session).
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
        // The JoinHandle is stored in SshTunnel; dropping it aborts this task.
        let task = tokio::spawn(async move {
            loop {
                let (mut local_stream, orig_addr) = match listener.accept().await {
                    Ok(pair) => pair,
                    Err(_) => break,
                };

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
                    // SSH session is dead — break so the task exits cleanly.
                    // The Drop impl on SshTunnel won't be triggered here (we're inside the task),
                    // but the DB pool's next operation will fail with an IO error, which triggers
                    // reconnect logic in the command layer that replaces this tunnel entirely.
                    Err(_) => break,
                };

                tokio::spawn(async move {
                    let mut ssh_stream = channel.into_stream();
                    tokio::io::copy_bidirectional(&mut local_stream, &mut ssh_stream)
                        .await
                        .ok();
                });
            }
        });

        Ok(SshTunnel { local_port, _task: task })
    }

    pub fn local_port(&self) -> u16 {
        self.local_port
    }
}
