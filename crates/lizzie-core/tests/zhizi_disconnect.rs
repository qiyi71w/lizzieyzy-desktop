//! 断开智子引擎 through the public `Workbench` operations. 智子 engines launch through the product
//! `CloudConnector` and talk to a loopback 智子 service scripted here (Socket.IO readiness, then a
//! GTP engine over `stdin` / `stdout` events) that records every connection and its closing;
//! local and 自建算力 engines are controlled GTP processes. Independent expectations: Java
//! `ZhiziEngineConnections` (targets, state, prepare), `EngineManager.zhiziSelectionBusy` /
//! `disconnectZhiziEngines` (selection rolled back to no current engine) and
//! `RemoteComputeEngineSwitch.cancelZhizi` with `Leelaz.markZhiziDisconnected` (a request or
//! callback already under way connects nothing again).

use std::net::TcpListener;
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::sync::{mpsc, Arc, Mutex};
use std::thread::{self, JoinHandle};
use std::time::{Duration, Instant};

use lizzie_core::api::{CommandResult, EngineState, MatchCommand, MatchSessionSettings, ZhiziConnection};
use lizzie_core::cloud::{CloudSettings, Provider};
use lizzie_core::engine::cloud::{CloudConnector, CloudLaunch, CloudProvider, SocketCredentials};
use lizzie_core::engine::gtp::LaunchSpec;
use lizzie_core::engine::profile::{EngineProfile, EngineStartup};
use lizzie_core::engine::runtime::{EngineInput, EngineIo, InstanceId, Launcher};
use lizzie_core::settings::{ExtraMode, MatchSettings, Storage};
use lizzie_core::workbench::{Workbench, ZHIZI_ACTIVATION_RETIRED};
use serde_json::Value;
use tokio_tungstenite::tungstenite::{accept, Message};

const KATA: &str = "name\nversion\nlist_commands\nkomi\nboardsize\nclear_board\nplay\nundo\nkata-analyze\nkata-set-rules\nkata-get-rules\ngenmove\nstop";
const RULES: &str = r#"{"friendlyPassOk":true,"hasButton":false,"ko":"SIMPLE","scoring":"AREA","suicide":false,"tax":"NONE","whiteHandicapBonus":"N"}"#;
const WAIT: Duration = Duration::from_secs(8);

/// The answer of a GTP engine to `cmd` (numbered `n`): identification, rules, an analysis
/// stream's header; `genmove` is never answered, so an engine game keeps waiting for its move.
/// Each answer starts with the blank line that ends an analysis stream still running.
fn gtp_answer(n: &str, cmd: &str) -> Option<String> {
    let reply = match cmd.split(' ').next().unwrap() {
        "name" => "KataGo",
        "version" => "1.16.4",
        "list_commands" => KATA,
        "kata-get-rules" => RULES,
        "genmove" => return None,
        _ => "",
    };
    Some(if cmd.starts_with("kata-analyze") { format!("\n={n} \n") } else { format!("\n={n} {reply}\n\n") })
}

/// A loopback 智子 service. Each engine is one Socket.IO connection; `open` counts those not
/// closed yet. While `hold` is set, the engine answers nothing, so it stays starting.
struct Service {
    address: String,
    connections: Arc<AtomicUsize>,
    open: Arc<AtomicUsize>,
    hold: Arc<AtomicBool>,
}

fn service() -> Service {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let address = format!("http://{}", listener.local_addr().unwrap());
    let connections = Arc::new(AtomicUsize::new(0));
    let open = Arc::new(AtomicUsize::new(0));
    let hold = Arc::new(AtomicBool::new(false));
    let (count, live, held) = (connections.clone(), open.clone(), hold.clone());
    thread::spawn(move || {
        for stream in listener.incoming() {
            let Ok(stream) = stream else { return };
            count.fetch_add(1, Ordering::SeqCst);
            live.fetch_add(1, Ordering::SeqCst);
            let (live, held) = (live.clone(), held.clone());
            thread::spawn(move || {
                serve_engine(stream, &held);
                live.fetch_sub(1, Ordering::SeqCst);
            });
        }
    });
    Service { address, connections, open, hold }
}

fn serve_engine(stream: std::net::TcpStream, hold: &AtomicBool) {
    let Ok(mut ws) = accept(stream) else { return };
    let handshake = "0{\"sid\":\"fixture\",\"upgrades\":[],\"pingInterval\":25000,\"pingTimeout\":20000}";
    if ws.send(Message::Text(handshake.into())).is_err() { return; }
    loop {
        match ws.read() {
            Ok(Message::Text(text)) if text.as_str() == "40" => break,
            Ok(_) => continue,
            Err(_) => return,
        }
    }
    for packet in ["40{\"sid\":\"fixture\"}", "42[\"ready\"]"] {
        if ws.send(Message::Text(packet.into())).is_err() { return; }
    }
    loop {
        let text = match ws.read() {
            Ok(Message::Text(text)) => text.to_string(),
            Ok(Message::Close(_)) | Err(_) => return,
            Ok(_) => continue,
        };
        let Some(event) = text.strip_prefix("42").and_then(|e| serde_json::from_str::<Value>(e).ok()) else { continue };
        if event[0] != "stdin" { continue; }
        while hold.load(Ordering::SeqCst) {
            thread::sleep(Duration::from_millis(5));
        }
        for line in event[1].as_str().unwrap_or_default().lines() {
            let Some((n, cmd)) = line.split_once(' ') else { continue };
            let Some(answer) = gtp_answer(n, cmd) else { continue };
            let out = serde_json::to_string(&serde_json::json!(["stdout", answer])).unwrap();
            if ws.send(Message::Text(format!("42{out}").into())).is_err() { return; }
        }
    }
}

/// Local and 自建算力 processes: lines not yet answered and whether each was ended.
#[derive(Clone, Default)]
struct World(Arc<Mutex<Vec<(InstanceId, Vec<String>, bool)>>>);

impl World {
    fn take(&self, id: InstanceId) -> Vec<String> {
        let mut w = self.0.lock().unwrap();
        w.iter_mut().find(|p| p.0 == id).map(|p| std::mem::take(&mut p.1)).unwrap_or_default()
    }
    fn ids(&self) -> Vec<InstanceId> {
        self.0.lock().unwrap().iter().map(|p| p.0).collect()
    }
    fn ended(&self, id: InstanceId) -> bool {
        self.0.lock().unwrap().iter().any(|p| p.0 == id && p.2)
    }
}

struct Io(World, InstanceId);
impl EngineIo for Io {
    fn send(&mut self, line: &str) -> std::io::Result<()> {
        self.0 .0.lock().unwrap().iter_mut().find(|p| p.0 == self.1).unwrap().1.push(line.into());
        Ok(())
    }
    fn terminate(self: Box<Self>) {
        self.0 .0.lock().unwrap().iter_mut().find(|p| p.0 == self.1).unwrap().2 = true;
    }
}

/// 智子 profiles through the product connector, every other one as a controlled process.
struct Mixed {
    connector: CloudConnector,
    tx: Mutex<mpsc::Sender<(InstanceId, EngineInput)>>,
    endings: Arc<Mutex<Vec<JoinHandle<()>>>>,
    zhizi: Arc<Mutex<Vec<InstanceId>>>,
    world: World,
}

impl Launcher for Mixed {
    fn launch(&self, id: InstanceId, spec: &LaunchSpec) -> std::io::Result<Box<dyn EngineIo>> {
        match &spec.cloud {
            Some(cloud) if cloud.provider == CloudProvider::Zhizi => {
                self.zhizi.lock().unwrap().push(id);
                let tx = Mutex::new(self.tx.lock().unwrap().clone());
                self.connector.launch(id, cloud, Arc::new(move |i, e| { let _ = tx.lock().unwrap().send((i, e)); }), self.endings.clone())
            }
            _ => {
                self.world.0.lock().unwrap().push((id, Vec::new(), false));
                Ok(Box::new(Io(self.world.clone(), id)))
            }
        }
    }
}

struct Bench {
    wb: Workbench,
    world: World,
    service: Service,
    zhizi: Arc<Mutex<Vec<InstanceId>>>,
    events: mpsc::Receiver<(InstanceId, EngineInput)>,
    endings: Arc<Mutex<Vec<JoinHandle<()>>>>,
    _dir: tempfile::TempDir,
}

fn zhizi_profile(id: &str) -> EngineProfile {
    EngineProfile {
        id: id.into(),
        name: format!("智子 {id}"),
        command: "remote-compute://zhizi".into(),
        cloud: Some(CloudLaunch { provider: CloudProvider::Zhizi, identifier: "fixture-account".into(), args: String::new(), url: String::new() }),
        ..EngineProfile::default()
    }
}

fn local_profile(id: &str) -> EngineProfile {
    EngineProfile { id: id.into(), name: format!("Local {id}"), command: "katago gtp".into(), ..EngineProfile::default() }
}

fn custom_profile(id: &str) -> EngineProfile {
    EngineProfile {
        id: id.into(),
        name: "自建算力".into(),
        command: "remote-compute://custom-websocket".into(),
        cloud: Some(CloudLaunch { provider: CloudProvider::Custom, identifier: String::new(), args: String::new(), url: "ws://127.0.0.1:9/analysis".into() }),
        ..EngineProfile::default()
    }
}

fn zhizi_settings() -> CloudSettings {
    CloudSettings { provider: Provider::Zhizi, identifier: "fixture-account".into(), ..CloudSettings::default() }
}

impl Bench {
    fn new(profiles: Vec<EngineProfile>) -> Bench {
        let dir = tempfile::tempdir().unwrap();
        let service = service();
        let socket_url = service.address.clone();
        let (tx, events) = mpsc::channel();
        let endings = Arc::new(Mutex::new(Vec::new()));
        let zhizi = Arc::new(Mutex::new(Vec::new()));
        let world = World::default();
        let launcher = Mixed {
            connector: CloudConnector::new(Arc::new(move |_, _| {
                Ok(SocketCredentials { socket_url: socket_url.clone(), token: "fixture-token".into() })
            })),
            tx: Mutex::new(tx),
            endings: endings.clone(),
            zhizi: zhizi.clone(),
            world: world.clone(),
        };
        let mut wb = Workbench::new(
            Storage::in_dirs(&dir.path().join("config"), &dir.path().join("data")),
            None,
            Box::new(launcher),
            Vec::new(),
        );
        wb.set_auto_quick_analyze(false).unwrap();
        if !profiles.is_empty() {
            wb.save_engine_settings(profiles, EngineStartup::Default).unwrap();
        }
        Bench { wb, world, service, zhizi, events, endings, _dir: dir }
    }

    /// Answers the controlled processes and feeds the 智子 engines' events until `done` holds.
    fn until(&mut self, done: impl FnMut(&mut Workbench) -> bool) -> bool {
        self.until_within(WAIT, done)
    }

    fn until_within(&mut self, limit: Duration, mut done: impl FnMut(&mut Workbench) -> bool) -> bool {
        let end = Instant::now() + limit;
        loop {
            for id in self.world.ids() {
                for line in self.world.take(id) {
                    let Some((n, cmd)) = line.split_once(' ') else { continue };
                    let Some(answer) = gtp_answer(n, cmd) else { continue };
                    for out in answer.trim_end_matches('\n').lines() {
                        self.wb.engine_input(id, EngineInput::Stdout(out.to_string()));
                    }
                    if !cmd.starts_with("kata-analyze") {
                        self.wb.engine_input(id, EngineInput::Stdout(String::new()));
                    }
                }
            }
            if done(&mut self.wb) {
                return true;
            }
            if Instant::now() >= end {
                return false;
            }
            if let Ok((id, e)) = self.events.recv_timeout(Duration::from_millis(10)) {
                self.wb.engine_input(id, e);
            }
        }
    }

    fn connected(&mut self) -> ZhiziConnection {
        self.wb.view(false).engine.zhizi
    }

    /// Waits until the service has `open` engine connections left.
    fn open(&mut self, open: usize) -> bool {
        let counter = self.service.open.clone();
        self.until(|_| counter.load(Ordering::SeqCst) == open)
    }

    fn connections(&self) -> usize {
        self.service.connections.load(Ordering::SeqCst)
    }

    fn last_zhizi(&self) -> InstanceId {
        *self.zhizi.lock().unwrap().last().unwrap()
    }

    fn state_of(&mut self, index: usize) -> Option<EngineState> {
        self.wb.view(false).engine.engines[index].state
    }

    fn ready(&mut self, index: usize) -> bool {
        self.until(|wb| wb.view(false).engine.engines[index].state == Some(EngineState::Ready))
    }

    /// Waits until the engine at `index` holds the foreground seat and confirmed its board sync:
    /// a remote compute switch to it is complete.
    fn switched(&mut self, index: usize) -> bool {
        self.until(|wb| wb.view(false).engine.foreground.is_some_and(|f| f.index as usize == index && f.ready))
    }
}

impl Drop for Bench {
    fn drop(&mut self) {
        self.service.hold.store(false, Ordering::SeqCst);
        self.wb.end_engines();
        for handle in std::mem::take(&mut *self.endings.lock().unwrap()) {
            let _ = handle.join();
        }
    }
}

/// 智子 is the foreground engine and a local engine runs kept by fast engine change: the 智子
/// connection closes, the foreground seat is left empty (no current engine, not a failure), the
/// saved choices stay, and the local engine keeps running.
#[test]
fn disconnecting_the_foreground_zhizi_engine_empties_its_seat_and_keeps_the_local_engine() {
    let mut b = Bench::new(vec![local_profile("local")]);
    assert_eq!(b.connected(), ZhiziConnection::Disconnected);
    b.wb.switch_engine(0).unwrap();
    assert!(b.ready(0));
    let local = b.world.ids()[0];
    b.wb.activate_cloud(b.wb.cloud_ticket(), zhizi_settings(), true).unwrap();
    let zhizi_index = b.wb.config().engine.engines.iter().position(|p| p.cloud.is_some()).unwrap();
    assert!(b.switched(zhizi_index));
    assert_eq!(b.connected(), ZhiziConnection::Connected);
    assert_eq!(b.service.open.load(Ordering::SeqCst), 1);
    let config = b.wb.config().clone();

    assert_eq!(b.wb.disconnect_zhizi(), ZhiziConnection::Disconnected);
    assert!(b.open(0), "the service sees the connection closed");
    let view = b.wb.view(false);
    assert_eq!(view.engine.zhizi, ZhiziConnection::Disconnected);
    assert!(view.engine.foreground.is_none(), "no current foreground engine");
    assert!(!view.engine.down, "a disconnection is not a failure");
    assert!(view.engine_failures.is_empty());
    assert_eq!(view.engine.engines[zhizi_index].state, None);
    assert_eq!(view.engine.engines[0].state, Some(EngineState::Ready), "the local engine keeps running");
    assert!(!b.world.ended(local));
    assert_eq!(b.wb.config(), &config, "the saved engine and provider choices stay");
    b.until_within(Duration::from_millis(300), |_| false);
    assert_eq!(b.connections(), 1, "nothing connects again");
}

/// Two 智子 profiles in the foreground and secondary seats (双引擎模式) and a third kept by fast
/// engine change: all three close; the local engine in the list is untouched.
#[test]
fn foreground_secondary_and_kept_zhizi_engines_all_close() {
    let mut b = Bench::new(vec![zhizi_profile("a"), zhizi_profile("b"), zhizi_profile("c"), local_profile("local")]);
    assert!(b.wb.set_extra_mode(ExtraMode::DoubleEngine).unwrap());
    b.wb.switch_engine(2).unwrap();
    assert!(b.ready(2));
    b.wb.switch_engine(0).unwrap();
    assert!(b.ready(0));
    b.wb.switch_secondary_engine(1).unwrap();
    assert!(b.ready(1));
    assert_eq!(b.state_of(2), Some(EngineState::Ready), "kept by fast engine change");
    assert_eq!(b.service.open.load(Ordering::SeqCst), 3);
    assert_eq!(b.connected(), ZhiziConnection::Connected);

    assert_eq!(b.wb.disconnect_zhizi(), ZhiziConnection::Disconnected);
    assert!(b.open(0), "all three connections close");
    let view = b.wb.view(false);
    assert!(view.engine.foreground.is_none() && view.engine.secondary.is_none());
    assert!(view.engine.engines.iter().all(|e| e.state.is_none()));
    assert_eq!(view.engine.zhizi, ZhiziConnection::Disconnected);
    assert_eq!(b.connections(), 3);
}

/// Java keeps a selected engine that left the catalog; here removing a profile ends its engine at
/// once, so a 智子 entry removed while selected leaves no connection behind for the action to
/// find, and the status says so.
#[test]
fn a_selected_zhizi_entry_removed_from_the_list_leaves_no_connection() {
    let mut b = Bench::new(vec![local_profile("local"), zhizi_profile("z")]);
    b.wb.switch_engine(1).unwrap();
    assert!(b.ready(1));
    assert_eq!(b.connected(), ZhiziConnection::Connected);
    b.wb.save_engine_settings(vec![local_profile("local")], EngineStartup::Default).unwrap();
    assert!(b.open(0), "the removed entry's connection closes");
    assert_eq!(b.connected(), ZhiziConnection::Disconnected);
    assert_eq!(b.wb.disconnect_zhizi(), ZhiziConnection::Disconnected);
    assert_eq!(b.connections(), 1);
}

/// A local foreground engine, a 自建算力 secondary engine and a kept 智子 engine: only 智子
/// closes; the other two keep their processes and seats, and the foreground keeps analysing.
#[test]
fn local_and_custom_engines_keep_running_and_their_seats() {
    let mut b = Bench::new(vec![zhizi_profile("z"), local_profile("local"), custom_profile("custom")]);
    assert!(b.wb.set_extra_mode(ExtraMode::DoubleEngine).unwrap());
    b.wb.switch_engine(0).unwrap();
    assert!(b.ready(0));
    b.wb.switch_engine(1).unwrap();
    assert!(b.ready(1));
    b.wb.switch_secondary_engine(2).unwrap();
    assert!(b.ready(2));
    let ids = b.world.ids();
    let (local, custom) = (ids[0], ids[1]);
    assert_eq!(b.connected(), ZhiziConnection::Connected, "the kept 智子 engine still holds its connection");

    assert_eq!(b.wb.disconnect_zhizi(), ZhiziConnection::Disconnected);
    assert!(b.open(0));
    assert!(!b.world.ended(local) && !b.world.ended(custom));
    let view = b.wb.view(false);
    assert_eq!(view.engine.foreground.as_ref().map(|f| f.index), Some(1));
    assert!(view.engine.foreground.unwrap().ready);
    assert_eq!(view.engine.secondary.as_ref().map(|s| s.index), Some(2));
    assert_eq!(view.engine.engines[0].state, None);
    assert!(view.engine.analysing, "the local engine still analyses the position");
}

/// A switch to 智子 still starting (Java's switch transaction and `zhiziStarting`): refused,
/// nothing closes or changes; once the engine is ready the same action succeeds.
#[test]
fn a_zhizi_engine_still_starting_refuses_the_disconnection() {
    let mut b = Bench::new(vec![local_profile("local")]);
    b.service.hold.store(true, Ordering::SeqCst);
    b.wb.activate_cloud(b.wb.cloud_ticket(), zhizi_settings(), true).unwrap();
    let zhizi_index = b.wb.config().engine.engines.iter().position(|p| p.cloud.is_some()).unwrap();
    let counter = b.service.open.clone();
    assert!(b.until(|_| counter.load(Ordering::SeqCst) == 1));
    assert_eq!(b.state_of(zhizi_index), Some(EngineState::Loading));
    assert_eq!(b.connected(), ZhiziConnection::Busy);
    let before = b.wb.view(false);

    assert_eq!(b.wb.disconnect_zhizi(), ZhiziConnection::Busy);
    let after = b.wb.view(false);
    assert_eq!(after.engine, before.engine, "nothing changes");
    assert_eq!(b.service.open.load(Ordering::SeqCst), 1, "the connection stays");

    b.service.hold.store(false, Ordering::SeqCst);
    assert!(b.switched(zhizi_index));
    assert_eq!(b.connected(), ZhiziConnection::Connected);
    assert_eq!(b.wb.disconnect_zhizi(), ZhiziConnection::Disconnected);
    assert!(b.open(0));
}

/// An engine match with a 智子 participant (Java's active `EngineGameOwnerTransaction`): refused
/// with nothing closed, whatever engine is in the foreground seat.
#[test]
fn an_engine_match_with_a_zhizi_participant_refuses_the_disconnection() {
    let mut b = Bench::new(vec![zhizi_profile("z"), local_profile("local")]);
    let started = b.wb.engine_match(MatchCommand::Start {
        settings: MatchSettings { rules: "chinese".into(), ..MatchSettings::default() },
        session: MatchSessionSettings::default(),
        first_engine: 0,
        second_engine: 1,
    }).unwrap();
    assert_eq!(started, CommandResult::Done);
    assert!(b.ready(0) && b.ready(1));
    assert!(b.until(|wb| wb.inspect().engine_match.is_some()));
    assert_eq!(b.connected(), ZhiziConnection::Busy);
    assert_eq!(b.wb.disconnect_zhizi(), ZhiziConnection::Busy);
    assert_eq!(b.service.open.load(Ordering::SeqCst), 1, "the participant keeps its connection");
    assert!(b.wb.inspect().engine_match.is_some(), "the match goes on");
}

/// A 智子 activation requested before the disconnection (the shell reads the account first)
/// and committed after it is refused and connects nothing; a late transport callback of the
/// ended engine restarts nothing. A new explicit activation connects again.
#[test]
fn late_activations_and_callbacks_after_a_disconnection_do_not_reconnect() {
    let mut b = Bench::new(vec![local_profile("local")]);
    b.wb.activate_cloud(b.wb.cloud_ticket(), zhizi_settings(), true).unwrap();
    let zhizi_index = b.wb.config().engine.engines.iter().position(|p| p.cloud.is_some()).unwrap();
    assert!(b.switched(zhizi_index));
    let old = b.last_zhizi();
    let ticket = b.wb.cloud_ticket();

    assert_eq!(b.wb.disconnect_zhizi(), ZhiziConnection::Disconnected);
    assert!(b.open(0));
    let late = b.wb.activate_cloud(ticket, zhizi_settings(), true).unwrap_err();
    assert_eq!(late.message, ZHIZI_ACTIVATION_RETIRED);
    b.wb.engine_input(old, EngineInput::CloudRecovery { message: "connection lost".into() });
    b.wb.engine_input(old, EngineInput::Exited(None));
    let launched = b.zhizi.lock().unwrap().len();
    assert!(!b.until_within(Duration::from_millis(500), |wb| wb.view(false).engine.foreground.is_some()), "nothing takes the seat");
    assert_eq!(b.zhizi.lock().unwrap().len(), launched);
    assert_eq!(b.connections(), 1, "the service sees no new connection");
    assert!(b.wb.view(false).engine_failures.is_empty());

    // Nothing connected: the action still retires a request under way.
    let ticket = b.wb.cloud_ticket();
    assert_eq!(b.wb.disconnect_zhizi(), ZhiziConnection::Disconnected);
    assert!(b.wb.activate_cloud(ticket, zhizi_settings(), true).is_err());

    b.wb.activate_cloud(b.wb.cloud_ticket(), zhizi_settings(), true).unwrap();
    assert!(b.ready(zhizi_index));
    assert_eq!(b.connections(), 2, "an explicit activation connects again");
}
