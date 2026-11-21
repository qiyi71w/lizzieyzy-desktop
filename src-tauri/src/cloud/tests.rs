use super::*;
use std::io::{Read, Write};
use std::net::{TcpListener, TcpStream};
use std::sync::mpsc;
use std::thread;

struct Request { method: String, path: String, authorization: String, body: Value }
struct Reply { status: u16, headers: &'static str, body: Value }
impl Reply { fn ok(body: Value) -> Self { Self { status:200, headers:"",body } } }
fn read_request(stream: &mut TcpStream) -> Request {
    stream.set_read_timeout(Some(Duration::from_secs(5))).unwrap();
    let mut bytes = Vec::new();
    let mut byte = [0];
    while !bytes.ends_with(b"\r\n\r\n") { stream.read_exact(&mut byte).unwrap(); bytes.push(byte[0]); }
    let header = String::from_utf8(bytes).unwrap();
    let mut first = header.lines().next().unwrap().split_whitespace();
    let method = first.next().unwrap().to_owned();
    let path = first.next().unwrap().to_owned();
    let header_value = |name: &str| header.lines().filter_map(|l| l.split_once(':')).find(|(key,_)| key.eq_ignore_ascii_case(name)).map(|(_,value)| value.trim().to_owned()).unwrap_or_default();
    let size = header_value("content-length").parse().unwrap_or(0);
    let mut body = vec![0;size]; stream.read_exact(&mut body).unwrap();
    Request { method,path,authorization:header_value("authorization"),body:serde_json::from_slice(&body).unwrap_or(Value::Null) }
}
fn fixture(mut replies: impl FnMut(Request) -> Reply + Send + 'static, count: usize) -> (String,thread::JoinHandle<()>) {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    listener.set_nonblocking(true).unwrap();
    let base = format!("http://{}",listener.local_addr().unwrap());
    let thread = thread::spawn(move || {
        for _ in 0..count {
            let deadline = Instant::now()+Duration::from_secs(10);
            let mut stream = loop {
                match listener.accept() {
                    Ok((stream,_)) => break stream,
                    Err(e) if e.kind() == std::io::ErrorKind::WouldBlock && Instant::now()<deadline => thread::sleep(Duration::from_millis(5)),
                    Err(e) => panic!("fixture accept: {e}"),
                }
            };
            let reply = replies(read_request(&mut stream));
            let body = reply.body.to_string();
            let response = format!("HTTP/1.1 {} Test\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n{}\r\n{}",reply.status,body.len(),reply.headers,body);
            let _ = stream.write_all(response.as_bytes());
        }
    });
    (base,thread)
}
fn adapter(base: String) -> Adapter {
    let app = format!("cloud-test-{}-{}",std::process::id(),now_millis());
    let mut adapter = Adapter::new(crate::secrets::Store::memory(&app,None));
    adapter.base = base;
    adapter
}
fn network() -> NetworkSettings { NetworkSettings {proxy_mode:lizzie_core::settings::ProxyMode::Direct,..NetworkSettings::default()} }
fn password_login(adapter:&Adapter) -> CloudReply {
    adapter.dispatch(&CloudSettings::default(),&network(),CloudCommand::PasswordLogin {
        identifier:"test@example.invalid".into(),password:"account-password".into(),remember_token:false,remember_password:false,
    }).unwrap()
}

#[test]
fn session_only_login_caches_overview_and_fetches_fresh_engine_tokens() {
    let mut tokens = 0;
    let (base,server) = fixture(move |r| {
        match r.path.as_str() {
            "/api/cluster/account/login" => {
                assert_eq!(r.method,"POST"); assert_eq!(r.body,json!({"email":"test@example.invalid","password":"account-password"}));
                assert!(r.authorization.is_empty()); Reply::ok(json!({"token":"private-account-token"}))
            },
            "/api/cluster/account/me" => Reply::ok(json!({"email":"test@example.invalid","isMembership":true,"membershipExpiresAt":"2027-02-01T12:00:00Z","membershipAutoRenew":false})),
            "/api/cluster/balance" => Reply::ok(json!({"remainingBalance":"12.3456789","totalDuration":-1,"currentNumOfNodes":2})),
            "/api/cluster/usage/my-usages?page=0&pageSize=5" => Reply::ok(json!({"total":1,"items":[{"duration":14,"share":true,"totalCost":"0.000123"}]})),
            "/api/cluster/account/fetch-socketio-token" => {
                assert_eq!(r.authorization,format!("{} {}","Bearer","private-account-token")); assert_eq!(r.body["args"],VIP_ARGS);
                tokens+=1; Reply::ok(json!({"token":format!("socket-{tokens}"),"socketIOURL":"wss://engine.example.invalid"}))
            },
            _ => panic!("unexpected account request"),
        }
    },6);
    let adapter=adapter(base);
    let login=password_login(&adapter);
    assert!(login.state.authenticated && login.state.has_password);
    assert!(!login.settings.remember_token && !login.settings.remember_password);
    let overview=adapter.dispatch(&login.settings,&network(),CloudCommand::Overview {force:false}).unwrap();
    let cached=adapter.dispatch(&login.settings,&network(),CloudCommand::Overview {force:false}).unwrap();
    assert_eq!(overview.result,cached.result);
    if let CloudResult::Overview {overview} = overview.result {
        assert_eq!(overview.balance.remaining_balance_yuan,"12.3456789");
        assert_eq!(overview.balance.total_duration_seconds,0);
        assert!(overview.account.membership); assert!(overview.recent_usage.items[0].shared);
    } else {panic!("overview result expected");}
    let generation=login.state.generation;
    assert_eq!(adapter.token(&login.settings.identifier,"").unwrap().token,"socket-1");
    assert_eq!(adapter.token(&login.settings.identifier,"").unwrap().token,"socket-2");
    assert!(adapter.is_current(generation));
    let prefs=adapter.dispatch(&login.settings,&network(),CloudCommand::Remember {token:false,password:false}).unwrap();
    assert!(prefs.state.authenticated && prefs.state.has_password);
    let serialized=serde_json::to_string(&prefs).unwrap();
    assert!(!serialized.contains("private-account-token") && !serialized.contains("account-password"));
    let logout=adapter.dispatch(&prefs.settings,&network(),CloudCommand::Logout).unwrap();
    assert!(!logout.state.authenticated && !logout.state.has_password);
    assert!(!adapter.is_current(generation));
    assert_eq!(adapter.token(&login.settings.identifier,"").err().unwrap().key,"login_required");
    server.join().unwrap();
}

#[test]
fn explicit_top_up_requires_confirmation_and_verifies_actual_account_settlement() {
    let mut balance_reads=0;
    let mut credit_reads=0;
    let order=json!({"id":"0123456789abcdef01234567","amount":1000,"paidStatus":"SUCCESS","paidAt":"2026-11-07T00:00:00Z"});
    let (base,server)=fixture(move |r| {
        if r.path=="/api/cluster/account/login" {return Reply::ok(json!({"token":"private-token"}));}
        assert_eq!(r.authorization,"Bearer private-token");
        match r.path.as_str() {
            "/api/cluster/balance" => {balance_reads+=1; Reply::ok(if balance_reads==1 {json!({"remainingBalance":"1.00","totalCashAmount":"20.00"})} else {json!({"remainingBalance":"10.99","totalCashAmount":"30.00"})})},
            "/api/pay/orders" => {
                assert_eq!(r.body,json!({"payType":"WECHAT","amount":1000,"tradeType":"NATIVE","body":"LizzieYzy Next account top-up"})); Reply::ok(order.clone())
            },
            "/api/pay/orders/0123456789abcdef01234567" => Reply::ok(order.clone()),
            "/api/cluster/credit/my-credits?page=0&pageSize=20&creditType=CASH" => {
                credit_reads+=1;
                Reply::ok(if credit_reads==1 {json!({"items":[]})} else {json!({"items":[{"creditType":"CASH","source":"PAYMENT","amount":"10.00","createdAt":chrono::Utc::now().to_rfc3339()}]})})
            },
            _ => panic!("unexpected payment request"),
        }
    },9);
    let adapter=adapter(base); let login=password_login(&adapter); let settings=&login.settings;
    let purchase=Purchase::TopUp {amount_fen:1000};
    assert_eq!(adapter.dispatch(settings,&network(),CloudCommand::CreateOrder {purchase:purchase.clone(),confirmed:false}).unwrap_err().key,"confirmation_required");
    assert_eq!(adapter.dispatch(settings,&network(),CloudCommand::CreateOrder {purchase:Purchase::TopUp {amount_fen:999},confirmed:true}).unwrap_err().key,"invalid_top_up_amount");
    let created=adapter.dispatch(settings,&network(),CloudCommand::CreateOrder {purchase,confirmed:true}).unwrap();
    assert!(matches!(created.result,CloudResult::Order {..}));
    for settled in [false,true] {
        let reply=adapter.dispatch(settings,&network(),CloudCommand::VerifyPayment {id:"0123456789abcdef01234567".into()}).unwrap();
        if let CloudResult::PaymentVerification {verification,..}=reply.result {
            assert_eq!(verification.settled,settled); assert!(verification.remaining_balance_changed); assert_eq!(verification.cash_credit_found,settled);
        } else {panic!("verification result expected");}
    }
    server.join().unwrap();
}

#[test]
fn code_cooldown_reset_and_unauthorized_account_invalidation() {
    let (base,server)=fixture(move |r| {
        match r.path.as_str() {
            "/api/cluster/account/send-code" => {
                assert_eq!(r.body,json!({"phone":"123456789","type":"reset_password"}));
                Reply {status:200,headers:"Retry-After: 12\r\n",body:json!({})}
            },
            "/api/cluster/account/reset-password" => {
                assert_eq!(r.body,json!({"phone":"123456789","verificationCode":"123456","password":"new-password"}));
                Reply::ok(json!({"token":"reset-token"}))
            },
            "/api/cluster/account/me" => Reply {status:401,headers:"",body:json!({"key":"unauthorized","detail":"reset-token new-password"})},
            _ => panic!("unexpected authentication request"),
        }
    },3);
    let adapter=adapter(base); let settings=CloudSettings::default();
    let sent=adapter.dispatch(&settings,&network(),CloudCommand::SendCode {identifier:"123456789".into(),purpose:CodePurpose::ResetPassword}).unwrap();
    assert_eq!(sent.result,CloudResult::CodeSent {retry_after_seconds:12});
    let cooldown=adapter.dispatch(&settings,&network(),CloudCommand::SendCode {identifier:"123456789".into(),purpose:CodePurpose::ResetPassword}).unwrap_err();
    assert!(cooldown.retry_after_seconds>0 && cooldown.retry_after_seconds<=13);
    let reset=adapter.dispatch(&settings,&network(),CloudCommand::ResetPassword {identifier:"123456789".into(),code:"123456".into(),password:"new-password".into(),remember_token:false,remember_password:false}).unwrap();
    assert!(reset.state.authenticated);
    let failure=adapter.dispatch(&reset.settings,&network(),CloudCommand::Overview {force:true}).unwrap_err();
    assert!(failure.authentication_invalidated); assert_eq!(failure.key,"unauthorized");
    assert!(!format!("{failure:?}").contains("reset-token"));
    let state=adapter.dispatch(&reset.settings,&network(),CloudCommand::State).unwrap();
    assert!(!state.state.authenticated && state.state.has_password);
    server.join().unwrap();
}

#[test]
fn logout_retires_a_pending_login_before_credentials_can_be_committed() {
    let (arrived_tx,arrived_rx)=mpsc::channel();
    let (release_tx,release_rx)=mpsc::channel();
    let (base,server)=fixture(move |_| {
        arrived_tx.send(()).unwrap(); release_rx.recv_timeout(Duration::from_secs(5)).unwrap();
        Reply::ok(json!({"token":"late-token"}))
    },1);
    let adapter=Arc::new(adapter(base));
    let pending=adapter.clone();
    let login=thread::spawn(move || pending.dispatch(&CloudSettings::default(),&network(),CloudCommand::PasswordLogin {
        identifier:"test@example.invalid".into(),password:"account-password".into(),remember_token:false,remember_password:false,
    }));
    arrived_rx.recv_timeout(Duration::from_secs(5)).unwrap();
    let settings=CloudSettings {identifier:"test@example.invalid".into(),..CloudSettings::default()};
    let logout=adapter.dispatch(&settings,&network(),CloudCommand::Logout).unwrap();
    assert!(!logout.state.authenticated);
    release_tx.send(()).unwrap();
    assert!(login.join().unwrap().unwrap_err().cancelled);
    let state=adapter.dispatch(&settings,&network(),CloudCommand::State).unwrap();
    assert!(!state.state.authenticated && !state.state.has_password);
    server.join().unwrap();
}

#[test]
fn code_login_pages_and_membership_purchase_follow_server_products() {
    let mut account_reads=0;
    let (base,server)=fixture(move |r| {
        match r.path.as_str() {
            "/api/cluster/account/fast-login" => {
                assert_eq!(r.body,json!({"email":"member@example.invalid","verificationCode":"654321"}));
                Reply::ok(json!({"token":"member-token"}))
            },
            "/api/cluster/product?type=MEMBERSHIP" => Reply::ok(json!([
                {"type":"MEMBERSHIP","name":"MEMBERSHIP_3_MONTH","price":"9000.00"},
                {"type":"MEMBERSHIP","name":"MEMBERSHIP_1_MONTH","price":3000},
                {"type":"MEMBERSHIP","name":"UNSUPPORTED","price":1},
                {"type":"MEMBERSHIP","name":"MEMBERSHIP_6_MONTH","price":"0.1"}
            ])),
            "/api/cluster/usage/my-usages?page=3&pageSize=1&finished=false" => Reply::ok(json!({"total":8,"page":3,"pageSize":1,"items":[{"engineType":"go","gpuType":"vip-share","duration":17,"vip":true,"finished":false}]})),
            "/api/cluster/credit/my-credits?page=2&pageSize=100&creditType=BONUS+CREDITS" => Reply::ok(json!({"items":[{"creditType":"BONUS CREDITS","amount":"0.00001","source":"GIFT","productName":"gift"}]})),
            "/api/cluster/account/me" => {
                account_reads+=1;
                Reply::ok(json!({"isMembership":true,"membershipExpiresAt":if account_reads==1 {"2027-01-01T00:00:00Z"} else {"2027-04-01T00:00:00Z"}}))
            },
            "/api/pay/orders" => {
                assert_eq!(r.body,json!({"payType":"WECHAT","amount":9000,"tradeType":"NATIVE","body":"LizzieYzy Next VIP membership","orderType":"PURCHASE_PRODUCT","productName":"MEMBERSHIP_3_MONTH","extraInfo":{"autoRenew":true}}));
                Reply::ok(json!({"id":"abcdefabcdefabcdefabcdef","amount":9000,"paidStatus":"PENDING","orderType":"PURCHASE_PRODUCT","productName":"MEMBERSHIP_3_MONTH","nativePayRequest":{"codeURL":"weixin://wxpay/test"}}))
            },
            "/api/pay/orders/abcdefabcdefabcdefabcdef" => Reply::ok(json!({"id":"abcdefabcdefabcdefabcdef","amount":9000,"paidStatus":"SUCCESS","productName":"MEMBERSHIP_3_MONTH"})),
            _ => panic!("unexpected membership request"),
        }
    },8);
    let adapter=adapter(base);
    let login=adapter.dispatch(&CloudSettings::default(),&network(),CloudCommand::CodeLogin {identifier:"member@example.invalid".into(),code:"654321".into(),remember_token:false}).unwrap();
    let settings=&login.settings;
    let products=adapter.dispatch(settings,&network(),CloudCommand::Products).unwrap();
    if let CloudResult::Products {products,..}=products.result {
        assert_eq!(products.iter().map(|p|p.duration_months).collect::<Vec<_>>(),[1,3]);
        assert_eq!(products[1].price_fen,9000);
    } else {panic!("products expected");}
    let usage=adapter.dispatch(settings,&network(),CloudCommand::Usages {page:3,page_size:0,finished:Some(false)}).unwrap();
    assert!(matches!(usage.result,CloudResult::Usages {page:UsagePage {total:8,page:3,page_size:1,..}}));
    let credits=adapter.dispatch(settings,&network(),CloudCommand::Credits {page:2,page_size:999,credit_type:Some("BONUS CREDITS".into())}).unwrap();
    if let CloudResult::Credits {page}=credits.result {assert_eq!(page.items[0].amount_yuan,"0.00001");} else {panic!("credits expected");}
    let order=adapter.dispatch(settings,&network(),CloudCommand::CreateOrder {purchase:Purchase::Membership {product_name:"MEMBERSHIP_3_MONTH".into(),auto_renew:true},confirmed:true}).unwrap();
    assert!(matches!(order.result,CloudResult::Order {order:PaymentOrder {status:PaymentStatus::Pending,..}}));
    let verified=adapter.dispatch(settings,&network(),CloudCommand::VerifyPayment {id:"abcdefabcdefabcdefabcdef".into()}).unwrap();
    assert!(matches!(verified.result,CloudResult::PaymentVerification {verification:PaymentVerification {settled:true,membership_advanced:true,..},..}));
    server.join().unwrap();
}

#[test]
fn invalid_network_proxy_does_not_prevent_local_logout() {
    let adapter=adapter(BASE.into());
    let network=NetworkSettings {proxy_mode:lizzie_core::settings::ProxyMode::Manual,proxy_host:String::new(),proxy_port:0,..NetworkSettings::default()};
    let state=adapter.dispatch(&CloudSettings::default(),&network,CloudCommand::State).unwrap();
    let logout=adapter.dispatch(&state.settings,&network,CloudCommand::Logout).unwrap();
    assert!(!logout.state.authenticated && logout.settings.identifier.is_empty());
}

#[test]
fn imported_credentials_are_ready_before_apply_and_rollback_restores_native_session() {
    let mut sockets=0;
    let (base,server)=fixture(move |r| {
        if r.path=="/api/cluster/account/login" {return Reply::ok(json!({"token":"native-session-token"}));}
        assert_eq!(r.path,"/api/cluster/account/fetch-socketio-token");
        sockets+=1;
        assert_eq!(r.authorization,format!("{} {}","Bearer",if sockets==1 {"imported-session-token"} else {"native-session-token"}));
        Reply::ok(json!({"token":"socket-token","socketIOURL":"wss://engine.example.invalid"}))
    },3);
    let adapter=adapter(base);
    let native=password_login(&adapter);
    let imported=CloudSettings {identifier:"imported@example.invalid".into(),..CloudSettings::default()};
    let stage=adapter.stage_import(&imported,Some("imported-session-token".into()),None).unwrap();
    assert!(!stage.settings().remember_token);
    assert_eq!(adapter.token(&imported.identifier,"").unwrap().token,"socket-token");
    assert_eq!(adapter.dispatch(&native.settings,&network(),CloudCommand::Logout).unwrap_err().key,"import_in_progress");
    assert_eq!(adapter.stage_import(&imported,None,None).unwrap_err().key,"import_in_progress");
    let settled=adapter.settle_import(stage,false);
    assert!(!settled.rollback_failed);
    let restored=settled.reply;
    assert_eq!(restored.settings.identifier,native.settings.identifier);
    assert!(restored.state.authenticated && restored.state.has_password);
    assert_eq!(adapter.token(&native.settings.identifier,"").unwrap().token,"socket-token");
    let blank=adapter.stage_import(&imported,None,None).unwrap();
    assert_eq!(adapter.token(&imported.identifier,"").err().unwrap().key,"login_required");
    let settled=adapter.settle_import(blank,true);
    assert!(!settled.rollback_failed);
    let kept=settled.reply;
    assert!(!kept.state.authenticated && !kept.state.has_password);
    assert_eq!(kept.settings.identifier,imported.identifier);
    server.join().unwrap();
}

#[test]
fn closing_one_owner_cancels_all_its_reads_without_touching_other_owners_or_engine_tokens() {
    use crate::cloud_dispatch::Requests;
    use std::sync::Barrier;
    let listener=TcpListener::bind("127.0.0.1:0").unwrap();
    listener.set_nonblocking(true).unwrap();
    let base=format!("http://{}",listener.local_addr().unwrap());
    let release=Arc::new(Barrier::new(5));
    let (arrived_tx,arrived_rx)=mpsc::channel();
    let server_release=release.clone();
    let server=thread::spawn(move || {
        let mut workers=Vec::new();
        for _ in 0..5 {
            let deadline=Instant::now()+Duration::from_secs(10);
            let mut stream=loop {
                match listener.accept() {
                    Ok((stream,_))=>break stream,
                    Err(e) if e.kind()==std::io::ErrorKind::WouldBlock && Instant::now()<deadline=>thread::sleep(Duration::from_millis(5)),
                    Err(e)=>panic!("fixture accept: {e}"),
                }
            };
            let release=server_release.clone();
            let arrived=arrived_tx.clone();
            workers.push(thread::spawn(move || {
                let request=read_request(&mut stream);
                let body=if request.path=="/api/cluster/account/login" {json!({"token":"owner-session-token"})} else {
                    arrived.send(request.path.clone()).unwrap();
                    release.wait();
                    if request.path=="/api/cluster/account/fetch-socketio-token" {json!({"token":"engine-token","socketIOURL":"wss://engine.example.invalid"})}
                    else {json!({"items":[],"total":0})}
                }.to_string();
                let response=format!("HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",body.len());
                let _=stream.write_all(response.as_bytes());
            }));
        }
        for worker in workers {worker.join().unwrap();}
    });
    let adapter=Arc::new(adapter(base));
    let login=password_login(&adapter);
    let registry=Requests::default();
    let (finished_tx,finished_rx)=mpsc::channel();
    let mut cancelled_reads=Vec::new();
    for page in [1,2] {
        let adapter=adapter.clone(); let settings=login.settings.clone();
        let cancel=registry.register(10); let finished=finished_tx.clone();
        cancelled_reads.push(thread::spawn(move || finished.send(adapter.dispatch_scoped(&settings,&network(),CloudCommand::Usages {page,page_size:5,finished:None},cancel)).unwrap()));
    }
    let other=adapter.clone(); let settings=login.settings.clone(); let cancel=registry.register(20);
    let other_read=thread::spawn(move || other.dispatch_scoped(&settings,&network(),CloudCommand::Credits {page:3,page_size:5,credit_type:None},cancel));
    let engine=adapter.clone(); let identifier=login.settings.identifier.clone();
    let engine_token=thread::spawn(move || engine.token(&identifier,VIP_ARGS));
    for _ in 0..4 {arrived_rx.recv_timeout(Duration::from_secs(5)).unwrap();}
    registry.cancel(10,&adapter);
    for _ in 0..2 {assert!(finished_rx.recv_timeout(Duration::from_secs(5)).unwrap().unwrap_err().cancelled);}
    assert!(adapter.is_current(login.state.generation));
    release.wait();
    assert!(matches!(other_read.join().unwrap().unwrap().result,CloudResult::Credits {..}));
    assert_eq!(engine_token.join().unwrap().unwrap().token,"engine-token");
    for read in cancelled_reads {read.join().unwrap();}
    registry.cancel(999,&adapter); // Closing an already idle owner is harmless.
    server.join().unwrap();
}

#[test]
fn cancelled_payment_creation_is_uncertain_and_never_commits_an_order_baseline() {
    use crate::cloud_dispatch::Requests;
    let (arrived_tx,arrived_rx)=mpsc::channel();
    let (release_tx,release_rx)=mpsc::channel();
    let order=json!({"id":"0123456789abcdef01234567","amount":1000,"paidStatus":"SUCCESS"});
    let (base,server)=fixture(move |r| match r.path.as_str() {
        "/api/cluster/account/login"=>Reply::ok(json!({"token":"payment-session-token"})),
        "/api/cluster/balance"=>Reply::ok(json!({"remainingBalance":"0","totalCashAmount":"0"})),
        "/api/pay/orders"=>{
            arrived_tx.send(()).unwrap(); release_rx.recv_timeout(Duration::from_secs(5)).unwrap(); Reply::ok(order.clone())
        },
        "/api/pay/orders/0123456789abcdef01234567"=>Reply::ok(order.clone()),
        _=>panic!("unexpected payment request"),
    },4);
    let adapter=Arc::new(adapter(base)); let login=password_login(&adapter);
    let registry=Requests::default(); let cancel=registry.register(30);
    let pending=adapter.clone(); let settings=login.settings.clone();
    let (finished_tx,finished_rx)=mpsc::channel();
    let create=thread::spawn(move || finished_tx.send(pending.dispatch_scoped(&settings,&network(),CloudCommand::CreateOrder {purchase:Purchase::TopUp {amount_fen:1000},confirmed:true},cancel)).unwrap());
    arrived_rx.recv_timeout(Duration::from_secs(5)).unwrap();
    registry.cancel(30,&adapter);
    let failure=finished_rx.recv_timeout(Duration::from_secs(5)).unwrap().unwrap_err();
    assert!(failure.cancelled && failure.uncertain);
    release_tx.send(()).unwrap(); create.join().unwrap();
    let verify=adapter.dispatch(&login.settings,&network(),CloudCommand::VerifyPayment {id:"0123456789abcdef01234567".into()}).unwrap_err();
    assert_eq!(verify.key,"payment_baseline_missing");
    assert!(adapter.is_current(login.state.generation));
    server.join().unwrap();
}

#[test]
fn transient_socket_token_failure_is_retryable_but_authentication_is_not() {
    for (status, retryable) in [(503, true), (401, false)] {
        let (base,server)=fixture(move |r| {
            if r.path.ends_with("/login") { Reply::ok(json!({"token":"fixture"})) }
            else { Reply {status,headers:"",body:json!({"key":if status==401 {"unauthorized"} else {"unknown_error"}})} }
        },2);
        let adapter=adapter(base);
        let login=password_login(&adapter);
        let error=adapter.token(&login.settings.identifier,VIP_ARGS).err().unwrap();
        assert_eq!(error.retryable,retryable,"token status {status} must preserve transport retry eligibility");
        server.join().unwrap();
    }
}

#[test]
fn socket_token_adapter_obeys_the_transport_startup_budget() {
    use lizzie_core::engine::cloud::{CloudConnectError, CloudConnector, CloudLaunch, CloudProvider, SocketCredentials};
    use lizzie_core::engine::runtime::EngineInput;
    use tokio_tungstenite::tungstenite::{accept, Message};
    for (status, recover, expected) in [(503, true, 2), (503, false, 3), (401, false, 1)] {
        let listener=TcpListener::bind("127.0.0.1:0").unwrap();
        listener.set_nonblocking(true).unwrap();
        let socket_url=format!("ws://{}",listener.local_addr().unwrap());
        let socket=thread::spawn(move || {
            if !recover { return; }
            let deadline=Instant::now()+Duration::from_secs(10);
            let stream=loop {
                match listener.accept() {
                    Ok((stream,_))=>break stream,
                    Err(e) if e.kind()==std::io::ErrorKind::WouldBlock && Instant::now()<deadline=>thread::sleep(Duration::from_millis(5)),
                    Err(e)=>panic!("socket fixture: {e}"),
                }
            };
            stream.set_read_timeout(Some(Duration::from_secs(5))).unwrap();
            let mut peer=accept(stream).unwrap();
            peer.send(Message::Text("0{\"sid\":\"fixture\",\"upgrades\":[],\"pingInterval\":25000,\"pingTimeout\":20000}".into())).unwrap();
            assert_eq!(peer.read().unwrap().into_text().unwrap(),"40");
            peer.send(Message::Text("40{\"sid\":\"fixture\"}".into())).unwrap();
            peer.send(Message::Text("42[\"ready\"]".into())).unwrap();
            let command=peer.read().unwrap().into_text().unwrap();
            assert_eq!(serde_json::from_str::<Value>(command.strip_prefix("42").unwrap()).unwrap(),json!(["stdin","1 name\n"]));
            peer.send(Message::Text("42[\"stdout\",\"=1 KataGo\\n\\n\"]".into())).unwrap();
            let _=peer.read();
        });
        let calls=Arc::new(AtomicU64::new(0));
        let observed=calls.clone();
        let (base,server)=fixture(move |r| {
            if r.path.ends_with("/login") {return Reply::ok(json!({"token":"fixture-account-token"}));}
            let n=observed.fetch_add(1,Ordering::SeqCst)+1;
            if recover && n==2 {Reply::ok(json!({"token":"fixture-socket-token","socketIOURL":socket_url}))}
            else {Reply {status,headers:"",body:json!({"key":if status==401 {"unauthorized"} else {"unknown_error"}})}}
        },expected as usize+1);
        let adapter=Arc::new(adapter(base));
        let login=password_login(&adapter);
        let tokens=adapter.clone();
        let connector=CloudConnector::new(Arc::new(move |account,args| {
            tokens.token(account,args)
                .map(|token|SocketCredentials {socket_url:token.socket_url,token:token.token})
                .map_err(|error|CloudConnectError {message:error.to_string(),fatal:!error.retryable,key:error.key})
        }));
        let endings=Arc::new(Mutex::new(Vec::new()));
        let (send,receive)=mpsc::channel();
        let mut io=connector.launch(1,&CloudLaunch {provider:CloudProvider::Zhizi,identifier:login.settings.identifier,args:VIP_ARGS.into(),url:String::new()},
            Arc::new(move |_,event| {let _=send.send(event);}),endings.clone()).unwrap();
        io.send("1 name").unwrap();
        loop {
            let event=receive.recv_timeout(Duration::from_secs(10)).unwrap();
            if recover && matches!(&event,EngineInput::Stdout(line) if line=="=1 KataGo") {break;}
            if let EngineInput::CloudStartFailed(reason)=&event {
                assert!(!recover,"transient token failure prevented startup");
                // The service's failure key reaches the failure text, not its response body.
                let key=if status==401 {"unauthorized"} else {"unknown_error"};
                assert_eq!(reason,&lizzie_core::engine::cloud::CloudStartFailure::Token {key:key.into()});
                break;
            }
        }
        io.terminate();
        for handle in std::mem::take(&mut *endings.lock().unwrap()) {handle.join().unwrap();}
        server.join().unwrap();socket.join().unwrap();
        assert_eq!(calls.load(Ordering::SeqCst),expected);
    }
}
