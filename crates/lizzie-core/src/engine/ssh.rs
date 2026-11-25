//! SSH remote engines (Java `SSHController` / `AnalysisEngineSSHController` over ganymed-ssh2):
//! an engine command runs on an SSH host and its standard streams are the engine's.
//!
//! The connection is made off the caller (the workbench is never blocked by the network): the
//! returned [`EngineIo`] queues lines until the remote command runs. What can go wrong arrives as
//! [`EngineInput::Remote`] with a [`RemoteFailure`], which keeps a refused login, a changed host
//! key, the remote command's own end and a connection lost with the command's state unknown
//! apart. The password is looked up by account on the connection thread and never reaches the
//! engine runtime; host keys are trusted on first use and pinned ([`KnownHosts`]).

use std::borrow::Cow;
use std::collections::{BTreeMap, HashMap};
use std::io;
use std::path::{Path, PathBuf};
use std::pin::Pin;
use std::sync::{Arc, LazyLock, Mutex};
use std::thread::JoinHandle;
use std::time::Duration;

use russh::keys::{HashAlg, PrivateKeyWithHashAlg, PublicKey, PublicKeyOrCertificate};
use russh::{client, kex, mac, Preferred};
use russh::{ChannelMsg, Disconnect, Sig};
use serde::{Deserialize, Serialize};
use tokio::io::AsyncWriteExt;
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

/// russh's default key exchanges are modern only. The ganymed-ssh2 client the Java program used
/// before could only use SHA-1 key exchange, so servers set up for it keep working through these
/// two, offered after every default one: a server that supports any default still agrees on the
/// same one (Java `SshCommandSession.withLegacyKeyExchangeFallbacks`). Not group1, whose 1024-bit
/// group is too weak.
static KEY_EXCHANGES: LazyLock<Vec<kex::Name>> = LazyLock::new(|| {
    let defaults = &Preferred::DEFAULT.kex;
    // The defaults end with pseudo-algorithms that announce extensions, not ways to exchange keys.
    let extensions = [
        kex::EXTENSION_SUPPORT_AS_CLIENT,
        kex::EXTENSION_SUPPORT_AS_SERVER,
        kex::EXTENSION_OPENSSH_STRICT_KEX_AS_CLIENT,
        kex::EXTENSION_OPENSSH_STRICT_KEX_AS_SERVER,
    ];
    let end = defaults
        .iter()
        .position(|k| extensions.contains(k))
        .unwrap_or(defaults.len());
    let mut names = defaults[..end].to_vec();
    names.extend(
        [kex::DH_G14_SHA1, kex::DH_GEX_SHA1]
            .into_iter()
            .filter(|k| !defaults.contains(k)),
    );
    names.extend_from_slice(&defaults[end..]);
    names
});

/// russh's default message authentication codes leave out SHA-1; the Java program's SSH library
/// (Apache MINA SSHD) offers `hmac-sha1-etm@openssh.com` and `hmac-sha1` after its SHA-2 ones. So a
/// server whose only shared choice is SHA-1, with a cipher that needs a MAC, is reached as in Java,
/// and a server that supports any default still agrees on the same one. No other algorithm is added
/// (no CBC ciphers).
static MACS: LazyLock<Vec<mac::Name>> = LazyLock::new(|| {
    let defaults = &Preferred::DEFAULT.mac;
    let mut names = defaults.to_vec();
    names.extend(
        [mac::HMAC_SHA1_ETM, mac::HMAC_SHA1]
            .into_iter()
            .filter(|m| !defaults.contains(m)),
    );
    names
});

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
    /// The file of trusted host keys exists but could not be read or is not valid: nothing was
    /// sent to the host and the file was left as it is (Java `SshKnownHosts` refuses the key too).
    KnownHostsUnreadable {
        host: String,
        port: u16,
        path: String,
        detail: String,
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
            RemoteFailure::KnownHostsUnreadable {
                host,
                port,
                path,
                detail,
            } => write!(
                f,
                "refused {host}:{port}: trusted host keys in {path} could not be read: {detail}"
            ),
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

    /// The trusted keys by `host:port`: none while the file does not exist yet; an error when it
    /// exists but cannot be read or parsed, so that it is never taken for empty and overwritten.
    fn load(&self) -> io::Result<BTreeMap<String, String>> {
        match std::fs::read_to_string(&self.path) {
            Ok(text) => serde_json::from_str(&text).map_err(|e| io::Error::new(io::ErrorKind::InvalidData, e)),
            Err(e) if e.kind() == io::ErrorKind::NotFound => Ok(BTreeMap::new()),
            Err(e) => Err(e),
        }
    }

    fn store(&self, hosts: &BTreeMap<String, String>) -> io::Result<()> {
        if let Some(dir) = self.path.parent() {
            std::fs::create_dir_all(dir)?;
        }
        let text = serde_json::to_string_pretty(hosts).map_err(io::Error::other)?;
        crate::settings::write_atomic(&self.path, text.as_bytes())
    }

    /// Trusts `key` for `host:port` when none is trusted yet; a different trusted key refuses it,
    /// and so does a file of trusted keys that cannot be read.
    fn check(&self, host: &str, port: u16, key: &PublicKey) -> Result<(), RemoteFailure> {
        let _file = self.file.lock().unwrap();
        let mut hosts = self.load().map_err(|e| RemoteFailure::KnownHostsUnreadable {
            host: host.to_string(),
            port,
            path: self.path.display().to_string(),
            detail: e.to_string(),
        })?;
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

    /// Trusts the key `host:port` offered when it was last refused, if it is the one with the
    /// fingerprint `shown` the user was shown. `true` as well when that key is trusted already
    /// (another refusal of it was confirmed); `false` when another key was offered since: that
    /// one has its own refusal to confirm. A file of trusted keys that cannot be read is an error
    /// and is left as it is.
    pub fn trust_offered(&self, host: &str, port: u16, shown: &str) -> io::Result<bool> {
        let has_shown = |text: &String| PublicKey::from_openssh(text).is_ok_and(|key| fingerprint(&key) == shown);
        let name = host_key(host, port);
        let _file = self.file.lock().unwrap();
        let mut hosts = self.load()?;
        if hosts.get(&name).is_some_and(has_shown) {
            return Ok(true);
        }
        let mut offered = self.offered.lock().unwrap();
        if !offered.get(&name).is_some_and(has_shown) {
            return Ok(false);
        }
        hosts.insert(name.clone(), offered.remove(&name).unwrap());
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
        preferred: Preferred {
            kex: Cow::Borrowed(KEY_EXCHANGES.as_slice()),
            mac: Cow::Borrowed(MACS.as_slice()),
            ..Preferred::default()
        },
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
    // Input goes out on its own: a command that stops reading fills the channel window, and
    // waiting for room must hold up neither its output nor the end of the quit grace period.
    let (input, mut lines) = unbounded_channel::<String>();
    let mut stdin = writer.make_writer();
    let sending = async move {
        while let Some(line) = lines.recv().await {
            stdin.write_all(line.as_bytes()).await?;
        }
        // `quit` was the last line: end the command's input.
        stdin.shutdown().await
    };
    tokio::pin!(sending);
    let mut input = Some(input);
    for line in queued {
        let _ = input.as_ref().unwrap().send(format!("{line}\n"));
    }
    let mut failure: Option<String> = None;
    let mut sent = false;
    let (mut out, mut err) = (Lines::default(), Lines::default());
    let mut ended: Option<RemoteFailure> = None;
    let mut quitting: Option<Pin<Box<tokio::time::Sleep>>> = None;
    loop {
        tokio::select! {
            message = rx.recv(), if input.is_some() => match message {
                // Unsent when the input already failed; that failure is reported.
                Some(Write::Line(line)) => { let _ = input.as_ref().unwrap().send(format!("{line}\n")); }
                Some(Write::Quit) | None => {
                    let _ = input.take().unwrap().send("quit\n".into());
                    quitting = Some(Box::pin(tokio::time::sleep(QUIT_GRACE)));
                }
            },
            result = &mut sending, if !sent => {
                sent = true;
                if let Err(e) = result {
                    failure.get_or_insert(e.to_string());
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
                // The command did not end after `quit` (or never took it): stop it (where the
                // server honours signals) and close the channel, which ends its input and output.
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

    /// The defaults keep their order and come first, so a server that supports one of them agrees
    /// on what it did before; the SHA-1 fallbacks follow (key exchanges before the extension names).
    #[test]
    fn sha1_fallbacks_come_after_every_default_algorithm() {
        fn names<N: AsRef<str>>(list: &[N]) -> Vec<String> {
            list.iter().map(|n| n.as_ref().to_string()).collect()
        }
        let mut expected = names(&Preferred::DEFAULT.kex);
        let extensions = expected.iter().position(|n| n == "ext-info-c").unwrap();
        expected.splice(
            extensions..extensions,
            [
                "diffie-hellman-group14-sha1".to_string(),
                "diffie-hellman-group-exchange-sha1".to_string(),
            ],
        );
        assert_eq!(names(&KEY_EXCHANGES), expected);
        assert!(!expected.iter().any(|n| n == "diffie-hellman-group1-sha1"));

        let mut expected = names(&Preferred::DEFAULT.mac);
        assert!(
            !expected.iter().any(|n| n.contains("sha1")),
            "russh's defaults changed: {expected:?}"
        );
        expected.extend(["hmac-sha1-etm@openssh.com".to_string(), "hmac-sha1".to_string()]);
        assert_eq!(names(&MACS), expected);
    }

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

    #[test]
    fn only_the_key_the_user_was_shown_can_be_trusted() {
        let key = |s: &str| PublicKey::from_openssh(s).unwrap();
        let a = key("ssh-ed25519 AAAAC3NzaC1lZDI1NTE5AAAAIC86sYXFL25W92Pzq2/xqryr7bk/CTjx5Rxp/fBxiY9t");
        let b = key("ssh-ed25519 AAAAC3NzaC1lZDI1NTE5AAAAIJ9gRqlnqviY7yC5fr94J5rkV4YBWbqfPn4+LEd6xZx+");
        let c = key("ssh-ed25519 AAAAC3NzaC1lZDI1NTE5AAAAIKbyUPvA2pdfmYUBTAVa7GMg4mfIL0gFQ3D0T5PUJSbB");
        let dir = tempfile::tempdir().unwrap();
        let hosts = KnownHosts::new(dir.path().join("hosts.json"));
        hosts.check("h", 22, &a).unwrap();
        // Two refusals in a row: B is shown first, then another connection is offered C.
        let Err(RemoteFailure::HostKeyChanged { offered: shown_b, .. }) = hosts.check("h", 22, &b) else {
            panic!()
        };
        let Err(RemoteFailure::HostKeyChanged { offered: shown_c, .. }) = hosts.check("h", 22, &c) else {
            panic!()
        };
        assert!(
            !hosts.trust_offered("h", 22, &shown_b).unwrap(),
            "B's confirmation does not trust C"
        );
        assert!(hosts.check("h", 22, &a).is_ok(), "A is still the trusted key");
        assert!(hosts.trust_offered("h", 22, &shown_c).unwrap());
        assert!(hosts.check("h", 22, &c).is_ok() && hosts.check("h", 22, &a).is_err());
    }
}
