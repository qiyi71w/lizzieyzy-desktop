//! Account operations run on a shell worker, outside the workbench lock. Only a current
//! generation can commit credentials, cached account data, or a settings reply.
use std::collections::HashMap;
use std::path::Path;
use std::sync::{atomic::{AtomicBool, AtomicU64, Ordering}, Arc, Mutex};
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};
use lizzie_core::{cloud::*, settings::NetworkSettings};
use serde_json::{json, Value};
mod catalog;
mod data;
mod http;
mod import;
pub use import::{CloudImportSettlement, CloudImportStage};
#[cfg(test)]
mod tests;
use data::Baseline;
use http::Http;

const BASE: &str = "https://www.zhizigo.com";
#[derive(Default)]
struct Session {
    identifier: String,
    restored: bool,
    token: String,
    password: String,
    token_stored: bool,
    password_stored: bool,
    storage_warning: bool,
    overview: Option<(Instant, Overview)>,
    products: Vec<MembershipProduct>,
    orders: HashMap<String, Baseline>,
    cooldowns: HashMap<String, Instant>,
    import_pending: bool,
    settings_snapshot: CloudSettings,
}
pub struct Adapter {
    session: Mutex<Session>,
    generation: Arc<AtomicU64>,
    network: Mutex<NetworkSettings>,
    store: crate::secrets::Store,
    base: String,
    world: Option<String>,
}
impl Adapter {
    pub fn new(application: String, config_dir: &Path) -> Self {
        Self { session: Mutex::new(Session::default()), generation: Arc::new(AtomicU64::new(0)),
            network: Mutex::new(NetworkSettings::default()), store: crate::secrets::Store::new(application,config_dir),
            base: BASE.into(), world: None }
    }
    /// Refreshes engine-token networking without changing the account or retiring requests.
    pub fn set_network(&self, network: &NetworkSettings) {
        self.network.lock().unwrap().clone_from(network);
    }
    pub fn is_current(&self, generation: u64) -> bool {
        self.generation.load(Ordering::Acquire) == generation
    }
    pub fn authenticated(&self) -> bool {
        !self.session.lock().unwrap().token.is_empty()
    }
    /// Marks one window-owned request while excluding account/cache/baseline commits. Engine
    /// token fetches and other request owners retain their independent cancellation identity.
    pub fn cancel_scoped(&self, cancel: &AtomicBool) {
        let _session = self.session.lock().unwrap();
        cancel.store(true, Ordering::Release);
    }
    fn prepare(&self, session: &mut Session, settings: &CloudSettings) {
        if !same_account(&session.identifier,&settings.identifier) {
            self.generation.fetch_add(1,Ordering::AcqRel);
            *session = Session { identifier: settings.identifier.trim().into(), ..Session::default() };
        }
        session.settings_snapshot = settings.clone();
        if session.restored { return; }
        session.restored = true;
        for (kind,remember) in [("account-token",settings.remember_token),("password",settings.remember_password)] {
            if !remember { continue; }
            match self.store.read(kind,&settings.identifier) {
                Ok(Some(secret)) => if kind == "account-token" { session.token = secret; session.token_stored = true; }
                    else { session.password = secret; session.password_stored = true; },
                Ok(None) => {},
                Err(_) => session.storage_warning = true,
            }
        }
    }
    fn persist(&self, settings: &mut CloudSettings, session: &mut Session) {
        session.storage_warning = false;
        for (kind,remember,secret,stored) in [
            ("account-token", &mut settings.remember_token, &session.token, &mut session.token_stored),
            ("password", &mut settings.remember_password, &session.password, &mut session.password_stored),
        ] {
            *stored = false;
            if *remember && !secret.is_empty() {
                if self.store.write(kind,&session.identifier,secret).is_ok() { *stored = true; }
                else {
                    *remember = false;
                    session.storage_warning = true;
                    // Do not resurrect an earlier persisted credential after session-only failure.
                    if self.store.delete(kind,&session.identifier).is_err() { session.storage_warning = true; }
                }
            } else {
                *remember = false;
                if self.store.delete(kind,&session.identifier).is_err() { session.storage_warning = true; }
            }
        }
    }
    fn reply(&self, mut settings: CloudSettings, result: CloudResult, http: &Http) -> Result<CloudReply,CloudFailure> {
        let mut session = self.session.lock().unwrap();
        http.current()?;
        settings.identifier = session.identifier.clone();
        settings.remember_token &= session.token_stored;
        settings.remember_password &= session.password_stored;
        session.settings_snapshot = settings.clone();
        let mut state = self.state(&session);
        state.generation = http.expected;
        Ok(CloudReply { settings, state, result })
    }
    fn state(&self, session: &Session) -> CloudState {
        CloudState { authenticated: !session.token.is_empty(), has_password: !session.password.is_empty(),
            credential_backend: self.store.backend().into(), credential_available: self.store.available(),
            token_stored_securely: session.token_stored, password_stored_securely: session.password_stored,
            storage_warning: session.storage_warning, generation: self.generation.load(Ordering::Acquire) }
    }
    fn authenticate_error(&self, http: &Http, mut failure: CloudFailure, authenticated: bool) -> CloudFailure {
        if authenticated && (failure.status == 401 || failure.key == "unauthorized") {
            let mut session = self.session.lock().unwrap();
            if http.current().is_ok() {
                session.token.clear(); session.token_stored = false; session.overview = None; session.orders.clear();
                session.storage_warning |= self.store.delete("account-token",&session.identifier).is_err();
                self.generation.fetch_add(1,Ordering::AcqRel);
                failure.authentication_invalidated = true;
            }
        }
        failure
    }
    pub fn dispatch(&self, settings: &CloudSettings, network: &NetworkSettings, command: CloudCommand) -> Result<CloudReply,CloudFailure> {
        self.dispatch_scoped(settings,network,command,Arc::new(AtomicBool::new(false)))
    }
    pub fn dispatch_scoped(&self, settings: &CloudSettings, network: &NetworkSettings, command: CloudCommand, cancel: Arc<AtomicBool>) -> Result<CloudReply,CloudFailure> {
        self.set_network(network);
        // Login selects its explicit account before restoring any remembered password.
        let login_identifier = match &command {
            CloudCommand::PasswordLogin { identifier,.. } | CloudCommand::CodeLogin { identifier,.. }
                | CloudCommand::ResetPassword { identifier,.. } => Some(identifier.trim().to_owned()), _ => None,
        };
        let mut settings = settings.clone();
        let (expected, token) = {
            let mut session = self.session.lock().unwrap();
            if cancel.load(Ordering::Acquire) { return Err(CloudFailure::cancelled()); }
            if session.import_pending { return Err(CloudFailure::new("import_in_progress")); }
            if let Some(identifier) = login_identifier {
                if identifier.is_empty() { return Err(CloudFailure::new("invalid_account")); }
                if !same_account(&settings.identifier,&identifier) {
                    self.generation.fetch_add(1,Ordering::AcqRel);
                    let warning = self.store.delete("account-token",&settings.identifier).is_err()
                        | self.store.delete("password",&settings.identifier).is_err();
                    *session = Session { identifier: identifier.clone(), storage_warning: warning, ..Session::default() };
                    settings.remember_token = false; settings.remember_password = false;
                }
                settings.identifier = identifier;
            }
            self.prepare(&mut session,&settings);
            let expected = if matches!(&command,CloudCommand::PasswordLogin {..} | CloudCommand::CodeLogin {..} | CloudCommand::ResetPassword {..} | CloudCommand::Logout) {
                self.generation.fetch_add(1,Ordering::AcqRel) + 1
            } else { self.generation.load(Ordering::Acquire) };
            (expected, session.token.clone())
        };
        let direct_network;
        let request_network = if matches!(&command,CloudCommand::State | CloudCommand::Logout | CloudCommand::Remember {..}) {
            direct_network = NetworkSettings { proxy_mode: lizzie_core::settings::ProxyMode::Direct, ..NetworkSettings::default() };
            &direct_network
        } else { network };
        let http = Http::new(request_network,self.generation.clone(),expected,cancel)?;
        let authenticated = !matches!(&command, CloudCommand::PasswordLogin {..} | CloudCommand::CodeLogin {..} | CloudCommand::ResetPassword {..} | CloudCommand::SendCode {..} | CloudCommand::Products | CloudCommand::State | CloudCommand::Logout | CloudCommand::Remember {..});
        let result = self.execute(&mut settings,command,&http,&token);
        match result {
            Ok(result) => self.reply(settings,result,&http),
            Err(failure) => { http.current()?; Err(self.authenticate_error(&http,failure,authenticated)) },
        }
    }
    fn execute(&self, settings: &mut CloudSettings, command: CloudCommand, http: &Http, token: &str) -> Result<CloudResult,CloudFailure> {
        let get = |path: &str| { require_token(token)?; http.get(&format!("{}{path}",self.base),token) };
        match command {
            CloudCommand::State => Ok(CloudResult::State),
            CloudCommand::PasswordLogin {identifier,password,remember_token,remember_password} => {
                let password = if password.is_empty() { self.session.lock().unwrap().password.clone() } else { password.trim().into() };
                if password.is_empty() { return Err(CloudFailure::new("password_required")); }
                self.login(settings,http,&identifier,&password,None,false,remember_token,remember_password)?;
                Ok(CloudResult::LoggedIn)
            }
            CloudCommand::CodeLogin {identifier,code,remember_token} => {
                let remember_password = settings.remember_password;
                self.login(settings,http,&identifier,"",Some(&code),false,remember_token,remember_password)?;
                Ok(CloudResult::LoggedIn)
            }
            CloudCommand::ResetPassword {identifier,code,password,remember_token,remember_password} => {
                if password.encode_utf16().count() < 8 { return Err(CloudFailure::new("password_too_short")); }
                self.login(settings,http,&identifier,&password,Some(&code),true,remember_token,remember_password)?;
                Ok(CloudResult::PasswordReset)
            }
            CloudCommand::SendCode {identifier,purpose} => {
                if identifier.trim().is_empty() { return Err(CloudFailure::new("invalid_account")); }
                let cooldown_key = format!("{}:{purpose:?}",identifier.trim().to_lowercase());
                if let Some(until) = self.session.lock().unwrap().cooldowns.get(&cooldown_key) {
                    if *until > Instant::now() { return Err(CloudFailure { retry_after_seconds: until.duration_since(Instant::now()).as_secs().saturating_add(1).min(600) as u32, ..CloudFailure::new("fast_login_too_frequent") }); }
                }
                let mut body = identifier_body(&identifier);
                body["type"] = json!(match purpose { CodePurpose::FastLogin => "fast_login", CodePurpose::ResetPassword => "reset_password" });
                let response = http.post(&format!("{}/api/cluster/account/send-code",self.base),"",body,true);
                let cooldown = match &response { Ok(r) => if r.retry_after == 0 { 60 } else { r.retry_after }, Err(e) if e.retry_after_seconds > 0 => e.retry_after_seconds,
                    Err(e) if e.key == "fast_login_too_frequent" => 60, _ => 0 }.min(600);
                if cooldown > 0 {
                    let mut session = self.session.lock().unwrap(); http.current()?;
                    session.cooldowns.insert(cooldown_key,Instant::now()+Duration::from_secs(cooldown as u64));
                }
                response?;
                Ok(CloudResult::CodeSent { retry_after_seconds: cooldown })
            }
            CloudCommand::Remember {token,password} => {
                let mut session = self.session.lock().unwrap(); http.current()?;
                settings.remember_token = token; settings.remember_password = password;
                self.persist(settings,&mut session);
                Ok(CloudResult::State)
            }
            CloudCommand::Logout => {
                let mut session = self.session.lock().unwrap(); http.current()?;
                session.token.clear(); session.password.clear(); session.overview = None; session.orders.clear(); session.products.clear();
                settings.remember_token = false; settings.remember_password = false;
                self.persist(settings,&mut session);
                session.identifier.clear(); settings.identifier.clear();
                Ok(CloudResult::LoggedOut)
            }
            CloudCommand::Overview {force} => {
                require_token(token)?;
                {
                    let session = self.session.lock().unwrap();
                    if !force { if let Some((when,overview)) = &session.overview {
                        if when.elapsed() < Duration::from_secs(45) { return Ok(CloudResult::Overview { overview: overview.clone() }); }
                    }}
                }
                let overview = Overview { loaded_at: now_millis(), account: data::account(&get("/api/cluster/account/me")?)?,
                    balance: data::balance(&get("/api/cluster/balance")?)?, recent_usage: data::usages(&get("/api/cluster/usage/my-usages?page=0&pageSize=5")?,0,5)? };
                let mut session = self.session.lock().unwrap(); http.current()?;
                session.overview = Some((Instant::now(),overview.clone()));
                Ok(CloudResult::Overview {overview})
            }
            CloudCommand::Usages {page,page_size,finished} => {
                let size = page_size.clamp(1,100);
                let mut path = format!("/api/cluster/usage/my-usages?page={page}&pageSize={size}");
                if let Some(finished) = finished { path.push_str(&format!("&finished={finished}")); }
                Ok(CloudResult::Usages { page: data::usages(&get(&path)?,page,size)? })
            }
            CloudCommand::Credits {page,page_size,credit_type} => {
                let size = page_size.clamp(1,100);
                let mut url = reqwest::Url::parse(&format!("{}/api/cluster/credit/my-credits?page={page}&pageSize={size}",self.base)).map_err(|_| CloudFailure::new("invalid_endpoint"))?;
                if let Some(credit_type) = credit_type.filter(|s| !s.trim().is_empty()) { url.query_pairs_mut().append_pair("creditType",credit_type.trim()); }
                require_token(token)?;
                Ok(CloudResult::Credits { page: data::credits(&http.get(url.as_str(),token)?,page,size)? })
            }
            CloudCommand::RefreshCatalog { args } => {
                require_token(token)?;
                let catalog = catalog::refresh(http,&self.base,token,self.world.as_deref())?;
                settings.args = catalog.resolve_args(&args);
                settings.catalog = catalog.clone(); settings.catalog_updated_at = now_millis();
                Ok(CloudResult::Catalog {catalog})
            }
            CloudCommand::Products => {
                let products = data::products(&http.get(&format!("{}/api/cluster/product?type=MEMBERSHIP",self.base),"")?)?;
                let mut session = self.session.lock().unwrap(); http.current()?; session.products = products.clone();
                Ok(CloudResult::Products { products, top_up_presets_fen: TOP_UP_PRESETS_FEN.to_vec() })
            }
            CloudCommand::CreateOrder {purchase,confirmed} => {
                if !confirmed { return Err(CloudFailure::new("confirmation_required")); }
                require_token(token)?;
                let (body,purpose,amount,name) = match purchase {
                    Purchase::TopUp {amount_fen} => {
                        if !TOP_UP_PRESETS_FEN.contains(&amount_fen) { return Err(CloudFailure::new("invalid_top_up_amount")); }
                        (json!({"payType":"WECHAT","amount":amount_fen,"tradeType":"NATIVE","body":"LizzieYzy Next account top-up"}),PaymentPurpose::BalanceTopUp,amount_fen as u64,String::new())
                    }
                    Purchase::Membership {product_name,auto_renew} => {
                        let product = self.session.lock().unwrap().products.iter().find(|p| p.name == product_name).cloned().ok_or_else(|| CloudFailure::new("invalid_product"))?;
                        (json!({"payType":"WECHAT","amount":product.price_fen,"tradeType":"NATIVE","body":"LizzieYzy Next VIP membership","orderType":"PURCHASE_PRODUCT","productName":product.name,"extraInfo":{"autoRenew":auto_renew}}),PaymentPurpose::VipMembership,product.price_fen,product.name)
                    }
                };
                let baseline = Baseline {purpose,observed_at:chrono::Utc::now().timestamp(),
                    balance: if purpose == PaymentPurpose::BalanceTopUp {Some(data::balance(&get("/api/cluster/balance")?)?)} else {None},
                    account: if purpose == PaymentPurpose::VipMembership {Some(data::account(&get("/api/cluster/account/me")?)?)} else {None}};
                let order = data::order(&http.post(&format!("{}/api/pay/orders",self.base),token,body,false)?.body)?;
                if order.amount_fen != amount || order.purpose != purpose || order.product_name != name { return Err(CloudFailure::new("invalid_response")); }
                let mut session = self.session.lock().unwrap(); http.current()?; session.orders.insert(order.id.clone(),baseline);
                Ok(CloudResult::Order {order})
            }
            CloudCommand::ReadOrder {id} => Ok(CloudResult::Order {order: self.read_order(http,token,&id)?}),
            CloudCommand::VerifyPayment {id} => {
                let order = self.read_order(http,token,&id)?;
                let baseline = self.session.lock().unwrap().orders.get(&order.id).cloned().ok_or_else(|| CloudFailure::new("payment_baseline_missing"))?;
                let balance = if order.purpose == PaymentPurpose::BalanceTopUp {Some(data::balance(&get("/api/cluster/balance")?)?)} else {None};
                let credits = if order.purpose == PaymentPurpose::BalanceTopUp {Some(data::credits(&get("/api/cluster/credit/my-credits?page=0&pageSize=20&creditType=CASH")?,0,20)?)} else {None};
                let account = if order.purpose == PaymentPurpose::VipMembership {Some(data::account(&get("/api/cluster/account/me")?)?)} else {None};
                let verification = data::verify(&baseline,&order,balance.as_ref(),credits.as_ref(),account.as_ref());
                if verification.settled { let mut session = self.session.lock().unwrap(); http.current()?; session.overview = None; }
                Ok(CloudResult::PaymentVerification {order,verification})
            }
        }
    }
    #[allow(clippy::too_many_arguments)]
    fn login(&self, settings: &mut CloudSettings, http: &Http, identifier: &str, password: &str, code: Option<&str>, reset: bool, remember_token: bool, remember_password: bool) -> Result<(),CloudFailure> {
        let mut body = identifier_body(identifier);
        if code.is_some_and(|c| c.trim().is_empty()) { return Err(CloudFailure::new("verification_code_required")); }
        if let Some(code) = code { body["verificationCode"] = json!(code.trim()); }
        if code.is_none() || reset { body["password"] = json!(password); }
        let path = if reset {"reset-password"} else if code.is_some() {"fast-login"} else {"login"};
        let response = http.post(&format!("{}/api/cluster/account/{path}",self.base),"",body,false).map_err(|mut failure| {
            if failure.status == 401 && failure.key == "unauthorized" { failure.key = if code.is_some() {"invalid_verification_code"} else {"invalid_credentials"}.into(); }
            failure
        })?;
        let token = data::string(&response.body,"token");
        if token.is_empty() { return Err(CloudFailure::new("invalid_response")); }
        let mut session = self.session.lock().unwrap(); http.current()?;
        if !same_account(&session.identifier,identifier) { return Err(CloudFailure::cancelled()); }
        session.identifier = identifier.trim().into(); session.token = token; session.overview = None; session.orders.clear();
        if code.is_none() || reset { session.password = password.into(); }
        settings.identifier = identifier.trim().into(); settings.remember_token = remember_token; settings.remember_password = remember_password;
        self.persist(settings,&mut session);
        Ok(())
    }
    fn read_order(&self, http: &Http, token: &str, id: &str) -> Result<PaymentOrder,CloudFailure> {
        let id = id.trim();
        if !data::valid_order_id(id) { return Err(CloudFailure::new("invalid_order_id")); }
        require_token(token)?;
        let order = data::order(&http.get(&format!("{}/api/pay/orders/{id}",self.base),token)?)?;
        if !id.eq_ignore_ascii_case(&order.id) { return Err(CloudFailure::new("invalid_response")); }
        Ok(order)
    }
    /// Each transport startup invokes this anew. Account login is independent from permission
    /// to start a billed engine; the caller must obtain that explicit authorization separately.
    pub fn token(&self, account: &str, args: &str) -> Result<CloudSocketToken,CloudFailure> {
        let (token,expected) = {
            let session = self.session.lock().unwrap();
            if !same_account(account,&session.identifier) { return Err(CloudFailure::new("login_required")); }
            require_token(&session.token)?;
            (session.token.clone(),self.generation.load(Ordering::Acquire))
        };
        let network = self.network.lock().unwrap().clone();
        let http = Http::new(&network,self.generation.clone(),expected,Arc::new(AtomicBool::new(false)))?;
        let response = http.post(&format!("{}/api/cluster/account/fetch-socketio-token",self.base),&token,json!({"args":if args.trim().is_empty() {VIP_ARGS} else {args}}),false)
            .map_err(|mut e| {
                // FETCH_SOCKET_TOKEN is idempotent in Java. The transport, not HTTP, owns the
                // three-attempt startup budget; payment and other POSTs retain their policy.
                e.retryable = !e.cancelled && (e.key == "network_error"
                    || matches!(e.status, 408 | 425 | 429) || e.status >= 500);
                e.uncertain = false;
                self.authenticate_error(&http,e,true)
            })?;
        http.current()?;
        let token = data::string(&response.body,"token");
        let socket_url = data::string(&response.body,"socketIOURL");
        if token.is_empty() || socket_url.is_empty() { return Err(CloudFailure::new("invalid_response")); }
        Ok(CloudSocketToken {token,socket_url})
    }
}
fn same_account(a: &str, b: &str) -> bool { a.trim().eq_ignore_ascii_case(b.trim()) }
fn require_token(token: &str) -> Result<(),CloudFailure> { if token.is_empty() {Err(CloudFailure::new("login_required"))} else {Ok(())} }
fn identifier_body(identifier: &str) -> Value { let identifier = identifier.trim(); if identifier.contains('@') {json!({"email":identifier})} else {json!({"phone":identifier})} }
fn now_millis() -> u64 { SystemTime::now().duration_since(UNIX_EPOCH).unwrap_or_default().as_millis().min(u64::MAX as u128) as u64 }
