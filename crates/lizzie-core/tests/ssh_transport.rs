//! A remote engine's SSH connection through the process launcher against an in-process SSH
//! server: a server that offers only SHA-1 key exchange, or only a SHA-1 MAC with a cipher that
//! needs one, is still reached, and a host key other than the trusted one is refused before any
//! credential is sent, until the user trusts it. A file of trusted keys that cannot be read
//! refuses every key and is left as it is.

use std::borrow::Cow;
use std::net::TcpListener;
use std::sync::mpsc::{channel, Receiver};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use lizzie_core::engine::gtp::LaunchSpec;
use lizzie_core::engine::process::ProcessLauncher;
use lizzie_core::engine::runtime::{EngineInput, Launcher};
use lizzie_core::engine::ssh::{KnownHosts, RemoteFailure, SshConnector, SshSettings};
use russh::keys::ssh_key::private::Ed25519Keypair;
use russh::keys::{HashAlg, PrivateKey};
use russh::server::{self, Auth, Msg, Session};
use russh::{cipher, kex, mac, Channel, ChannelId, Preferred};

const COMMAND: &str = "katago gtp -model m.bin.gz";

/// What the server saw of one connection.
#[derive(Default)]
struct Seen {
    /// Passwords and public keys offered, in order.
    credentials: Vec<String>,
    commands: Vec<String>,
}

struct Server {
    seen: Arc<Mutex<Seen>>,
    channels: Vec<Channel<Msg>>,
}

impl server::Handler for Server {
    type Error = russh::Error;

    async fn auth_password(&mut self, _: &str, password: &str) -> Result<Auth, Self::Error> {
        self.seen
            .lock()
            .unwrap()
            .credentials
            .push(format!("password {password}"));
        Ok(Auth::Accept)
    }

    async fn auth_publickey_offered(&mut self, _: &str, _: server::PublicKeyOffer<'_>) -> Result<Auth, Self::Error> {
        self.seen.lock().unwrap().credentials.push("public key".into());
        Ok(Auth::reject())
    }

    async fn channel_open_session(
        &mut self,
        channel: Channel<Msg>,
        reply: server::ChannelOpenHandle,
        _: &mut Session,
    ) -> Result<(), Self::Error> {
        self.channels.push(channel);
        reply.accept().await;
        Ok(())
    }

    async fn exec_request(
        &mut self,
        channel: ChannelId,
        data: &[u8],
        session: &mut Session,
    ) -> Result<(), Self::Error> {
        self.seen
            .lock()
            .unwrap()
            .commands
            .push(String::from_utf8_lossy(data).into_owned());
        session.channel_success(channel)?;
        session.data(channel, &b"= ready\n\n"[..])?;
        Ok(())
    }
}

fn host_key(seed: u8) -> PrivateKey {
    Ed25519Keypair::from_seed(&[seed; 32]).into()
}

/// Serves the next connection to `listener` on its own thread, with `key` and only the algorithms
/// `preferred`.
fn serve(listener: &TcpListener, key: PrivateKey, preferred: Preferred) -> Arc<Mutex<Seen>> {
    let seen = Arc::new(Mutex::new(Seen::default()));
    let handler = Server {
        seen: seen.clone(),
        channels: Vec::new(),
    };
    let config = Arc::new(server::Config {
        keys: vec![key],
        auth_rejection_time: Duration::ZERO,
        preferred,
        ..Default::default()
    });
    let listener = listener.try_clone().unwrap();
    std::thread::spawn(move || {
        let (stream, _) = listener.accept().unwrap();
        stream.set_nonblocking(true).unwrap();
        let runtime = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .unwrap();
        runtime.block_on(async move {
            let stream = tokio::net::TcpStream::from_std(stream).unwrap();
            if let Ok(session) = server::run_stream(config, stream, handler).await {
                let _ = session.await;
            }
        });
    });
    seen
}

struct Remote {
    launcher: ProcessLauncher,
    inputs: Receiver<EngineInput>,
    known_hosts: Arc<KnownHosts>,
    settings: SshSettings,
}

impl Remote {
    fn new(dir: &std::path::Path, port: u16) -> Remote {
        let (tx, inputs) = channel();
        let tx = Mutex::new(tx);
        let known_hosts = Arc::new(KnownHosts::new(dir.join("ssh-known-hosts.json")));
        let launcher = ProcessLauncher::new(Arc::new(move |_, input| {
            let _ = tx.lock().unwrap().send(input);
        }))
        .with_ssh(SshConnector::new(
            Arc::new(|_| Some("secret".into())),
            known_hosts.clone(),
        ));
        Remote {
            launcher,
            inputs,
            known_hosts,
            settings: SshSettings {
                host: "127.0.0.1".into(),
                port: port.to_string(),
                user: "go".into(),
                ..SshSettings::default()
            },
        }
    }

    /// Starts the engine command and returns the first thing that comes back.
    fn start(&self, id: u64) -> EngineInput {
        let io = self
            .launcher
            .launch(id, &LaunchSpec::remote(COMMAND, &self.settings))
            .unwrap();
        let first = self
            .inputs
            .recv_timeout(Duration::from_secs(20))
            .expect("an answer from the connection");
        io.terminate();
        self.launcher.waiter().wait(Duration::from_secs(5));
        while self.inputs.try_recv().is_ok() {}
        first
    }
}

fn listen() -> (TcpListener, u16, tempfile::TempDir) {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let port = listener.local_addr().unwrap().port();
    (listener, port, tempfile::tempdir().unwrap())
}

fn only_kex(kex: &'static [kex::Name]) -> Preferred {
    Preferred {
        kex: Cow::Borrowed(kex),
        ..Preferred::default()
    }
}

fn only_mac(mac: &'static [mac::Name]) -> Preferred {
    Preferred {
        cipher: Cow::Borrowed(&[cipher::AES_128_CTR]),
        mac: Cow::Borrowed(mac),
        ..Preferred::default()
    }
}

#[test]
fn a_server_offering_only_sha1_key_exchange_or_mac_runs_the_engine_command() {
    let (listener, port, dir) = listen();
    let remote = Remote::new(dir.path(), port);
    for only in [
        only_kex(&[kex::DH_G14_SHA1]),
        only_kex(&[kex::DH_GEX_SHA1]),
        only_mac(&[mac::HMAC_SHA1]),
        only_mac(&[mac::HMAC_SHA1_ETM]),
    ] {
        let described = format!("{:?} {:?}", only.kex, only.mac);
        let seen = serve(&listener, host_key(1), only);
        assert_eq!(remote.start(1), EngineInput::Stdout("= ready".into()), "{described}");
        let seen = seen.lock().unwrap();
        assert_eq!(seen.credentials, ["password secret"]);
        assert_eq!(seen.commands, [COMMAND]);
    }
}

#[test]
fn a_changed_host_key_is_refused_before_any_credential_until_it_is_trusted() {
    let (listener, port, dir) = listen();
    let remote = Remote::new(dir.path(), port);

    // First use trusts the key.
    serve(&listener, host_key(1), Preferred::default());
    assert_eq!(remote.start(1), EngineInput::Stdout("= ready".into()));

    // Another key, also from servers that would only agree to SHA-1 algorithms.
    for preferred in [
        Preferred::default(),
        only_kex(&[kex::DH_G14_SHA1]),
        only_mac(&[mac::HMAC_SHA1]),
    ] {
        let seen = serve(&listener, host_key(2), preferred);
        let EngineInput::Remote(RemoteFailure::HostKeyChanged { offered, .. }) = remote.start(2) else {
            panic!("a changed key must be refused")
        };
        assert_eq!(
            offered,
            host_key(2).public_key().fingerprint(HashAlg::Sha256).to_string()
        );
        let seen = seen.lock().unwrap();
        assert!(
            seen.credentials.is_empty() && seen.commands.is_empty(),
            "{:?}",
            seen.credentials
        );
    }

    // 信任新密钥并重试 trusts the key shown, and the next connection logs in.
    let offered = host_key(2).public_key().fingerprint(HashAlg::Sha256).to_string();
    assert!(remote.known_hosts.trust_offered("127.0.0.1", port, &offered).unwrap());
    let seen = serve(&listener, host_key(2), Preferred::default());
    assert_eq!(remote.start(3), EngineInput::Stdout("= ready".into()));
    assert_eq!(seen.lock().unwrap().credentials, ["password secret"]);
}

#[test]
fn a_file_of_trusted_keys_that_cannot_be_read_refuses_the_connection_and_is_left_as_it_is() {
    let (listener, port, dir) = listen();
    let remote = Remote::new(dir.path(), port);
    let path = remote.known_hosts.path().to_path_buf();

    // No file yet: the first key is trusted and written down.
    serve(&listener, host_key(1), Preferred::default());
    assert_eq!(remote.start(1), EngineInput::Stdout("= ready".into()));
    let trusted = std::fs::read_to_string(&path).unwrap();
    assert!(trusted.contains(&format!("[127.0.0.1]:{port}")), "{trusted}");

    let refuses = |id: u64| {
        let before = std::fs::read(&path);
        let seen = serve(&listener, host_key(2), Preferred::default());
        let EngineInput::Remote(RemoteFailure::KnownHostsUnreadable { path: shown, .. }) = remote.start(id) else {
            panic!("an unreadable file of trusted keys must refuse the key")
        };
        assert_eq!(shown, path.display().to_string());
        let seen = seen.lock().unwrap();
        assert!(
            seen.credentials.is_empty() && seen.commands.is_empty(),
            "{:?}",
            seen.credentials
        );
        // 信任新密钥并重试 does not replace it either.
        let offered = host_key(2).public_key().fingerprint(HashAlg::Sha256).to_string();
        assert!(remote.known_hosts.trust_offered("127.0.0.1", port, &offered).is_err());
        assert_eq!(std::fs::read(&path).ok(), before.ok(), "the file is left as it was");
    };

    // Not valid JSON (a damaged or hand-edited file): nothing is sent and nothing is written.
    std::fs::write(&path, b"{\"[127.0.0.1]:22\": \"ssh-ed25519 AAAA").unwrap();
    refuses(2);
    std::fs::write(&path, b"").unwrap();
    refuses(3);

    // A file that cannot be read at all.
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        std::fs::write(&path, &trusted).unwrap();
        std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o000)).unwrap();
        // Skipped where permissions do not stop reading (running as root).
        if std::fs::read(&path).is_err() {
            let seen = serve(&listener, host_key(2), Preferred::default());
            let EngineInput::Remote(RemoteFailure::KnownHostsUnreadable { .. }) = remote.start(4) else {
                panic!("an unreadable file of trusted keys must refuse the key")
            };
            assert!(seen.lock().unwrap().credentials.is_empty());
            let offered = host_key(2).public_key().fingerprint(HashAlg::Sha256).to_string();
            assert!(remote.known_hosts.trust_offered("127.0.0.1", port, &offered).is_err());
        }
        std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o600)).unwrap();
        assert_eq!(std::fs::read_to_string(&path).unwrap(), trusted);
    }

    // The file restored: the trusted key logs in again and a changed one is refused as before.
    std::fs::write(&path, &trusted).unwrap();
    let seen = serve(&listener, host_key(1), Preferred::default());
    assert_eq!(remote.start(5), EngineInput::Stdout("= ready".into()));
    assert_eq!(seen.lock().unwrap().credentials, ["password secret"]);
    serve(&listener, host_key(2), Preferred::default());
    assert!(matches!(
        remote.start(6),
        EngineInput::Remote(RemoteFailure::HostKeyChanged { .. })
    ));
}
