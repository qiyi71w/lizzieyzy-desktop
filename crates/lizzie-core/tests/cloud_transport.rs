//! Tests cross the same CloudConnector/EngineIo interface as the process launcher. Wire fixtures
//! come from the fixed Java transports, not from private adapter helpers.

use std::io::{Read, Write};
use std::net::{TcpListener, TcpStream};
use std::sync::{mpsc, Arc, Mutex};
use std::thread::{self, JoinHandle};
use std::time::{Duration, Instant};

use lizzie_core::api::GameCommand;
use lizzie_core::browse::Navigation;
use lizzie_core::engine::cloud::{CloudConnectError, CloudConnector, CloudLaunch, CloudProvider, SocketCredentials, SocketTokenSource};
use lizzie_core::engine::gtp::{parse_info, Dialect};
use lizzie_core::engine::runtime::{EngineInput, EngineIo, FocusCapability};
use lizzie_core::record::{BoardSize, Point, ROOT};
use lizzie_core::settings::{NetworkSettings, ProxyMode};
use serde_json::{json, Value};
use tokio_tungstenite::tungstenite::{accept, Message, WebSocket};

#[path = "cloud_transport/workbench.rs"]
mod workbench;

type Peer = WebSocket<TcpStream>;

fn server(run: impl FnOnce(Peer) + Send + 'static) -> (String, JoinHandle<()>) {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let address = listener.local_addr().unwrap();
    let thread = thread::spawn(move || {
        let (stream, _) = listener.accept().unwrap();
        stream.set_read_timeout(Some(Duration::from_secs(8))).unwrap();
        stream.set_write_timeout(Some(Duration::from_secs(8))).unwrap();
        run(accept(stream).unwrap());
    });
    (format!("ws://{address}"), thread)
}

fn text(peer: &mut Peer) -> String {
    loop {
        match peer.read().unwrap() {
            Message::Text(text) => return text.to_string(),
            Message::Ping(_) | Message::Pong(_) => {},
            other => panic!("unexpected frame: {other:?}"),
        }
    }
}

fn write(peer: &mut Peer, value: Value) {
    peer.send(Message::Text(value.to_string().into())).unwrap();
}

fn socketio_ready(peer: &mut Peer) {
    peer.send(Message::Text("0{\"sid\":\"fixture\",\"upgrades\":[],\"pingInterval\":25000,\"pingTimeout\":20000}".into())).unwrap();
    assert_eq!(text(peer), "40");
    peer.send(Message::Text("40{\"sid\":\"fixture\"}".into())).unwrap();
    peer.send(Message::Text("42[\"ready\"]".into())).unwrap();
}

struct Running {
    io: Option<Box<dyn EngineIo>>,
    events: mpsc::Receiver<EngineInput>,
    endings: Arc<Mutex<Vec<JoinHandle<()>>>>,
}

impl Running {
    fn new(provider: CloudProvider, url: String, tokens: SocketTokenSource) -> Self {
        let launch = CloudLaunch { provider, identifier: "fixture-account".into(), args: "fixture-args".into(), url };
        Self::launch(launch, CloudConnector::new(tokens))
    }
    fn launch(launch: CloudLaunch, connector: CloudConnector) -> Self {
        let (tx, events) = mpsc::channel();
        let endings = Arc::new(Mutex::new(Vec::new()));
        let io = connector.launch(17, &launch, Arc::new(move |id, event| {
            assert_eq!(id, 17);
            let _ = tx.send(event);
        }), endings.clone()).unwrap();
        Self { io: Some(io), events, endings }
    }
    fn network(provider: CloudProvider, url: String, network: NetworkSettings) -> Self {
        let socket_url = url.clone();
        let connector = CloudConnector::new(Arc::new(move |_, _| {
            Ok(SocketCredentials { socket_url: socket_url.clone(), token: "private-socket-token".into() })
        })).with_network(Arc::new(move || Ok(network.clone())));
        let launch = CloudLaunch { provider, identifier: "fixture-account".into(), args: String::new(), url };
        Self::launch(launch, connector)
    }
    fn custom(url: String) -> Self {
        Self::new(CloudProvider::Custom, url, Arc::new(|_, _| panic!("custom transport must not resolve an account token")))
    }
    fn zhizi(url: String) -> Self {
        Self::new(CloudProvider::Zhizi, String::new(), Arc::new(move |identifier, args| {
            assert_eq!(identifier, "fixture-account"); assert_eq!(args, "fixture-args");
            Ok(SocketCredentials { socket_url: url.clone(), token: "private-socket-token".into() })
        }))
    }
    fn send(&mut self, line: &str) { self.io.as_mut().unwrap().send(line).unwrap(); }
    fn until(&self, predicate: impl Fn(&EngineInput) -> bool) -> Vec<EngineInput> {
        let deadline = Instant::now() + Duration::from_secs(8);
        let mut received = Vec::new();
        loop {
            let event = self.events.recv_timeout(deadline.saturating_duration_since(Instant::now())).unwrap();
            let complete = predicate(&event);
            received.push(event);
            if complete { return received; }
        }
    }
    fn stdout(&self, expected: &str) -> Vec<EngineInput> {
        self.until(|event| matches!(event, EngineInput::Stdout(line) if line == expected))
    }
    fn stop(&mut self) {
        if let Some(io) = self.io.take() { io.terminate(); }
    }
}

impl Drop for Running {
    fn drop(&mut self) {
        self.stop();
        let handles = std::mem::take(&mut *self.endings.lock().unwrap());
        for handle in handles { handle.join().unwrap(); }
    }
}

#[test]
fn engine_response_can_synchronously_send_the_next_bootstrap_command() {
    let (url, peer) = server(|mut peer| { let _ = peer.read(); });
    let slot: Arc<Mutex<Option<Box<dyn EngineIo>>>> = Arc::new(Mutex::new(None));
    let callback_slot = slot.clone();
    let (done, observed) = mpsc::channel();
    let endings = Arc::new(Mutex::new(Vec::new()));
    let launch = CloudLaunch { provider: CloudProvider::Custom, identifier: String::new(), args: String::new(), url };
    let io = CloudConnector::new(Arc::new(|_, _| panic!("custom account lookup"))).launch(
        1, &launch, Arc::new(move |_, event| {
            if let EngineInput::Stdout(line) = event {
                if line.starts_with("=1") {
                    callback_slot.lock().unwrap().as_mut().unwrap().send("2 version").unwrap();
                }
                if line.starts_with("=2") { let _ = done.send(line); }
            }
        }), endings.clone(),
    ).unwrap();
    *slot.lock().unwrap() = Some(io);
    slot.lock().unwrap().as_mut().unwrap().send("1 name").unwrap();
    let version = observed.recv_timeout(Duration::from_secs(3))
        .expect("bootstrap callback must not deadlock when sending the next command");
    assert!(version.starts_with("=2 "));
    slot.lock().unwrap().take().unwrap().terminate();
    for thread in std::mem::take(&mut *endings.lock().unwrap()) { thread.join().unwrap(); }
    peer.join().unwrap();
}

#[test]
fn custom_queries_preserve_rectangular_position_rules_parameters_and_analysis_details() {
    let (url, peer) = server(|mut peer| {
        let query: Value = serde_json::from_str(&text(&mut peer)).unwrap();
        assert_eq!(query["boardXSize"], 9); assert_eq!(query["boardYSize"], 7);
        assert_eq!(query["komi"], 6.5); assert_eq!(query["rules"]["scoring"], "TERRITORY");
        assert_eq!(query["moves"], json!([["B", "pass"], ["W", "D4"]]));
        assert_eq!(query["initialStones"], json!([["B", "A1"], ["W", "H6"]]));
        assert_eq!(query["initialPlayer"], "B"); assert_eq!(query["analyzeTurns"], json!([2]));
        assert_eq!(query["maxVisits"], 10_000_000); assert_eq!(query["priority"], 0);
        assert_eq!(query["playoutDoublingAdvantage"], 1.25);
        assert_eq!(query["overrideSettings"]["wideRootNoise"], 0.2);
        assert_eq!(query["overrideSettings"]["reportAnalysisWinratesAs"], "WHITE");
        assert_eq!(query["reportDuringSearchEvery"], 0.1);
        assert_eq!(query["includeOwnership"], true); assert_eq!(query["includeMovesOwnership"], true);
        write(&mut peer, json!({"id":query["id"], "isDuringSearch":false,
            "moveInfos":[{"move":"E4", "visits":30, "edgeVisits":23, "winrate":0.6,"scoreLead":2.5,"prior":0.2,"order":0,"pv":["E4","pass"],"pvVisits":[30,17],"ownership":vec![0.1;63]}],
            "rootInfo":{"visits":40,"winrate":0.55,"scoreLead":1.25},"ownership":vec![0.25;63]}));
        // Keep the transport alive until its owner closes it.
        let _ = peer.read();
    });
    let mut engine = Running::custom(url);
    for command in ["1 rectangular_boardsize 9 7", "2 komi 6.5", "3 kata-set-rules japanese", "4 set_position B A1 W H6", "5 play B pass", "6 play W D4", "7 kata-set-param playoutDoublingAdvantage 1.25", "8 kata-set-param analysisWideRootNoise 0.2", "9 kata-analyze W 5 ownership true rootInfo true pvVisits true"] { engine.send(command); }
    let events = engine.until(|event| matches!(event, EngineInput::Stdout(line) if line.starts_with("info move")));
    let line = events.iter().find_map(|event| match event { EngineInput::Stdout(line) if line.starts_with("info move") => Some(line), _ => None }).unwrap();
    let info = parse_info(line, Dialect::Kata, BoardSize { width: 9, height: 7 });
    assert_eq!(info.root_visits, Some(40)); assert!((info.root_winrate.unwrap() - 55.0).abs() < 1e-9);
    assert_eq!(info.root_score_lead, Some(1.25)); assert_eq!(info.ownership.as_ref().unwrap().len(), 63);
    assert_eq!(info.candidates[0].allocation_visits, 23); assert_eq!(info.candidates[0].pv_visits, vec![30,17]);
    assert_eq!(info.candidates[0].ownership.as_ref().unwrap().len(), 63);
    engine.stop(); peer.join().unwrap();
}

#[test]
fn custom_stop_waits_for_both_final_result_and_matching_terminate_ack_and_ignores_retired_errors() {
    let (url, peer) = server(|mut peer| {
        let query: Value = serde_json::from_str(&text(&mut peer)).unwrap();
        let terminate: Value = serde_json::from_str(&text(&mut peer)).unwrap();
        assert_eq!(terminate["action"], "terminate"); assert_eq!(terminate["terminateId"], query["id"]);
        write(&mut peer, json!({"id":terminate["id"],"action":"terminate","terminateId":query["id"]}));
        write(&mut peer, json!({"id":query["id"],"isDuringSearch":false,"moveInfos":[{"move":"pass","visits":3}]}));
        let next: Value = serde_json::from_str(&text(&mut peer)).unwrap();
        write(&mut peer, json!({"id":query["id"],"error":"late retired query"}));
        write(&mut peer, json!({"id":next["id"],"isDuringSearch":false,"moveInfos":[{"move":"D4","visits":9}]}));
        let _ = peer.read();
    });
    let mut engine = Running::custom(url);
    engine.send("1 kata-analyze B 50"); engine.stdout("=1");
    engine.send("2 stop");
    let events = engine.stdout("=2");
    assert!(events.iter().any(|event| matches!(event, EngineInput::Stdout(line) if line.starts_with("info move pass"))));
    engine.send("3 genmove W"); engine.stdout("=3 D4");
    engine.send("4 name"); engine.stdout("=4 KataGo");
    engine.stop(); peer.join().unwrap();
}

#[test]
fn custom_genmove_snapshots_budget_and_updates_moves_without_accepting_invalid_parameters() {
    let (url, peer) = server(|mut peer| {
        let query: Value = serde_json::from_str(&text(&mut peer)).unwrap();
        assert_eq!(query["maxVisits"], 1024); assert_eq!(query["overrideSettings"]["maxTime"], 1.5);
        assert_eq!(query["priority"], 2); assert_eq!(query["includeOwnership"], false);
        assert_eq!(query["moves"], json!([]));
        write(&mut peer, json!({"id":query["id"],"isDuringSearch":false,"moveInfos":[{"move":"pass","visits":1024}]}));
        let next: Value = serde_json::from_str(&text(&mut peer)).unwrap();
        assert_eq!(next["moves"], json!([["W","pass"]]));
        assert_eq!(next["maxVisits"], 10_000_000);
        write(&mut peer, json!({"id":next["id"],"isDuringSearch":false,"moveInfos":[]}));
        let _ = peer.read();
    });
    let mut engine = Running::custom(url);
    for line in ["1 kata-set-param maxVisits 1024", "2 kata-set-param maxTime 1.5", "3 kata-set-param maxVisits 2.5", "4 kata-set-param analysisWideRootNoise NaN", "5 genmove W"] { engine.send(line); }
    let events = engine.stdout("=5 pass");
    assert!(events.iter().any(|e| matches!(e, EngineInput::Stdout(line) if line.starts_with("?3 "))));
    assert!(events.iter().any(|e| matches!(e, EngineInput::Stdout(line) if line.starts_with("?4 "))));
    engine.send("6 kata-analyze B 50"); engine.stdout("=6");
    engine.stop(); peer.join().unwrap();
}

#[test]
fn custom_loadsgf_snapshot_preserves_komi_player_stones_and_rectangular_coordinates() {
    let snapshot = tempfile::NamedTempFile::new().unwrap();
    std::fs::write(snapshot.path(), "(;FF[4]GM[1]CA[UTF-8]SZ[9:7]KM[0.5]PL[W]AB[aa]AW[ig])").unwrap();
    let (url, peer) = server(|mut peer| {
        let query: Value = serde_json::from_str(&text(&mut peer)).unwrap();
        assert_eq!(query["initialPlayer"], "W"); assert_eq!(query["komi"], 0.5);
        assert_eq!(query["initialStones"], json!([["B","A7"],["W","J1"]]));
        assert_eq!(query["boardXSize"], 9); assert_eq!(query["boardYSize"], 7);
        write(&mut peer, json!({"id":query["id"],"isDuringSearch":false,"moveInfos":[]}));
        let _ = peer.read();
    });
    let mut engine = Running::custom(url);
    engine.send(&format!("1 loadsgf {}", snapshot.path().display())); engine.stdout("=1");
    engine.send("2 kata-analyze 50"); engine.stdout("=2");
    engine.stop(); peer.join().unwrap();
}

#[test]
fn custom_rejects_unsupported_commands_and_zero_liberty_setup_without_mutating_position() {
    let (url, peer) = server(|mut peer| {
        let query: Value = serde_json::from_str(&text(&mut peer)).unwrap();
        assert_eq!(query["initialStones"], json!([["B","A1"]]));
        write(&mut peer, json!({"id":query["id"],"isDuringSearch":false,"moveInfos":[]}));
        let _ = peer.read();
    });
    let mut engine = Running::custom(url);
    for line in ["1 boardsize 2", "2 set_position B A1", "3 set_position B A1 W A2 W B1", "4 time_left B 1 0", "5 kata-raw-nn 0", "6 kata-set-param unknown 2", "7 kata-analyze B 50"] { engine.send(line); }
    let events = engine.stdout("=7");
    for id in [3,4,5,6] { assert!(events.iter().any(|event| matches!(event, EngineInput::Stdout(line) if line.starts_with(&format!("?{id} "))))); }
    engine.stop(); peer.join().unwrap();
}

#[test]
fn custom_wrong_query_identity_retires_transport_once_without_echoing_server_secrets() {
    let (url, peer) = server(|mut peer| {
        let _: Value = serde_json::from_str(&text(&mut peer)).unwrap();
        write(&mut peer, json!({"id":"wrong-secret-id", "isDuringSearch":false, "moveInfos":[]}));
        let _ = peer.read();
    });
    let mut engine = Running::custom(format!("{url}/private-url-secret"));
    engine.send("1 kata-analyze B 50");
    let events = engine.until(|event| matches!(event, EngineInput::CloudRecovery { .. } | EngineInput::Exited(_)));
    assert!(events.iter().any(|event| matches!(event, EngineInput::Exited(None))),
        "custom sockets use the normal alive-check restart policy, not Zhizi reconstruction");
    assert!(!format!("{events:?}").contains("secret"));
    assert!(engine.io.as_mut().unwrap().send("2 name").is_err());
    assert!(engine.events.recv_timeout(Duration::from_millis(100)).is_err());
    engine.stop(); peer.join().unwrap();
}

#[test]
fn zhizi_waits_for_real_ready_and_decodes_text_binary_chunks_and_heartbeats() {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let address = listener.local_addr().unwrap();
    let peer = thread::spawn(move || {
        let (stream, _) = listener.accept().unwrap();
        stream.set_read_timeout(Some(Duration::from_secs(4))).unwrap();
        let mut peer = tokio_tungstenite::tungstenite::accept_hdr(stream, |request: &tokio_tungstenite::tungstenite::handshake::server::Request, response| {
            assert!(request.uri().path().starts_with("/socket.io.v4"));
            let query = request.uri().query().unwrap();
            assert!(query.contains("EIO=4")); assert!(query.contains("transport=websocket"));
            assert!(query.contains("zz-socketio-token=private-socket-token"));
            Ok(response)
        }).unwrap();
        peer.send(Message::Text("0{\"sid\":\"fixture\"}".into())).unwrap();
        assert_eq!(text(&mut peer), "40");
        peer.send(Message::Text("40{}".into())).unwrap();
        // A namespace connect is not engine readiness; queued GTP must remain unwritten.
        peer.get_mut().set_read_timeout(Some(Duration::from_millis(100))).unwrap();
        assert!(peer.read().is_err());
        peer.get_mut().set_read_timeout(Some(Duration::from_secs(4))).unwrap();
        peer.send(Message::Text("42[\"ready\"]".into())).unwrap();
        let input = text(&mut peer);
        let event: Value = serde_json::from_str(input.strip_prefix("42").unwrap()).unwrap();
        assert_eq!(event, json!(["stdin","1 name\n"]));
        peer.send(Message::Text("2heartbeat".into())).unwrap(); assert_eq!(text(&mut peer), "3heartbeat");
        peer.send(Message::Text("42[\"stdout\",\"=1 Ka\"]".into())).unwrap();
        peer.send(Message::Text("451-[\"stdout\",{\"_placeholder\":true,\"num\":0}]".into())).unwrap();
        peer.send(Message::Binary(b"taGo\n\n".to_vec().into())).unwrap();
        peer.send(Message::Text("42[\"stderr\",\"private-socket-\"]".into())).unwrap();
        peer.send(Message::Text("451-[\"stderr\",{\"_placeholder\":true,\"num\":0}]".into())).unwrap();
        peer.send(Message::Binary(b"token\n".to_vec().into())).unwrap();
        let _ = peer.read();
    });
    let mut engine = Running::zhizi(format!("http://{address}"));
    engine.send("1 name"); engine.stdout("=1 KataGo");
    let events = engine.until(|event| matches!(event, EngineInput::Stderr(_)));
    assert!(!format!("{events:?}").contains("private-socket-token"));
    engine.stop(); peer.join().unwrap();
}

#[test]
fn cancelling_during_token_resolution_cannot_open_a_late_socket_or_emit_failure() {
    let (entered_tx, entered) = mpsc::channel();
    let (release_tx, release) = mpsc::channel();
    let release = Mutex::new(release);
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    listener.set_nonblocking(true).unwrap();
    let address = listener.local_addr().unwrap();
    let mut engine = Running::new(CloudProvider::Zhizi, String::new(), Arc::new(move |_, _| {
        entered_tx.send(()).unwrap(); release.lock().unwrap().recv().unwrap();
        Ok(SocketCredentials { socket_url: format!("ws://{address}"), token: "secret".into() })
    }));
    entered.recv_timeout(Duration::from_secs(4)).unwrap();
    let started = Instant::now(); engine.stop();
    assert!(started.elapsed() < Duration::from_millis(100));
    release_tx.send(()).unwrap();
    drop(engine.io.take());
    // Joining the worker and token resolver proves that there is no deferred socket use.
    for handle in std::mem::take(&mut *engine.endings.lock().unwrap()) { handle.join().unwrap(); }
    assert!(listener.accept().is_err()); assert!(engine.events.try_recv().is_err());
}

#[test]
fn fatal_resource_authorization_does_not_retry_or_expose_resolver_diagnostics() {
    let calls = Arc::new(Mutex::new(0)); let observed = calls.clone();
    let mut engine = Running::new(CloudProvider::Zhizi, String::new(), Arc::new(move |_, _| {
        *observed.lock().unwrap() += 1;
        Err(CloudConnectError { message: "403 private-token-in-service-response".into(), fatal: true })
    }));
    let events = engine.until(|event| matches!(event, EngineInput::Exited(None)));
    assert_eq!(*calls.lock().unwrap(), 1); assert!(!format!("{events:?}").contains("private-token"));
    engine.stop();
}

#[test]
fn zhizi_transient_startup_retries_with_fresh_credentials_and_drops_stale_output() {
    let (url, peer) = server(|mut peer| {
        socketio_ready(&mut peer);
        let input = text(&mut peer);
        assert_eq!(serde_json::from_str::<Value>(input.strip_prefix("42").unwrap()).unwrap(), json!(["stdin","1 name\n"]));
        peer.send(Message::Text("42[\"stdout\",\"=1 KataGo\\n\\n\"]".into())).unwrap();
        let _ = peer.read();
    });
    let calls = Arc::new(Mutex::new(0)); let observed = calls.clone();
    let mut engine = Running::new(CloudProvider::Zhizi, String::new(), Arc::new(move |_, _| {
        let mut count = observed.lock().unwrap(); *count += 1;
        if *count == 1 { Err(CloudConnectError { message: "worker unavailable".into(), fatal: false }) }
        else { Ok(SocketCredentials { socket_url: url.clone(), token: "secret".into() }) }
    }));
    engine.send("1 name"); engine.stdout("=1 KataGo"); assert_eq!(*calls.lock().unwrap(), 2);
    engine.stop(); peer.join().unwrap();
}

#[test]
fn established_disconnect_requests_one_reconstruction_not_a_hidden_reconnect() {
    let (url, peer) = server(|mut peer| { socketio_ready(&mut peer); peer.close(None).unwrap(); });
    let calls = Arc::new(Mutex::new(0)); let observed = calls.clone();
    let mut engine = Running::new(CloudProvider::Zhizi, String::new(), Arc::new(move |_, _| {
        *observed.lock().unwrap() += 1;
        Ok(SocketCredentials { socket_url: url.clone(), token: "secret".into() })
    }));
    engine.until(|event| matches!(event, EngineInput::CloudRecovery { .. }));
    assert_eq!(*calls.lock().unwrap(), 1);
    assert!(engine.events.recv_timeout(Duration::from_millis(100)).is_err());
    engine.stop(); peer.join().unwrap();
}

#[test]
fn launch_and_socket_credentials_debug_never_reveal_secrets() {
    let launch = CloudLaunch { provider: CloudProvider::Custom, identifier: "private-account".into(), args: "private-args".into(), url: "wss://private-user:private-password@example.test/private-code".into() };
    let credentials = SocketCredentials { socket_url: "wss://example.test/private-code".into(), token: "private-token".into() };
    assert!(!format!("{launch:?} {credentials:?}").contains("private"));
}

#[test]
fn raw_analysis_and_repeated_accepted_totals_do_not_keep_a_stalled_zhizi_session_alive() {
    let (started_tx, started) = mpsc::channel();
    let (url, peer) = server(move |mut peer| {
        socketio_ready(&mut peer);
        assert!(text(&mut peer).contains("kata-analyze"));
        started_tx.send(()).unwrap();
        // A peer can keep producing bytes while the current board rejects every result.
        for _ in 0..230 {
            if peer.send(Message::Text("42[\"stdout\",\"info move D4 visits 100 winrate 0.5\\n\"]".into())).is_err() { return; }
            thread::sleep(Duration::from_millis(100));
        }
    });
    let mut engine = Running::zhizi(url);
    engine.send("1 kata-analyze B 50");
    started.recv_timeout(Duration::from_secs(4)).unwrap();
    engine.io.as_mut().unwrap().analysis_progress_accepted(100);
    let start = Instant::now();
    let mut repeated = false;
    loop {
        if !repeated && start.elapsed() > Duration::from_secs(10) {
            engine.io.as_mut().unwrap().analysis_progress_accepted(100);
            repeated = true;
        }
        let event = engine.events.recv_timeout(Duration::from_secs(24).saturating_sub(start.elapsed())).unwrap();
        if matches!(event, EngineInput::CloudRecovery { .. }) { break; }
    }
    assert!(repeated);
    assert!(start.elapsed() >= Duration::from_secs(19));
    assert!(start.elapsed() < Duration::from_secs(24));
    engine.stop(); peer.join().unwrap();
}

#[test]
fn stop_cancels_the_continuous_analysis_watchdog() {
    let (stopped_tx, stopped) = mpsc::channel();
    let (url, peer) = server(move |mut peer| {
        socketio_ready(&mut peer);
        assert!(text(&mut peer).contains("kata-analyze"));
        assert!(text(&mut peer).contains("stop"));
        stopped_tx.send(()).unwrap();
        peer.get_mut().set_read_timeout(Some(Duration::from_secs(25))).unwrap();
        let _ = peer.read();
    });
    let mut engine = Running::zhizi(url);
    engine.send("1 kata-analyze B 50"); engine.send("2 stop");
    stopped.recv_timeout(Duration::from_secs(4)).unwrap();
    assert!(engine.events.recv_timeout(Duration::from_secs(21)).is_err());
    engine.stop(); peer.join().unwrap();
}

#[test]
fn cancel_during_ready_wait_closes_connection_without_flushing_queued_gtp() {
    let (connected_tx, connected) = mpsc::channel();
    let (url, peer) = server(move |mut peer| {
        peer.send(Message::Text("0{\"sid\":\"fixture\"}".into())).unwrap();
        assert_eq!(text(&mut peer), "40");
        peer.send(Message::Text("40{}".into())).unwrap();
        connected_tx.send(()).unwrap();
        match peer.read() {
            Err(_) | Ok(Message::Close(_)) => {},
            other => panic!("cancelled startup sent GTP: {other:?}"),
        }
    });
    let mut engine = Running::zhizi(url);
    engine.send("1 name");
    connected.recv_timeout(Duration::from_secs(4)).unwrap();
    engine.stop(); peer.join().unwrap();
    assert!(engine.events.try_recv().is_err());
}

#[test]
fn zhizi_transient_startup_is_bounded_to_three_attempts() {
    let calls = Arc::new(Mutex::new(0)); let observed = calls.clone();
    let mut engine = Running::new(CloudProvider::Zhizi, String::new(), Arc::new(move |_, _| {
        *observed.lock().unwrap() += 1;
        Err(CloudConnectError { message: "no worker available".into(), fatal: false })
    }));
    let start = Instant::now();
    engine.until(|event| matches!(event, EngineInput::Exited(None)));
    assert_eq!(*calls.lock().unwrap(), 3);
    assert!(start.elapsed() >= Duration::from_millis(5400));
    assert!(engine.events.recv_timeout(Duration::from_millis(100)).is_err());
    engine.stop();
}

#[test]
fn custom_stop_rejects_an_incomplete_termination_receipt() {
    let (url, peer) = server(|mut peer| {
        let query: Value = serde_json::from_str(&text(&mut peer)).unwrap();
        let terminate: Value = serde_json::from_str(&text(&mut peer)).unwrap();
        write(&mut peer, json!({"id":query["id"],"isDuringSearch":false,"moveInfos":[]}));
        write(&mut peer, json!({"id":terminate["id"],"action":"terminate"}));
        let _ = peer.read();
    });
    let mut engine = Running::custom(url);
    engine.send("1 kata-analyze B 50"); engine.stdout("=1"); engine.send("2 stop");
    let events = engine.until(|event| matches!(event, EngineInput::Exited(None)));
    assert!(!events.iter().any(|event| matches!(event, EngineInput::Stdout(line) if line == "=2")));
    engine.stop(); peer.join().unwrap();
}

fn manual_proxy(port: u16) -> NetworkSettings {
    NetworkSettings { proxy_mode: ProxyMode::Manual, proxy_host: "127.0.0.1".into(), proxy_port: port }
}

fn accept_bounded(listener: &TcpListener) -> TcpStream {
    listener.set_nonblocking(true).unwrap();
    let deadline = Instant::now() + Duration::from_secs(8);
    loop {
        match listener.accept() {
            Ok((stream, _)) => {
                stream.set_read_timeout(Some(Duration::from_secs(8))).unwrap();
                stream.set_write_timeout(Some(Duration::from_secs(8))).unwrap();
                return stream;
            }
            Err(error) if error.kind() == std::io::ErrorKind::WouldBlock && Instant::now() < deadline => thread::sleep(Duration::from_millis(5)),
            Err(error) => panic!("fixture accept failed: {error}"),
        }
    }
}

fn proxy_session(peer: &mut Peer, provider: CloudProvider) {
    match provider {
        CloudProvider::Zhizi => {
            socketio_ready(peer);
            assert!(text(peer).contains("1 name"));
            peer.send(Message::Text("42[\"stdout\",\"=1 fixture\\n\\n\"]".into())).unwrap();
        }
        CloudProvider::Custom => {
            let query: Value = serde_json::from_str(&text(peer)).unwrap();
            write(peer, json!({"id":query["id"], "isDuringSearch":false, "moveInfos":[{"move":"pass","visits":1}]}));
        }
    }
    let _ = peer.read();
}

fn routed_request(engine: &mut Running, provider: CloudProvider) {
    match provider {
        CloudProvider::Zhizi => { engine.send("1 name"); engine.stdout("=1 fixture"); }
        CloudProvider::Custom => { engine.send("1 genmove B"); engine.stdout("=1 pass"); }
    }
}

fn upgrade_proxy(listener: TcpListener, provider: CloudProvider) -> JoinHandle<()> {
    thread::spawn(move || {
        let stream = accept_bounded(&listener);
        let mut peer = tokio_tungstenite::tungstenite::accept_hdr(stream,
            |request: &tokio_tungstenite::tungstenite::handshake::server::Request, response| {
                // HTTP proxies receive absolute-form requests; this host has no direct route.
                assert_eq!(request.uri().scheme_str(), Some("http"));
                assert_eq!(request.uri().host(), Some("cloud-proxy.invalid"));
                if provider == CloudProvider::Zhizi {
                    assert_eq!(request.uri().path(), "/socket.io.v4/");
                    assert!(request.uri().query().unwrap().contains("zz-socketio-token="));
                } else {
                    assert_eq!(request.uri().path(), "/private-link");
                }
                Ok(response)
            }).unwrap();
        proxy_session(&mut peer, provider);
    })
}

#[test]
fn manual_proxy_routes_both_cloud_protocols_without_resolving_the_target() {
    for provider in [CloudProvider::Custom, CloudProvider::Zhizi] {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let settings = manual_proxy(listener.local_addr().unwrap().port());
        let peer = upgrade_proxy(listener, provider);
        let mut engine = Running::network(provider, "ws://cloud-proxy.invalid/private-link".into(), settings);
        routed_request(&mut engine, provider);
        engine.stop(); peer.join().unwrap();
    }
}

#[test]
fn system_proxy_and_direct_modes_use_an_isolated_process_environment() {
    const CHILD: &str = "LIZZIE_CLOUD_PROXY_TEST_CHILD";
    if std::env::var_os(CHILD).is_none() {
        let status = std::process::Command::new(std::env::current_exe().unwrap())
            .args(["--exact", "system_proxy_and_direct_modes_use_an_isolated_process_environment", "--nocapture"])
            .env(CHILD, "1")
            .env_remove("HTTP_PROXY").env_remove("HTTPS_PROXY").env_remove("ALL_PROXY").env_remove("NO_PROXY")
            .env_remove("http_proxy").env_remove("https_proxy").env_remove("all_proxy").env_remove("no_proxy")
            .status().unwrap();
        assert!(status.success());
        return;
    }
    // This child runs only this test: environment mutation cannot race other transport tests.
    for provider in [CloudProvider::Custom, CloudProvider::Zhizi] {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let proxy = format!("http://{}", listener.local_addr().unwrap());
        std::env::set_var("http_proxy", &proxy);
        std::env::set_var("https_proxy", &proxy);
        std::env::set_var("all_proxy", &proxy);
        std::env::set_var("no_proxy", "");
        let peer = upgrade_proxy(listener, provider);
        let settings = NetworkSettings { proxy_mode: ProxyMode::System, ..NetworkSettings::default() };
        let mut engine = Running::network(provider, "ws://cloud-proxy.invalid/private-link".into(), settings);
        routed_request(&mut engine, provider);
        engine.stop(); peer.join().unwrap();

        // Direct must ignore even an unreachable proxy from the environment.
        std::env::set_var("http_proxy", "http://127.0.0.1:1");
        std::env::set_var("all_proxy", "http://127.0.0.1:1");
        let (url, peer) = server(move |mut peer| proxy_session(&mut peer, provider));
        let mut engine = Running::network(provider, url, NetworkSettings::default());
        routed_request(&mut engine, provider);
        engine.stop(); peer.join().unwrap();
    }
}

#[test]
fn invalid_or_unreachable_manual_proxy_never_falls_through_to_the_destination() {
    for provider in [CloudProvider::Custom, CloudProvider::Zhizi] {
        let destination = TcpListener::bind("127.0.0.1:0").unwrap();
        destination.set_nonblocking(true).unwrap();
        let unavailable = TcpListener::bind("127.0.0.1:0").unwrap();
        let port = unavailable.local_addr().unwrap().port();
        drop(unavailable);
        for settings in [manual_proxy(0), manual_proxy(port), NetworkSettings {
            proxy_mode: ProxyMode::Manual, proxy_host: "private-user:private-password@127.0.0.1/private-proxy".into(), proxy_port: 80,
        }] {
            let mut engine = Running::network(provider, format!("ws://{}/private-link", destination.local_addr().unwrap()), settings);
            engine.send("1 name");
            let events = engine.until(|event| matches!(event, EngineInput::Exited(None)));
            for event in events {
                if let EngineInput::Stderr(message) = event {
                    for secret in ["private-user", "private-password", "private-proxy", "private-link", "private-socket-token", "ws://"] {
                        assert!(!message.contains(secret));
                    }
                }
            }
            assert!(matches!(destination.accept(), Err(error) if error.kind() == std::io::ErrorKind::WouldBlock));
            engine.stop();
        }
    }
}

fn read_http_head(stream: &mut TcpStream) -> String {
    let mut bytes = Vec::new();
    while !bytes.ends_with(b"\r\n\r\n") {
        let mut byte = [0];
        stream.read_exact(&mut byte).unwrap();
        bytes.push(byte[0]);
        assert!(bytes.len() < 16_384);
    }
    String::from_utf8(bytes).unwrap()
}

#[test]
fn cancelled_proxy_handshake_closes_both_protocols_promptly() {
    for provider in [CloudProvider::Custom, CloudProvider::Zhizi] {
        for scheme in ["ws", "wss"] {
            let listener = TcpListener::bind("127.0.0.1:0").unwrap();
            let settings = manual_proxy(listener.local_addr().unwrap().port());
            let (arrived, received) = mpsc::channel();
            let peer = thread::spawn(move || {
                let mut stream = accept_bounded(&listener);
                let request = read_http_head(&mut stream);
                assert!(request.starts_with(if scheme == "wss" { "CONNECT " } else { "GET http://" }));
                arrived.send(()).unwrap();
                let mut byte = [0];
                assert_eq!(stream.read(&mut byte).unwrap(), 0);
            });
            let mut engine = Running::network(provider, format!("{scheme}://cloud-proxy.invalid/private-link"), settings);
            engine.send("1 name");
            received.recv_timeout(Duration::from_secs(4)).unwrap();
            let start = Instant::now();
            engine.stop();
            drop(engine);
            peer.join().unwrap();
            assert!(start.elapsed() < Duration::from_secs(2));
        }
    }
}

#[test]
fn proxy_tunnel_cannot_downgrade_tls_to_a_plain_websocket() {
    for provider in [CloudProvider::Custom, CloudProvider::Zhizi] {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let settings = manual_proxy(listener.local_addr().unwrap().port());
        let peer = thread::spawn(move || {
            // Zhizi has exactly three bounded startup attempts; Custom has one.
            for _ in 0..if provider == CloudProvider::Zhizi { 3 } else { 1 } {
                let mut stream = accept_bounded(&listener);
                let request = read_http_head(&mut stream);
                assert!(request.starts_with("CONNECT cloud-proxy.invalid:443 HTTP/1.1\r\n"));
                stream.write_all(b"HTTP/1.1 200 Connection established\r\n\r\n").unwrap();
                let mut tls = [0; 5];
                stream.read_exact(&mut tls).unwrap();
                assert_eq!(tls[0], 0x16, "the tunnel must begin TLS, not plaintext GET");
                stream.write_all(b"HTTP/1.1 101 Switching Protocols\r\nUpgrade: websocket\r\nConnection: Upgrade\r\n\r\n").unwrap();
            }
        });
        let mut engine = Running::network(provider, "wss://cloud-proxy.invalid/private-link".into(), settings);
        engine.send("1 name");
        let events = engine.until(|event| matches!(event, EngineInput::Exited(None)));
        assert!(!events.iter().any(|event| matches!(event, EngineInput::Stdout(_))));
        engine.stop(); peer.join().unwrap();
    }
}

#[test]
fn invalid_websocket_accept_is_rejected_through_the_proxy() {
    for provider in [CloudProvider::Custom, CloudProvider::Zhizi] {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let settings = manual_proxy(listener.local_addr().unwrap().port());
        let peer = thread::spawn(move || {
            for _ in 0..if provider == CloudProvider::Zhizi { 3 } else { 1 } {
                let mut stream = accept_bounded(&listener);
                let _ = read_http_head(&mut stream);
                stream.write_all(b"HTTP/1.1 101 Switching Protocols\r\nUpgrade: websocket\r\nConnection: Upgrade\r\nSec-WebSocket-Accept: invalid\r\n\r\n").unwrap();
            }
        });
        let mut engine = Running::network(provider, "ws://cloud-proxy.invalid/private-link".into(), settings);
        engine.send("1 name");
        let events = engine.until(|event| matches!(event, EngineInput::Exited(None)));
        assert!(!events.iter().any(|event| matches!(event, EngineInput::Stdout(_))));
        engine.stop(); peer.join().unwrap();
    }
}

#[test]
fn connector_reads_current_proxy_again_for_each_launch() {
    for provider in [CloudProvider::Custom, CloudProvider::Zhizi] {
        let current = Arc::new(Mutex::new(manual_proxy(0)));
        let source = current.clone();
        let (url, peer) = server(move |mut peer| proxy_session(&mut peer, provider));
        let socket_url = url.clone();
        let connector = CloudConnector::new(Arc::new(move |_, _| Ok(SocketCredentials {
            socket_url: socket_url.clone(), token: "private-socket-token".into(),
        }))).with_network(Arc::new(move || Ok(source.lock().unwrap().clone())));
        let launch = CloudLaunch { provider, identifier: String::new(), args: String::new(), url };
        let first = Running::launch(launch.clone(), connector.clone());
        first.until(|event| matches!(event, EngineInput::Exited(None)));
        drop(first);
        *current.lock().unwrap() = NetworkSettings::default();
        let mut second = Running::launch(launch, connector);
        routed_request(&mut second, provider);
        second.stop(); peer.join().unwrap();
    }
}

#[test]
fn unavailable_network_snapshot_fails_closed_without_disclosing_its_error() {
    for provider in [CloudProvider::Custom, CloudProvider::Zhizi] {
        let destination = TcpListener::bind("127.0.0.1:0").unwrap();
        destination.set_nonblocking(true).unwrap();
        let connector = CloudConnector::new(Arc::new(|_, _| panic!("network failure must precede token resolution")))
            .with_network(Arc::new(|| Err(CloudConnectError { message: "private-proxy-password".into(), fatal: true })));
        let launch = CloudLaunch { provider, identifier: String::new(), args: String::new(), url: format!("ws://{}", destination.local_addr().unwrap()) };
        let engine = Running::launch(launch, connector);
        let events = engine.until(|event| matches!(event, EngineInput::Exited(None)));
        assert!(events.iter().any(|event| matches!(event, EngineInput::Stderr(message) if message == "Cloud network configuration failed")));
        assert!(!events.iter().any(|event| matches!(event, EngineInput::Stderr(message) if message.contains("private-proxy-password"))));
        assert!(matches!(destination.accept(), Err(error) if error.kind() == std::io::ErrorKind::WouldBlock));
    }
}

#[test]
fn custom_command_after_a_pending_stop_closes_the_stream_and_answers_in_order() {
    let (url, peer) = server(|mut peer| {
        let first: Value = serde_json::from_str(&text(&mut peer)).unwrap();
        let terminate: Value = serde_json::from_str(&text(&mut peer)).unwrap();
        assert_eq!(terminate["terminateId"], first["id"]);
        // The stop's terminate is still unanswered; the next query needs no second terminate.
        let next: Value = serde_json::from_str(&text(&mut peer)).unwrap();
        assert!(next.get("action").is_none(), "{next}");
        assert_eq!(next["moves"], json!([["B", "D4"]]));
        let _ = peer.read();
    });
    let mut engine = Running::custom(url);
    engine.send("1 kata-analyze B 50"); engine.stdout("=1");
    engine.send("2 stop");
    engine.send("3 play B D4");
    let lines: Vec<String> = engine.stdout("=3").into_iter()
        .filter_map(|event| match event { EngineInput::Stdout(line) => Some(line), _ => None }).collect();
    let at = |line: &str| lines.iter().position(|l| l == line).unwrap_or_else(|| panic!("{line:?} missing from {lines:?}"));
    assert!(at("") < at("=2") && at("=2") < at("=3"), "the stream ends, then each command is answered in order: {lines:?}");
    engine.send("4 kata-analyze W 50"); engine.stdout("=4");
    engine.stop(); peer.join().unwrap();
}

fn d4() -> Point { Point { x: 3, y: 15 } }

#[test]
fn custom_engine_reports_focus_unsupported_without_a_remote_query_and_keeps_analysing() {
    let peer = workbench::peer(workbench::terminate_only);
    let mut b = workbench::bench();
    b.open("(;SZ[19];B[pd];W[dd])");
    b.activate_custom(&peer.url);
    assert!(b.pump_until(Duration::from_secs(10), workbench::ready));
    b.wb.toggle_analysis().unwrap();
    assert!(b.wb.view(false).engine.paused);
    // A new instance switched in while paused still runs its start-up probe.
    assert!(b.wb.restart_engine().unwrap());
    assert!(!workbench::ready(&mut b.wb), "the restarted instance starts up again");
    assert!(b.pump_until(Duration::from_secs(10), workbench::ready));
    let v = b.wb.view(false);
    assert!(v.engine.paused);
    assert_eq!(v.focus.capability, FocusCapability::Unsupported);
    assert!(!v.focus.available, "评估此点 must not be offered");
    assert!(peer.latest_queries().is_empty(), "the probe must not cost a remote query: {:?}", peer.latest_queries());
    b.wb.toggle_analysis().unwrap();
    assert!(b.pump_until(Duration::from_secs(10), |_| !peer.latest_queries().is_empty()), "resumed analysis reaches the peer");
    b.finish();
}

#[test]
fn custom_engine_moving_forward_during_a_long_analysis_searches_the_new_node() {
    // Every query gets one in-progress result and keeps searching.
    let peer = workbench::peer(|v| {
        if v["action"] == "terminate" {
            return workbench::terminate_only(v);
        }
        let n = v["moves"].as_array().map_or(0, Vec::len);
        let mv = ["Q4", "D16"][n % 2];
        vec![json!({"id": v["id"], "isDuringSearch": true,
            "moveInfos": [{"move": mv, "visits": 50, "winrate": 0.55, "scoreLead": 1.0, "prior": 0.2, "order": 0, "pv": [mv]}],
            "rootInfo": {"visits": 60, "winrate": 0.55, "scoreLead": 1.0}})]
    });
    let mut b = workbench::bench();
    b.open("(;SZ[19];B[pd];W[dd];B[pp])");
    b.wb.navigate(Navigation::First);
    b.activate_custom(&peer.url);
    assert!(b.pump_until(Duration::from_secs(10), |wb| workbench::ready(wb) && wb.view(false).analysis.is_some()));
    let old = peer.queries().last().unwrap()["id"].clone();

    b.wb.navigate(Navigation::Forward(1));
    let candidate = |wb: &mut lizzie_core::workbench::Workbench| {
        wb.view(false).analysis.and_then(|a| a.candidates.first().and_then(|c| c.point))
    };
    assert!(
        b.pump_until(Duration::from_secs(10), |wb| candidate(wb) == Some(Point { x: 3, y: 3 })),
        "the new node gets its own search (D16), queries: {:?}",
        peer.queries().iter().map(|q| q["moves"].clone()).collect::<Vec<_>>()
    );
    assert_eq!(peer.queries().last().unwrap()["moves"], json!([["B", "Q16"]]));
    assert!(peer.terminated().contains(&old), "the superseded remote query is terminated");
    b.finish();
}

#[test]
fn custom_engine_single_move_keeps_its_evaluation() {
    // A move query gets its final answer at once with D4; analysis queries get nothing.
    let peer = workbench::peer(|v| {
        if v["action"] == "terminate" {
            return workbench::terminate_only(v);
        }
        if v["priority"] != 2 {
            return Vec::new();
        }
        vec![json!({"id": v["id"], "isDuringSearch": false,
            "moveInfos": [{"move": "D4", "visits": 120, "winrate": 0.62, "scoreLead": 2.5, "prior": 0.3, "order": 0, "pv": ["D4", "Q16"]}],
            "rootInfo": {"visits": 130, "winrate": 0.6, "scoreLead": 2.0}})]
    });
    let mut b = workbench::bench();
    b.open("(;SZ[19])");
    b.activate_custom(&peer.url);
    assert!(b.pump_until(Duration::from_secs(10), workbench::ready));
    // Paused, so no other analysis can supply the root's evaluation.
    b.wb.toggle_analysis().unwrap();
    assert!(b.wb.view(false).engine.paused);
    assert!(b.wb.view(false).analysis.is_none());

    b.wb.game(GameCommand::EngineMove).unwrap();
    assert!(b.pump_until(Duration::from_secs(10), |wb| wb.view(false).position.move_number == 1));
    assert_eq!(b.wb.document().record.node(ROOT).children.len(), 1, "D4 is played once");
    b.wb.navigate(Navigation::First);
    let root = b.wb.view(false).analysis.expect("the move's evaluation belongs to the position it was asked for");
    assert_eq!(root.candidates.first().and_then(|c| c.point), Some(d4()));
    b.finish();
}

/// 关闭所有引擎 while a 自建算力 engine is still connecting (its peer never answers the WebSocket
/// upgrade): the connection closes, the seat is empty, and nothing reports the switch as running
/// or failed — the next view shows an empty seat that is not down, which ends the 远程算力
/// window's switch (monitorEngineSwitch: the switch no longer holds the seat).
#[test]
fn shutting_all_engines_down_while_a_custom_engine_connects_empties_the_seat_and_closes_the_link() {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let url = format!("ws://{}/", listener.local_addr().unwrap());
    let (closed_tx, closed) = mpsc::channel();
    thread::spawn(move || {
        let Ok((mut stream, _)) = listener.accept() else { return };
        let mut buf = [0u8; 1024];
        while matches!(stream.read(&mut buf), Ok(n) if n > 0) {}
        let _ = closed_tx.send(());
    });
    let mut b = workbench::bench();
    b.activate_custom(&url);
    let v = b.wb.view(false);
    let cloud = v.engine.foreground.as_ref().expect("the cloud engine holds the seat while it connects");
    assert!(!cloud.ready);
    assert!(!b.pump_until(Duration::from_millis(300), workbench::ready), "the peer never completes the upgrade");
    let before = b.wb.view(false).seq;

    assert!(b.wb.shutdown_engines(lizzie_core::api::EngineScope::All));
    closed.recv_timeout(Duration::from_secs(3)).expect("the connection closes");
    b.pump_until(Duration::from_millis(300), |_| false);
    let v = b.wb.view(false);
    assert!(v.seq > before);
    assert!(v.engine.foreground.is_none());
    assert!(!v.engine.down, "a shutdown is not a failure");
    assert!(v.engine_failure.is_none());
    assert!(v.engine.engines.iter().all(|e| e.state.is_none()), "nothing left running");
    b.finish();
}
