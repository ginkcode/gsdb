use ssh2::Session;
use std::net::TcpStream;
use std::sync::Arc;
use tokio::sync::Mutex;

use super::types::SshConfig;

/// Active SSH tunnel that keeps the session alive
pub struct SshTunnel {
    #[allow(dead_code)]
    session: Arc<Mutex<Option<Session>>>,
    local_port: u16,
}

impl SshTunnel {
    /// Create an SSH tunnel and return the local port
    pub fn create(ssh: &SshConfig, target_host: &str, target_port: u16) -> Result<Self, String> {
        // Connect to SSH server
        let ssh_addr = format!("{}:{}", ssh.host, ssh.port);
        let tcp = TcpStream::connect(&ssh_addr)
            .map_err(|e| format!("Failed to connect to SSH server {}: {}", ssh_addr, e))?;
        tcp.set_nonblocking(false)
            .map_err(|e| format!("Failed to set blocking mode: {}", e))?;

        let mut session =
            Session::new().map_err(|e| format!("Failed to create SSH session: {}", e))?;
        session.set_tcp_stream(tcp);
        session
            .handshake()
            .map_err(|e| format!("SSH handshake failed: {}", e))?;

        // Authenticate
        if let Some(password) = &ssh.password {
            session
                .userauth_password(&ssh.username, password)
                .map_err(|e| format!("SSH password authentication failed: {}", e))?;
        } else if let Some(private_key) = &ssh.private_key {
            let passphrase = ssh.private_key_passphrase.as_deref();
            // libssh2's userauth_pubkey_memory does not support the OpenSSH private key
            // format (-----BEGIN OPENSSH PRIVATE KEY-----). Write to a temp file so that
            // OpenSSL reads it directly, which handles both legacy PEM and OpenSSH formats.
            let tmp_key_path = {
                use std::io::Write;
                let mut tmp = tempfile::NamedTempFile::new()
                    .map_err(|e| format!("Failed to create temp key file: {}", e))?;
                tmp.write_all(private_key.as_bytes())
                    .map_err(|e| format!("Failed to write temp key file: {}", e))?;
                // Keep the file alive by persisting it; we delete it after auth.
                tmp.into_temp_path()
            };
            let result = session.userauth_pubkey_file(
                &ssh.username,
                None,
                tmp_key_path.as_ref(),
                passphrase,
            );
            // Delete temp file immediately after use regardless of result
            let _ = tmp_key_path.close();
            result.map_err(|e| format!("SSH key authentication failed: {}", e))?;
        } else {
            // Try default SSH key from ssh-agent
            session
                .userauth_agent(&ssh.username)
                .map_err(|e| format!("SSH agent authentication failed: {}", e))?;
        }

        if !session.authenticated() {
            return Err("SSH authentication failed".to_string());
        }

        // Bind the local listener before spawning so we can return the port immediately
        let listener = std::net::TcpListener::bind("127.0.0.1:0")
            .map_err(|e| format!("Failed to bind local port: {}", e))?;
        let local_port = listener
            .local_addr()
            .map_err(|e| format!("Failed to get local address: {}", e))?
            .port();
        listener
            .set_nonblocking(true)
            .map_err(|e| format!("Failed to set non-blocking: {}", e))?;

        let target_host = target_host.to_string();

        // The SSH session owns all channels, so everything must run on one thread.
        // We use a non-blocking event loop that:
        //   1. Accepts new TCP connections and opens a fresh SSH channel for each
        //   2. Pumps data bidirectionally between each (TcpStream, Channel) pair
        std::thread::spawn(move || {
            use std::io::{Read, Write};

            // Each entry is (local_tcp_stream, ssh_channel)
            let mut pipes: Vec<(std::net::TcpStream, ssh2::Channel)> = Vec::new();
            let mut buf = [0u8; 16384];

            loop {
                // Accept any new incoming connections (listener is non-blocking)
                loop {
                    match listener.accept() {
                        Ok((stream, _)) => {
                            stream.set_nonblocking(true).ok();
                            // channel_direct_tcpip is a round-trip with the server so it
                            // must run in blocking mode; non-blocking returns EAGAIN silently
                            session.set_blocking(true);
                            let ch_result =
                                session.channel_direct_tcpip(&target_host, target_port, None);
                            session.set_blocking(false);
                            if let Ok(ch) = ch_result {
                                pipes.push((stream, ch));
                            }
                            // If channel open failed the stream is dropped → client gets RST
                        }
                        Err(e) if e.kind() == std::io::ErrorKind::WouldBlock => break,
                        Err(_) => return, // listener closed or fatal error
                    }
                }

                // Pump data for all active pipes
                let mut to_close: Vec<usize> = Vec::new();
                for (i, (stream, ch)) in pipes.iter_mut().enumerate() {
                    let mut done = false;

                    // channel → local stream
                    match ch.read(&mut buf) {
                        Ok(0) => {} // libssh2 non-blocking returns 0 when no data, not WouldBlock
                        Ok(n) => {
                            stream.write_all(&buf[..n]).ok();
                        }
                        Err(e) if e.kind() == std::io::ErrorKind::WouldBlock => {}
                        Err(_) => done = true,
                    }

                    // local stream → channel
                    match stream.read(&mut buf) {
                        Ok(0) => done = true, // TCP peer closed connection
                        Ok(n) => {
                            ch.write_all(&buf[..n]).ok();
                        }
                        Err(e) if e.kind() == std::io::ErrorKind::WouldBlock => {}
                        Err(_) => done = true,
                    }

                    // ch.eof() is only reliable after set_blocking(false); check last
                    if done || ch.eof() {
                        to_close.push(i);
                    }
                }

                // Remove closed pipes (in reverse order to preserve indices)
                for i in to_close.into_iter().rev() {
                    let (_, mut ch) = pipes.remove(i);
                    let _ = ch.send_eof();
                    let _ = ch.close();
                }

                std::thread::sleep(std::time::Duration::from_millis(if pipes.is_empty() {
                    5
                } else {
                    1
                }));
            }
        });

        let session_arc = Arc::new(Mutex::new(None::<Session>));
        Ok(SshTunnel {
            session: session_arc,
            local_port,
        })
    }

    pub fn local_port(&self) -> u16 {
        self.local_port
    }
}
