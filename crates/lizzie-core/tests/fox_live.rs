//! Controlled GTP + actual loopback Fox peer, through the same Workbench commands as Tauri.
//! No official account/client writes: these tests prove local lifecycle and transaction ownership.
use std::{collections::HashMap, io::{Read, Write}, net::{TcpListener, TcpStream}, sync::{Arc, Mutex}, time::{Duration, Instant}};
use lizzie_core::{api::{Command, CommandResult, EditCommand, FoxCommand, FoxReplaceChoice, FoxStartOutcome}, browse::Navigation, engine::{gtp::LaunchSpec, profile::{EngineProfile, EngineStartup}, runtime::{EngineInput, EngineIo, InstanceId, Launcher}}, fox::{Framer, encode}, record::{Point, ROOT}, settings::Storage, workbench::Workbench};
const CAPS: &str = "name\nversion\nlist_commands\nboardsize\nclear_board\nkomi\nplay\nundo\nset_position\nkata-analyze\nkata-get-rules\nkata-set-rules\nkata-get-param\nkata-set-param\nkgs-time_settings\nkata-time_settings\nkata-list_time_settings\nset_free_handicap\ntime_left\nstop\nkata-search_analyze_cancellable";
const RULES: &str = r#"{"friendlyPassOk":true,"hasButton":false,"ko":"SIMPLE","scoring":"AREA","suicide":false,"tax":"NONE","whiteHandicapBonus":"N"}"#;
#[derive(Clone, Default)]
struct World(Arc<Mutex<(InstanceId, Vec<String>)>>);
struct Io(World);
impl EngineIo for Io {
    fn send(&mut self, text: &str) -> std::io::Result<()> { self.0.0.lock().unwrap().1.push(text.into()); Ok(()) }
    fn terminate(self: Box<Self>) {}
}
struct Launch(World);
impl Launcher for Launch {
    fn launch(&self, id: InstanceId, _: &LaunchSpec) -> std::io::Result<Box<dyn EngineIo>> { self.0.0.lock().unwrap().0 = id; Ok(Box::new(Io(self.0.clone()))) }
}
struct Harness {
    wb: Workbench, world: World, engine: InstanceId, params: HashMap<String,String>, rules: String,
    held: Vec<String>, sent: Vec<String>, streaming: bool, peer: Option<TcpStream>, framer: Framer,
    received: Vec<String>, dir: tempfile::TempDir, pda_readback: Option<String>,
}
impl Harness {
    fn new() -> Self {
        let dir = tempfile::tempdir().unwrap(); let world = World::default();
        let mut wb = Workbench::new(Storage::in_dirs(&dir.path().join("config"), &dir.path().join("data")), None, Box::new(Launch(world.clone())), vec![]);
        wb.set_auto_quick_analyze(false).unwrap();
        let config = dir.path().join("engine.cfg"); std::fs::write(&config, "numSearchThreads = 1\n").unwrap();
        wb.save_engine_settings(vec![EngineProfile { name:"KataGo".into(), command:format!("/opt/katago gtp -config {}", config.display()), ..EngineProfile::default() }], EngineStartup::Default).unwrap();
        wb.start_engines(); let engine = world.0.lock().unwrap().0;
        let mut h = Self { wb,world,engine,params:HashMap::from([("maxTime".into(),"12.5".into()),("maxVisits".into(),"999".into()),("playoutDoublingAdvantage".into(),"0".into())]),rules:RULES.into(),held:vec![],sent:vec![],streaming:false,peer:None,framer:Framer::default(),received:vec![],dir,pda_readback:None };
        h.serve(None); h
    }
    fn output(&mut self, text: &str) { self.wb.engine_input(self.engine, EngineInput::Stdout(text.into())); }
    fn reply(&mut self, id: &str, text: &str) { self.output(&format!("={id} {text}")); self.output(""); }
    fn serve(&mut self, reject: Option<&str>) {
        for _ in 0..1000 {
            let lines = std::mem::take(&mut self.world.0.lock().unwrap().1);
            if lines.is_empty() { return; }
            for line in lines {
                let (id, command) = line.trim().split_once(' ').unwrap();
                self.sent.push(command.into());
                if std::mem::take(&mut self.streaming) { self.output(""); }
                if command.starts_with("kata-search_analyze_cancellable") { self.held.push(id.into()); continue; }
                if command.starts_with("kata-analyze") { self.output(&format!("={id}")); self.streaming=true; continue; }
                if reject.is_some_and(|prefix|command.starts_with(prefix)) { self.output(&format!("?{id} rejected")); self.output(""); continue; }
                let answer = match command {
                    "name" => "KataGo".into(), "version" => "1.18.2".into(), "list_commands" => CAPS.into(),
                    "kata-get-rules" => self.rules.clone(), "kata-list_time_settings" => "none absolute byoyomi".into(),
                    _ if command.starts_with("kata-set-rules ") => { self.rules=command[15..].into(); String::new() },
                    "kata-get-param playoutDoublingAdvantage" if self.pda_readback.is_some() => self.pda_readback.clone().unwrap(),
                    _ if command.starts_with("kata-get-param ") => self.params.get(&command[15..]).unwrap().clone(),
                    _ if command.starts_with("kata-set-param ") => { let (key,value)=command[15..].split_once(' ').unwrap(); self.params.insert(key.into(),value.into()); String::new() },
                    _ => String::new(),
                };
                self.reply(id,&answer);
            }
        }
        panic!("controlled engine did not quiesce");
    }
    fn prepare(&mut self) { self.wb.fox(FoxCommand::Prepare).unwrap(); self.serve(None); assert!(self.wb.inspect().fox.can_start); }
    fn start(&mut self) {
        self.prepare();
        let probe=TcpListener::bind(("127.0.0.1",0)).unwrap(); let port=probe.local_addr().unwrap().port(); drop(probe);
        let document=self.wb.document(); let (document,revision)=(document.id,document.revision);
        self.wb.fox(FoxCommand::Start { port,max_time:12.5,max_visits:500,warning_confirmed:true,replace:FoxReplaceChoice::Replace,document,revision }).unwrap();
        self.serve(None); assert!(self.wb.inspect().fox.listening); assert!(!self.wb.inspect().fox.ready);
        let peer=TcpStream::connect(("127.0.0.1",port)).unwrap(); peer.set_nonblocking(true).unwrap(); peer.set_nodelay(true).unwrap(); self.peer=Some(peer);
        self.wait_for(|h| h.wb.inspect().fox.connection != 0);
    }
    fn send(&mut self, bodies: &[&str]) {
        let mut bytes=Vec::new(); for body in bodies { bytes.extend(encode(body).unwrap()); }
        self.peer.as_mut().unwrap().write_all(&bytes).unwrap(); self.pump();
    }
    fn pump(&mut self) {
        for _ in 0..64 {
            self.wb.engine_tick(Instant::now()); self.serve(None);
            let mut bytes=[0;8192];
            if let Some(peer)=&mut self.peer { match peer.read(&mut bytes) {
                Ok(n) if n>0 => self.framer.push(&bytes[..n], |body| self.received.push(body)).unwrap(),
                Ok(_) => {}, Err(e) if e.kind()==std::io::ErrorKind::WouldBlock => {}, Err(e)=>panic!("peer read: {e}"),
            } }
            std::thread::yield_now();
        }
    }
    fn wait_for(&mut self, ready: impl Fn(&mut Self) -> bool) {
        let until = Instant::now() + Duration::from_secs(2);
        loop {
            self.pump();
            if ready(self) { return; }
            assert!(Instant::now() < until, "Fox transition timed out: {:?}", self.wb.inspect().fox);
            std::thread::sleep(Duration::from_millis(1));
        }
    }
    fn initialize(&mut self) {
        self.send(&["FARULE,19,60,30,3,650","FASTATUS,1,B","FATIMELEFT,60,30,3","FAMOVE,1,B,0^0^0^N"]);
        self.wait_for(|h| h.received.iter().any(|s|s=="AFSTATUS,B"));
        self.send(&["FATIMELEFT,59,30,3"]); self.wait_for(|h| h.held.len()==1);
    }
    fn choose(&mut self, vertex: &str) {
        let before = self.received.iter().filter(|s| s.starts_with("AFPLAY,")).count();
        let id=self.held.remove(0); self.reply(&id,vertex);
        self.wait_for(|h| h.received.iter().filter(|s| s.starts_with("AFPLAY,")).count() > before);
    }
}
#[test]
fn choice_is_pending_until_official_confirmation_and_browsing_never_changes_live_search() {
    let mut h=Harness::new(); h.start(); h.initialize();
    assert_eq!(h.wb.cursor(),ROOT); h.choose("R16");
    assert_eq!(h.wb.cursor(),ROOT); assert!(!h.wb.document().record.has_actions());
    assert!(h.received.iter().any(|s|s=="AFPLAY,B,1^16^3^B"));
    let view=h.wb.inspect().fox; assert!(view.pending.as_ref().unwrap().written); assert_eq!(view.submissions[0].outcome,"Pending");
    h.send(&["FASTATUS,1,B"]); assert!(!h.wb.document().record.has_actions());
    h.send(&["FAMOVE,0,B,1^16^3^B"]);
    h.wait_for(|h| h.wb.inspect().fox.live_node != Some(ROOT));
    let live=h.wb.inspect().fox.live_node.unwrap(); assert_ne!(live,ROOT); assert_eq!(h.wb.cursor(),live);
    assert_eq!(h.wb.inspect().fox.submissions[0].outcome,"Confirmed");
    h.wb.navigate(Navigation::First); assert_eq!(h.wb.cursor(),ROOT); assert!(h.wb.inspect().fox.browsing);
    assert!(h.wb.inspect().analysis.is_none());
    h.send(&["FAMOVE,1,B,2^3^15^W","FATIMELEFT,58,30,3"]);
    h.wait_for(|h| h.wb.inspect().fox.live_node != Some(live));
    assert_eq!(h.wb.cursor(),ROOT); assert_ne!(h.wb.inspect().fox.live_node,Some(live));
    assert!(matches!(h.wb.edit(EditCommand::Play{point:Point{x:0,y:0},color:None}).unwrap(),CommandResult::Refused{..}));
    assert!(h.wb.game_gate(&Command::SetKomi{komi:7.5}).is_some());
    assert!(h.wb.game_gate(&Command::Navigate{nav:Navigation::Last}).is_none());
    h.wb.fox(FoxCommand::ReturnLive).unwrap(); assert_eq!(Some(h.wb.cursor()),h.wb.inspect().fox.live_node);
}
#[test]
fn stop_invalidates_late_choice_and_confirms_rules_limits_no_clock_and_position_before_release() {
    let mut h=Harness::new(); h.start(); h.initialize(); let late=h.held.remove(0);
    h.wb.fox(FoxCommand::Stop).unwrap(); assert!(h.wb.inspect().fox.restoring); assert!(h.wb.inspect().fox.owns_board);
    h.reply(&late,"D4"); assert!(!h.wb.document().record.has_actions());
    h.serve(None); h.pump();
    let view=h.wb.inspect().fox; assert!(!view.owns_board); assert!(!view.restoring); assert_eq!(view.status_key,"Fox.stopped");
    assert_eq!(h.params["maxTime"],"12.5"); assert_eq!(h.params["maxVisits"],"999");
    assert!(h.sent.iter().any(|c|c=="kata-time_settings none"));
    assert!(!h.received.iter().any(|s|s.starts_with("AFPLAY")));
}
#[test]
fn restore_refusal_keeps_engine_unavailable_and_preserves_confirmed_history() {
    let mut h=Harness::new(); h.start(); h.initialize(); h.choose("R16"); h.send(&["FAMOVE,0,B,1^16^3^B"]);
    h.wait_for(|h| h.wb.cursor() != ROOT);
    let nodes=h.wb.document().record.path_to(h.wb.cursor());
    h.wb.fox(FoxCommand::Stop).unwrap(); h.serve(Some("kata-time_settings none"));
    h.wb.engine_tick(Instant::now()); h.serve(None);
    assert_eq!(h.wb.inspect().fox.status_key,"Fox.unrestored");
    assert_eq!(h.wb.document().record.path_to(h.wb.cursor()),nodes);
    h.wb.fox(FoxCommand::Prepare).unwrap(); assert!(!h.wb.inspect().fox.can_start);
}
#[test]
fn expired_turn_and_unconfirmed_write_fail_closed_without_an_optimistic_move() {
    let mut h=Harness::new(); h.start(); h.initialize(); h.choose("R16");
    h.wb.engine_tick(Instant::now()+Duration::from_secs(6)); h.serve(None);
    assert_eq!(h.wb.inspect().fox.status_key,"Fox.confirmationFailed");
    assert!(!h.wb.document().record.has_actions());
    let count=h.received.iter().filter(|s|s.starts_with("AFPLAY")).count(); h.pump();
    assert_eq!(h.received.iter().filter(|s|s.starts_with("AFPLAY")).count(),count);
}
#[test]
fn start_requires_warning_and_a_document_bound_replacement_and_save_exits_attempt() {
    let mut h=Harness::new(); h.prepare(); h.wb.edit(EditCommand::Play{point:Point{x:3,y:3},color:None}).unwrap();
    let v=h.wb.inspect(); let (document,revision)=(v.document.id,v.document.revision);
    let start=|warning_confirmed,replace|FoxCommand::Start{port:7000,max_time:0.0,max_visits:0,warning_confirmed,replace,document,revision};
    h.wb.fox(start(false,FoxReplaceChoice::Ask)).unwrap(); assert_eq!(h.wb.inspect().fox.outcome,Some(FoxStartOutcome::NeedsWarning));
    h.wb.fox(start(true,FoxReplaceChoice::Ask)).unwrap(); assert_eq!(h.wb.inspect().fox.outcome,Some(FoxStartOutcome::NeedsReplacement{document,revision}));
    h.wb.fox(start(true,FoxReplaceChoice::Save)).unwrap(); assert_eq!(h.wb.inspect().fox.outcome,Some(FoxStartOutcome::SaveRequested)); assert!(!h.wb.inspect().fox.listening);
    assert!(h.dir.path().exists());
}

#[test]
fn pda_edit_stops_the_search_and_only_publishes_acknowledged_readback() {
    let mut h=Harness::new(); h.start(); h.initialize(); let late=h.held.remove(0);
    h.wb.fox(FoxCommand::SetPda{value:0.5}).unwrap();
    assert_eq!(h.wb.inspect().fox.desired_pda,Some(0.5));
    assert_eq!(h.wb.inspect().fox.confirmed_pda,Some(0.0));
    h.reply(&late,"D4"); h.serve(None); h.pump();
    assert_eq!(h.wb.inspect().fox.confirmed_pda,Some(0.5));
    let stored: serde_json::Value=serde_json::from_slice(&std::fs::read(&h.wb.storage().config_file).unwrap()).unwrap();
    assert_eq!(stored["engine"]["kataPda"],serde_json::json!(0.5));
    assert_eq!(stored["engine"]["kataPdaEnabled"],serde_json::json!(true));
    assert!(!h.received.iter().any(|s|s.starts_with("AFPLAY")));
    let setter=h.sent.iter().position(|s|s=="kata-set-param playoutDoublingAdvantage 0.5").unwrap();
    assert_eq!(h.sent[setter-1],"stop");
}

#[test]
fn refused_or_mismatched_pda_preserves_the_last_confirmed_preference() {
    for refuse_setter in [true,false] {
        let mut h=Harness::new(); h.start(); h.initialize(); let late=h.held.remove(0);
        h.wb.fox(FoxCommand::SetPda{value:0.5}).unwrap(); h.reply(&late,"D4"); h.serve(None); h.pump();
        assert_eq!(h.wb.inspect().fox.confirmed_pda,Some(0.5));
        h.wb.fox(FoxCommand::SetPda{value:0.8}).unwrap();
        if !refuse_setter { h.pda_readback=Some("0.7".into()); }
        h.serve(refuse_setter.then_some("kata-set-param playoutDoublingAdvantage")); h.pump();
        assert_eq!(h.wb.inspect().fox.status_key,"Fox.pdaFailed");
        assert_eq!(h.wb.inspect().fox.confirmed_pda,Some(0.5));
        let stored:serde_json::Value=serde_json::from_slice(&std::fs::read(&h.wb.storage().config_file).unwrap()).unwrap();
        assert_eq!(stored["engine"]["kataPda"],serde_json::json!(0.5));
        assert_eq!(stored["engine"]["kataPdaEnabled"],serde_json::json!(true));
        assert!(!h.received.iter().any(|s|s.starts_with("AFPLAY")));
    }
}

#[test]
fn disconnect_with_a_written_unconfirmed_move_never_replays_that_choice() {
    let mut h=Harness::new(); h.start(); h.initialize(); h.choose("R16");
    h.peer.take(); h.pump();
    h.wait_for(|h| h.wb.inspect().fox.status_key=="Fox.disconnected");
    assert_eq!(h.wb.inspect().fox.status_key,"Fox.disconnected");
    assert!(!h.wb.document().record.has_actions());
    let port=h.wb.inspect().fox.port;
    let connection=h.wb.inspect().fox.connection;
    let peer=TcpStream::connect(("127.0.0.1",port)).unwrap(); peer.set_nonblocking(true).unwrap(); peer.set_nodelay(true).unwrap(); h.peer=Some(peer);
    h.wait_for(|h| h.wb.inspect().fox.connection > connection);
    h.send(&["FARULE,19,60,30,3,650","FASTATUS,1,B","FATIMELEFT,58,30,3","FAMOVE,1,B,0^0^0^N"]);
    h.wait_for(|h| h.wb.inspect().fox.status_key != "Fox.syncing");
    assert!(!h.wb.document().record.has_actions());
    assert!(!h.wb.inspect().fox.pending.as_ref().is_some_and(|p|p.written));
}

#[test]
fn confirmed_reconnect_restores_turn_status_and_preserves_the_document() {
    let mut h=Harness::new(); h.start(); h.initialize(); h.choose("R16");
    h.send(&["FAMOVE,0,B,1^16^3^B"]);
    h.wait_for(|h| h.wb.cursor() != ROOT);
    let before=h.wb.inspect();
    h.peer.take();
    h.wait_for(|h| h.wb.inspect().fox.status_key=="Fox.disconnected");
    let peer=TcpStream::connect(("127.0.0.1",before.fox.port)).unwrap();
    peer.set_nonblocking(true).unwrap(); peer.set_nodelay(true).unwrap(); h.peer=Some(peer);
    h.wait_for(|h| h.wb.inspect().fox.connection > before.fox.connection);
    h.send(&["FARULE,19,60,30,3,650","FASTATUS,1,B,1^16^3^B","FAMOVE,0,B,1^16^3^B","FATIMELEFT,58,30,3"]);
    h.wait_for(|h| { let f=h.wb.inspect().fox; f.game > before.fox.game && f.ready });
    let after=h.wb.inspect();
    assert_eq!(after.fox.status_key,"Fox.opponentTurn");
    assert_eq!(after.document.id,before.document.id);
    assert_eq!(after.fox.live_node,before.fox.live_node);
    assert_eq!(after.position.move_number,1);
}

#[test]
fn terminal_restores_and_releases_the_engine_without_stopping_the_listener() {
    let mut h=Harness::new(); h.start(); h.initialize(); h.choose("R16");
    h.send(&["FAMOVE,0,B,1^16^3^B"]); h.wait_for(|h| h.wb.cursor()!=ROOT);
    let live=h.wb.cursor();
    h.send(&["FASTATUS,0"]);
    h.wait_for(|h| h.wb.inspect().fox.status_key=="Fox.terminal");
    let view=h.wb.inspect();
    assert!(!view.fox.owns_board,"terminal must return board ownership");
    assert!(view.fox.listening); assert_eq!(h.wb.cursor(),live);
    assert_eq!(h.params["maxTime"],"12.5"); assert_eq!(h.params["maxVisits"],"999");
    assert!(h.sent.iter().any(|s|s=="kata-time_settings none"));
    for terminal in ["FASTATUS,0","FARESULT,W+R"] {
        let revision=h.wb.inspect().fox.revision;
        h.send(&[terminal]);
        h.wait_for(|h| h.wb.inspect().fox.revision>revision);
        assert!(!h.wb.inspect().fox.owns_board,"repeated terminal must keep ownership released");
        assert!(h.wb.inspect().fox.listening); assert_eq!(h.wb.cursor(),live);
    }
    assert!(!matches!(h.wb.edit(EditCommand::Play{point:Point{x:3,y:3},color:None}).unwrap(),CommandResult::Refused{..}));
}

#[test]
fn duplicate_clients_cannot_cancel_search_or_invalidate_written_confirmation() {
    let mut h=Harness::new(); h.start(); h.initialize();
    let before=h.wb.inspect().fox;
    let duplicate=TcpStream::connect(("127.0.0.1",before.port)).unwrap();
    h.wait_for(|h| h.wb.inspect().fox.status_key=="Fox.duplicateClient");
    assert!(h.wb.inspect().fox.ready,"duplicate peer must not stop the current search");
    drop(duplicate); h.choose("R16");
    let duplicate=TcpStream::connect(("127.0.0.1",before.port)).unwrap();
    h.wait_for(|h| h.wb.inspect().fox.status_key=="Fox.duplicateClient");
    let pending=h.wb.inspect().fox;
    assert_eq!(pending.connection,before.connection); assert_eq!(pending.game,before.game);
    assert!(pending.pending.is_some_and(|p|p.written)); drop(duplicate);
    h.send(&["FAMOVE,0,B,1^16^3^B"]); h.wait_for(|h| h.wb.cursor()!=ROOT);
    assert_eq!(h.wb.inspect().fox.submissions[0].outcome,"Confirmed");
}

#[test]
fn valid_confirmation_before_a_bad_frame_is_committed_before_disconnect() {
    let mut h=Harness::new(); h.start(); h.initialize(); h.choose("R16");
    let mut frames=encode("FAMOVE,0,B,1^16^3^B").unwrap();
    frames.extend_from_slice(b"$BAD*00\r\n");
    h.peer.as_mut().unwrap().write_all(&frames).unwrap();
    h.wait_for(|h| h.wb.inspect().fox.status_key=="Fox.disconnected");
    assert_eq!(h.wb.inspect().position.move_number,1,"validated prefix is an authoritative move");
    assert_eq!(h.wb.inspect().fox.submissions[0].outcome,"Confirmed");
}

/// 棋盘同步工具's helper, scripted: it only has to exist; its lines are fed by the test.
struct Helper(Arc<Mutex<Option<lizzie_core::readboard::HelperId>>>);
impl lizzie_core::readboard::HelperHost for Helper {
    fn launch(&mut self, helper: lizzie_core::readboard::HelperId, _: &lizzie_core::readboard::HelperLaunch) -> std::io::Result<()> { *self.0.lock().unwrap() = Some(helper); Ok(()) }
    fn send(&mut self, _: lizzie_core::readboard::HelperId, _: &str) -> std::io::Result<()> { Ok(()) }
    fn stop(&mut self, _: lizzie_core::readboard::HelperId) -> bool { false }
}

#[test]
fn a_board_sync_helper_never_writes_into_the_fox_live_record() {
    // Java's Board drops the helper thread's place/addStone/reopen/clear/setHistory while the
    // Fox session owns the board (FoxLiveSession.rejectOrdinaryMutation, Board.java:938, 2739, 5438).
    use lizzie_core::{api::ReadBoardCommand, readboard::HelperInput};
    let mut h=Harness::new();
    let base=h.dir.path().join("app"); std::fs::create_dir_all(base.join("readboard")).unwrap();
    std::fs::write(base.join("readboard").join("readboard.exe"), b"helper").unwrap();
    let launched=Arc::new(Mutex::new(None));
    h.wb.set_readboard_host(Box::new(Helper(launched.clone())), vec![base]);
    assert_eq!(h.wb.readboard(ReadBoardCommand::Open).unwrap(), CommandResult::Done);
    let helper=launched.lock().unwrap().expect("the helper started");
    for line in ["ready","start 19 19","sync"] { h.wb.readboard_input(helper, HelperInput::Line(line.into())); }
    h.serve(None);
    h.start(); h.initialize(); h.choose("R16"); h.send(&["FAMOVE,0,B,1^16^3^B"]);
    h.wait_for(|h| h.wb.cursor() != ROOT);
    let (live, doc, revision)=(h.wb.inspect().fox.live_node, h.wb.document().id, h.wb.document().revision);
    // The helper reads a different board: a rebuilt snapshot and a further move.
    for frame in [&[(3usize,3usize,1u8),(15,15,4)][..], &[(3,3,1),(15,15,2),(16,16,3)][..]] {
        for y in 0..19 {
            let row: Vec<String>=(0..19).map(|x| frame.iter().find(|s| s.0==x && s.1==y).map_or("0".into(), |s| s.2.to_string())).collect();
            h.wb.readboard_input(helper, HelperInput::Line(format!("re={}", row.join(","))));
        }
        h.wb.readboard_input(helper, HelperInput::Line("end".into()));
        h.pump();
    }
    assert_eq!((h.wb.document().id, h.wb.document().revision), (doc, revision), "the live record is untouched");
    assert_eq!(h.wb.inspect().fox.live_node, live);
    h.send(&["FAMOVE,1,B,2^3^15^W","FATIMELEFT,58,30,3"]);
    h.wait_for(|h| h.wb.inspect().fox.live_node != live);
    assert_eq!(h.wb.inspect().position.move_number, 2, "the Fox game goes on");
    assert_ne!(h.wb.inspect().fox.status_key, "Fox.incomplete");
}
