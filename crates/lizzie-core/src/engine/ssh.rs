//! SSH remote engines (Java `SSHController` / `AnalysisEngineSSHController` over ganymed-ssh2):
//! an engine command runs on an SSH host and its standard streams are the engine's.
//!
//! The connection is made off the caller (the workbench is never blocked by the network): the
//! returned [`EngineIo`] queues lines until the remote command runs. What can go wrong arrives as
//! [`EngineInput::Remote`] with a [`RemoteFailure`], which keeps a refused login, a changed host
//! key, the remote command's own end and a connection lost with the command's state unknown
//! apart. The password is looked up by account on the connection thread and never reaches the
//! engine runtime; host keys are trusted on first use and pinned ([`KnownHosts`]).

use std::collections::{BTreeMap, HashMap};
use std::io;
use std::path::{Path, PathBuf};
use std::pin::Pin;
use std::sync::{Arc, Mutex};
use std::thread::JoinHandle;
use std::time::Duration;

use russh::client;
use russh::keys::{HashAlg, PrivateKeyWithHashAlg, PublicKey, PublicKeyOrCertificate};
use russh::{ChannelMsg, Disconnect, Sig};
use serde::{Deserialize, Serialize};
use tokio::sync::mpsc::{unbounded_channel, UnboundedReceiver, UnboundedSender};
use ts_rs::TS;

use super::process::Sink;
use super::runtime::{EngineInput, EngineIo, InstanceId};

/// Java `connect(null, 3000, 3000)`: TCP connection and key exchange limits.
const CONNECT: Duration = Duration::from_secs(3);
const KEY_EXCHANGE: Duration = Duration::from_secs(3);
/// Authentication and starting the command (Java waits without a limit).
const LOGIN: Duration = Duration::from_secs(15);
/// A connection that stops answering is noticed after about three keepalive intervals.
const KEEPALIVE: Duration = Duration::from_secs(10);
/// Ending: `quit`, then the channel is closed after this long (local `QUIT_GRACE`).
const QUIT_GRACE: Duration = Duration::from_secs(3);

/// The SSH connection of a remote engine (Java `EngineData` `ip` / `port` / `userName` /
/// `useKeyGen` / `keyGenPath`, or `analysis-engine-ssh-info`). The password is not part of it:
/// it is kept by the platform's credential storage under [`SshSettings::account`].
#[derive(Clone, Debug, Default, PartialEq, Eq, Hash, Serialize, Deserialize, TS)]
#[serde(default, rename_all = "camelCase")]
pub struct SshSettings {
    pub host: String,
    /// As entered; a value that is not a port number means 22 (`RemoteConnect.setPort`).
    pub port: String,
    pub user: String,
    /// 密钥 (key file) rather than 密码.
    pub use_key: bool,
    pub key_path: String,
}

impl SshSettings {
    /// `RemoteConnect.setPort`: `Integer.parseInt`, else 22.
    pub fn port_number(&self) -> u16 {
        self.port.trim().parse::<u16>().ok().filter(|p| *p != 0).unwrap_or(22)
    }

    /// The identity a password belongs to: `user@host:port`.
    pub fn account(&self) -> String {
        format!("{}@{}:{}", self.user.trim(), self.host.trim(), self.port_number())
    }

    pub fn launch(&self, command: &str) -> RemoteLaunch {
        RemoteLaunch {
            host: self.host.trim().to_string(),
            port: self.port_number(),
            user: self.user.trim().to_string(),
            key_file: self.use_key.then(|| PathBuf::from(self.key_path.trim())),
            command: command.trim().to_string(),
        }
    }
}

/// One remote command to run: where, as whom, how to log in and the command line.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct RemoteLaunch {
    pub host: String,
    pub port: u16,
    pub user: String,
    /// Key file authentication; `None` logs in with the account's password.
    pub key_file: Option<PathBuf>,
    pub command: String,
}

impl RemoteLaunch {
    pub fn account(&self) -> String {
        format!("{}@{}:{}", self.user, self.host, self.port)
    }
}

/// `EngineThreadPolicy.isExternalSshCommand`: a local command that is itself an SSH client
/// (`ssh`, `plink`), so the engine it starts runs on another host.
pub fn is_external_ssh(command: &str) -> bool {
    super::gtp::split_command(command.trim())
        .first()
        .is_some_and(|program| {
            let name = program.replace('\\', "/");
            let name = name.rsplit('/').next().unwrap_or("").to_lowercase();
            matches!(name.as_str(), "ssh" | "ssh.exe" | "plink" | "plink.exe")
        })
}

/// Why a remote engine could not start or is gone.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, TS)]
#[serde(tag = "type")]
pub enum RemoteFailure {
    /// The host could not be reached, the connection or key exchange failed or timed out, the key
    /// file could not be read or the command could not be started (`SSHController.engineFailed`).
    Connect { detail: String },
    /// The host refused the login (`SSHController.connectFailed`).
    Authentication,
    /// Password login without a password remembered for the account.
    PasswordMissing { account: String },
    /// The host presented a key other than the one trusted for it; nothing was sent to it.
    HostKeyChanged {
        host: String,
        port: u16,
        trusted: String,
        offered: String,
    },
    /// The remote command ended by itself: its exit status, or the signal that ended it.
    Ended {
        status: Option<u32>,
        signal: Option<String>,
    },
    /// The connection ended without the remote command's exit status: whether it still runs on
    /// the host is unknown.
    Lost { detail: String },
}

impl std::fmt::Display for RemoteFailure {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            RemoteFailure::Connect { detail } => write!(f, "remote connection failed: {detail}"),
            RemoteFailure::Authentication => write!(f, "remote login refused"),
            // Worded so the log sanitizer does not take the account for a password value.
            RemoteFailure::PasswordMissing { account } => write!(f, "{account} has no saved password"),
            RemoteFailure::HostKeyChanged {
                host,
                port,
                trusted,
                offered,
            } => {
                write!(
                    f,
                    "host key of {host}:{port} changed (trusted {trusted}, offered {offered})"
                )
            }
            RemoteFailure::Ended { status: Some(s), .. } => write!(f, "remote command exited with code {s}"),
            RemoteFailure::Ended { signal: Some(s), .. } => write!(f, "remote command ended by signal {s}"),
            RemoteFailure::Ended { .. } => write!(f, "remote command ended"),
            RemoteFailure::Lost { detail } => write!(f, "remote connection lost: {detail}"),
        }
    }
}

/// Looks up the password remembered for an account (`user@host:port`). Called on the connection
/// thread; it may block briefly on the platform's credential storage.
pub type PasswordSource = Arc<dyn Fn(&str) -> Option<String> + Send + Sync>;

/// Host keys trusted on first use, by `host:port`, in a file of this application's settings.
/// A different key is refused and remembered so the user can choose to trust it instead.
pub struct KnownHosts {
    path: PathBuf,
    file: Mutex<()>,
    offered: Mutex<HashMap<String, String>>,
}

fn host_key(host: &str, port: u16) -> String {
    format!("[{}]:{port}", host.to_lowercase())
}

fn fingerprint(key: &PublicKey) -> String {
    key.fingerprint(HashAlg::Sha256).to_string()
}

impl KnownHosts {
    pub fn new(path: PathBuf) -> KnownHosts {
        KnownHosts {
            path,
            file: Mutex::new(()),
            offered: Mutex::new(HashMap::new()),
        }
    }

    pub fn path(&self) -> &Path {
        &self.path
    }

    fn load(&self) -> BTreeMap<String, String> {
        std::fs::read_to_string(&self.path)
            .ok()
            .and_then(|text| serde_json::from_str(&text).ok())
            .unwrap_or_default()
    }

    fn store(&self, hosts: &BTreeMap<String, String>) -> io::Result<()> {
        if let Some(dir) = self.path.parent() {
            std::fs::create_dir_all(dir)?;
        }
        let text = serde_json::to_string_pretty(hosts).map_err(io::Error::other)?;
        crate::settings::write_atomic(&self.path, text.as_bytes())
    }

    /// Trusts `key` for `host:port` when none is trusted yet; a different trusted key refuses it.
    fn check(&self, host: &str, port: u16, key: &PublicKey) -> Result<(), RemoteFailure> {
        let _file = self.file.lock().unwrap();
        let mut hosts = self.load();
        let name = host_key(host, port);
        let offered = key
            .to_openssh()
            .map_err(|e| RemoteFailure::Connect { detail: e.to_string() })?;
        match hosts.get(&name).map(|text| PublicKey::from_openssh(text)) {
            Some(Ok(trusted)) if trusted.key_data() == key.key_data() => Ok(()),
            Some(trusted) => {
                let trusted = trusted.map_or_else(|_| "?".to_string(), |k| fingerprint(&k));
                self.offered.lock().unwrap().insert(name, offered);
                Err(RemoteFailure::HostKeyChanged {
                    host: host.to_string(),
                    port,
                    trusted,
                    offered: fingerprint(key),
                })
            }
            None => {
                hosts.insert(name, offered);
                // Not being able to remember the key does not stop this connection.
                let _ = self.store(&hosts);
                Ok(())
            }
        }
    }

    /// Trusts the key `host:port` offered when it was last refused. `false` when none was.
    pub fn trust_offered(&self, host: &str, port: u16) -> io::Result<bool> {
        let name = host_key(host, port);
        let Some(key) = self.offered.lock().unwrap().remove(&name) else {
            return Ok(false);
        };
        let _file = self.file.lock().unwrap();
        let mut hosts = self.load();
        hosts.insert(name, key);
        self.store(&hosts)?;
        Ok(true)
    }
}

/// Starts remote engine commands for the process launcher.
#[derive(Clone)]
pub struct SshConnector {
    passwords: PasswordSource,
    known_hosts: Arc<KnownHosts>,
}

enum Write {
    Line(String),
    Quit,
}

struct SshIo {
    tx: UnboundedSender<Write>,
    thread: Option<JoinHandle<()>>,
    endings: Arc<Mutex<Vec<JoinHandle<()>>>>,
}

impl SshConnector {
    pub fn new(passwords: PasswordSource, known_hosts: Arc<KnownHosts>) -> SshConnector {
        SshConnector { passwords, known_hosts }
    }

    pub(super) fn launch(
        &self,
        instance: InstanceId,
        remote: &RemoteLaunch,
        sink: Sink,
        endings: Arc<Mutex<Vec<JoinHandle<()>>>>,
    ) -> io::Result<Box<dyn EngineIo>> {
        let (tx, rx) = unbounded_channel();
        let connector = self.clone();
        let remote = remote.clone();
        let thread = std::thread::Builder::new()
            .name(format!("engine-{instance}-ssh"))
            .spawn(move || {
                let runtime = tokio::runtime::Builder::new_current_thread().enable_all().build();
                match runtime {
                    Ok(runtime) => runtime.block_on(run(connector, remote, instance, sink, rx)),
                    Err(e) => sink(
                        instance,
                        EngineInput::Remote(RemoteFailure::Connect { detail: e.to_string() }),
                    ),
                }
            })?;
        Ok(Box::new(SshIo {
            tx,
            thread: Some(thread),
            endings,
        }))
    }
}

impl SshIo {
    fn end(&mut self) {
        let Some(thread) = self.thread.take() else { return };
        let _ = self.tx.send(Write::Quit);
        let mut endings = self.endings.lock().unwrap();
        endings.retain(|h| !h.is_finished());
        endings.push(thread);
    }
}

impl EngineIo for SshIo {
    fn send(&mut self, line: &str) -> io::Result<()> {
        self.tx
            .send(Write::Line(line.to_string()))
            .map_err(|_| io::Error::new(io::ErrorKind::BrokenPipe, "engine input closed"))
    }

    fn terminate(mut self: Box<Self>) {
        self.end();
    }
}

impl Drop for SshIo {
    fn drop(&mut self) {
        self.end();
    }
}

/// Accepts the host key only when it is (or becomes) the trusted one.
struct Client {
    known_hosts: Arc<KnownHosts>,
    host: String,
    port: u16,
    refused: Arc<Mutex<Option<RemoteFailure>>>,
}

impl client::Handler for Client {
    type Error = russh::Error;

    async fn check_server_key(&mut self, key: &PublicKeyOrCertificate) -> Result<bool, Self::Error> {
        match self.known_hosts.check(&self.host, self.port, &key.public_key()) {
            Ok(()) => Ok(true),
            Err(failure) => {
                *self.refused.lock().unwrap() = Some(failure);
                Ok(false)
            }
        }
    }
}

/// Splits channel data into lines like the local process reader (`\n`, a trailing `\r` dropped).
#[derive(Default)]
struct Lines(Vec<u8>);

impl Lines {
    fn feed(&mut self, data: &[u8], mut each: impl FnMut(String)) {
        for &b in data {
            if b == b'\n' {
                while self.0.last() == Some(&b'\r') {
                    self.0.pop();
                }
                each(String::from_utf8_lossy(&self.0).into_owned());
                self.0.clear();
            } else {
                self.0.push(b);
            }
        }
    }

    fn finish(&mut self, mut each: impl FnMut(String)) {
        if !self.0.is_empty() {
            each(String::from_utf8_lossy(&self.0).into_owned());
            self.0.clear();
        }
    }
}

type Session = client::Handle<Client>;

async fn login(
    connector: &SshConnector,
    remote: &RemoteLaunch,
) -> Result<(Session, russh::Channel<client::Msg>), RemoteFailure> {
    let connect = |detail: String| RemoteFailure::Connect { detail };
    let stream = match tokio::time::timeout(
        CONNECT,
        tokio::net::TcpStream::connect((remote.host.as_str(), remote.port)),
    )
    .await
    {
        Err(_) => return Err(connect("connection timed out".into())),
        Ok(Err(e)) => return Err(connect(e.to_string())),
        Ok(Ok(stream)) => stream,
    };
    let config = Arc::new(client::Config {
        keepalive_interval: Some(KEEPALIVE),
        keepalive_max: 3,
        nodelay: true,
        ..Default::default()
    });
    let refused = Arc::new(Mutex::new(None));
    let handler = Client {
        known_hosts: connector.known_hosts.clone(),
        host: remote.host.clone(),
        port: remote.port,
        refused: refused.clone(),
    };
    let mut session = match tokio::time::timeout(KEY_EXCHANGE, client::connect_stream(config, stream, handler)).await {
        Err(_) => return Err(connect("key exchange timed out".into())),
        Ok(Err(e)) => return Err(refused.lock().unwrap().take().unwrap_or_else(|| connect(e.to_string()))),
        Ok(Ok(session)) => session,
    };
    let started = async {
        let auth = match &remote.key_file {
            Some(path) => {
                let key = russh::keys::load_secret_key(path, None)
                    .map_err(|e| connect(format!("{}: {e}", path.display())))?;
                let hash = session.best_supported_rsa_hash().await.ok().flatten().flatten();
                session
                    .authenticate_publickey(remote.user.clone(), PrivateKeyWithHashAlg::new(Arc::new(key), hash))
                    .await
            }
            None => {
                let account = remote.account();
                let Some(password) = (connector.passwords)(&account) else {
                    return Err(RemoteFailure::PasswordMissing { account });
                };
                session.authenticate_password(remote.user.clone(), password).await
            }
        };
        match auth {
            Ok(result) if result.success() => {}
            Ok(_) => return Err(RemoteFailure::Authentication),
            Err(e) => return Err(connect(e.to_string())),
        }
        let channel = session
            .channel_open_session()
            .await
            .map_err(|e| connect(e.to_string()))?;
        channel
            .exec(true, remote.command.as_bytes())
            .await
            .map_err(|e| connect(e.to_string()))?;
        Ok(channel)
    };
    let channel = match tokio::time::timeout(LOGIN, started).await {
        Err(_) => Err(connect("login timed out".into())),
        Ok(result) => result,
    };
    match channel {
        Ok(channel) => Ok((session, channel)),
        Err(failure) => {
            let _ = session.disconnect(Disconnect::ByApplication, "", "en").await;
            Err(failure)
        }
    }
}

async fn run(
    connector: SshConnector,
    remote: RemoteLaunch,
    instance: InstanceId,
    sink: Sink,
    mut rx: UnboundedReceiver<Write>,
) {
    // Lines written while connecting are sent once the command runs; ending meanwhile gives up.
    let mut queued = Vec::new();
    let connecting = login(&connector, &remote);
    tokio::pin!(connecting);
    let (session, channel) = loop {
        tokio::select! {
            result = &mut connecting => match result {
                Ok(connected) => break connected,
                Err(failure) => return sink(instance, EngineInput::Remote(failure)),
            },
            message = rx.recv() => match message {
                Some(Write::Line(line)) => queued.push(line),
                Some(Write::Quit) | None => return,
            },
        }
    };
    let (mut reader, writer) = channel.split();
    let mut failure: Option<String> = None;
    for line in queued {
        if let Err(e) = writer.data(format!("{line}\n").as_bytes()).await {
            failure.get_or_insert(e.to_string());
        }
    }
    let (mut out, mut err) = (Lines::default(), Lines::default());
    let mut ended: Option<RemoteFailure> = None;
    let mut quitting: Option<Pin<Box<tokio::time::Sleep>>> = None;
    let mut input_open = true;
    loop {
        tokio::select! {
            message = rx.recv(), if input_open => match message {
                Some(Write::Line(line)) => {
                    if let Err(e) = writer.data(format!("{line}\n").as_bytes()).await {
                        failure.get_or_insert(e.to_string());
                    }
                }
                Some(Write::Quit) | None => {
                    input_open = false;
                    let _ = writer.data(&b"quit\n"[..]).await;
                    let _ = writer.eof().await;
                    quitting = Some(Box::pin(tokio::time::sleep(QUIT_GRACE)));
                }
            },
            message = reader.wait() => match message {
                Some(ChannelMsg::Data { data }) => out.feed(&data, |l| sink(instance, EngineInput::Stdout(l))),
                Some(ChannelMsg::ExtendedData { data, ext: 1 }) => err.feed(&data, |l| sink(instance, EngineInput::Stderr(l))),
                Some(ChannelMsg::ExitStatus { exit_status }) => {
                    ended = Some(RemoteFailure::Ended { status: Some(exit_status), signal: None });
                }
                Some(ChannelMsg::ExitSignal { signal_name, .. }) => {
                    ended = Some(RemoteFailure::Ended { status: None, signal: Some(format!("{signal_name:?}")) });
                }
                Some(ChannelMsg::Close) | None => break,
                Some(_) => {}
            },
            _ = async { quitting.as_mut().unwrap().await }, if quitting.is_some() => {
                // The command did not end after `quit`: stop it (where the server honours
                // signals) and close the channel, which ends its input and output.
                let _ = writer.signal(Sig::KILL).await;
                let _ = writer.close().await;
                break;
            }
        }
    }
    out.finish(|l| sink(instance, EngineInput::Stdout(l)));
    err.finish(|l| sink(instance, EngineInput::Stderr(l)));
    let end = ended.unwrap_or_else(|| RemoteFailure::Lost {
        detail: failure.unwrap_or_else(|| "the connection closed".into()),
    });
    sink(instance, EngineInput::Remote(end));
    let _ = session.disconnect(Disconnect::ByApplication, "", "en").await;
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_port_that_is_not_a_number_is_22() {
        let mut s = SshSettings {
            host: "h".into(),
            user: "u".into(),
            ..Default::default()
        };
        for (port, expected) in [
            ("", 22),
            ("x", 22),
            ("2222", 2222),
            (" 23 ", 23),
            ("0", 22),
            ("70000", 22),
        ] {
            s.port = port.into();
            assert_eq!(s.port_number(), expected, "{port:?}");
        }
        assert_eq!(s.account(), "u@h:22");
    }
}
