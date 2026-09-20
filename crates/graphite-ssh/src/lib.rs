use std::path::Path;
use std::sync::Arc;

use async_trait::async_trait;
use graphite_core::{GraphiteError, Result, SshConfig};
use russh::client::{self, Handle, Msg};
use russh::ChannelMsg;
use russh_keys::key;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::{TcpListener, TcpStream};
use tokio::sync::Mutex;

struct Handler;

#[async_trait]
impl client::Handler for Handler {
    type Error = russh::Error;

    async fn check_server_key(
        &mut self,
        _server_public_key: &key::PublicKey,
    ) -> std::result::Result<bool, Self::Error> {
        Ok(true)
    }
}

pub struct SshTunnel {
    pub local_port: u16,
    _session: Arc<Mutex<Handle<Handler>>>,
}

pub async fn open_tunnel(ssh: &SshConfig, dest_host: &str, dest_port: u16) -> Result<SshTunnel> {
    let config = Arc::new(client::Config::default());
    let mut session = client::connect(config, (ssh.host.as_str(), ssh.port), Handler)
        .await
        .map_err(|e| GraphiteError::msg(e.to_string()))?;

    let authed = if let Some(key_path) = &ssh.private_key {
        let key = load_key(key_path)?;
        session
            .authenticate_publickey(&ssh.user, Arc::new(key))
            .await
            .map_err(|e| GraphiteError::msg(e.to_string()))?
    } else if let Some(password) = &ssh.password {
        session
            .authenticate_password(&ssh.user, password)
            .await
            .map_err(|e| GraphiteError::msg(e.to_string()))?
    } else {
        return Err(GraphiteError::msg("ssh needs password or privateKey"));
    };
    if !authed {
        return Err(GraphiteError::msg("ssh authentication failed"));
    }

    let listener = TcpListener::bind("127.0.0.1:0")
        .await
        .map_err(|e| GraphiteError::msg(e.to_string()))?;
    let local_port = listener
        .local_addr()
        .map_err(|e| GraphiteError::msg(e.to_string()))?
        .port();
    let session = Arc::new(Mutex::new(session));
    let dest_host = dest_host.to_string();
    let session_clone = session.clone();
    tokio::spawn(async move {
        loop {
            let Ok((socket, _)) = listener.accept().await else {
                break;
            };
            let session = session_clone.clone();
            let dest_host = dest_host.clone();
            tokio::spawn(async move {
                let _ = pump(session, socket, dest_host, dest_port).await;
            });
        }
    });
    Ok(SshTunnel {
        local_port,
        _session: session,
    })
}

async fn pump(
    session: Arc<Mutex<Handle<Handler>>>,
    mut local: TcpStream,
    dest_host: String,
    dest_port: u16,
) -> Result<()> {
    let channel = {
        let mut session = session.lock().await;
        session
            .channel_open_direct_tcpip(&dest_host, dest_port as u32, "127.0.0.1", 0)
            .await
            .map_err(|e| GraphiteError::msg(e.to_string()))?
    };
    let mut channel: russh::Channel<Msg> = channel;
    let mut buf = [0u8; 8192];
    loop {
        tokio::select! {
            read = local.read(&mut buf) => {
                let n = read.map_err(|e| GraphiteError::msg(e.to_string()))?;
                if n == 0 {
                    let _ = channel.eof().await;
                    break;
                }
                channel
                    .data(&buf[..n])
                    .await
                    .map_err(|e| GraphiteError::msg(e.to_string()))?;
            }
            msg = channel.wait() => {
                match msg {
                    Some(ChannelMsg::Data { ref data }) => {
                        local
                            .write_all(data)
                            .await
                            .map_err(|e| GraphiteError::msg(e.to_string()))?;
                    }
                    Some(ChannelMsg::Eof) | None => break,
                    _ => {}
                }
            }
        }
    }
    Ok(())
}

fn load_key(path: &str) -> Result<key::KeyPair> {
    russh_keys::load_secret_key(Path::new(path), None).map_err(|e| GraphiteError::msg(e.to_string()))
}
