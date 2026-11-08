//! 弈客直播中心 (`YikeLiveDialog`) and the 弈客 web window (`BrowserFrame` with `isYike`): one
//! native window holding the local toolbar webview and the remote 弈客 page, the page's bridge
//! plugin and the scripts injected into it.
//!
//! Bridge: the remote page can call only `plugin:yike-page|report` (capabilities/yike-page.json);
//! [`report`] admits it only from the `yike-page` webview at a 弈客 address and passes bounded,
//! typed [`PageReport`]s to the workbench.

use std::fmt::Write as _;
use std::sync::Mutex;

use lizzie_core::api::{ApiError, ErrorKind, OnlineJob, PageReport};
use lizzie_core::settings::WindowPlacement;
use serde::Deserialize;
use tauri::plugin::TauriPlugin;
use tauri::webview::{NewWindowResponse, PageLoadEvent, WebviewBuilder};
use tauri::window::WindowBuilder;
use tauri::{
    AppHandle, Emitter, EventTarget, LogicalPosition, LogicalSize, Manager, PhysicalPosition, PhysicalSize, Url,
    Webview, WebviewUrl, WebviewWindowBuilder, Window, WindowEvent, Wry,
};
use tauri_plugin_opener::OpenerExt;

use crate::{publish, Shared, State};

/// 弈客直播中心.
pub const LIVE: &str = "yike-live";
/// The 弈客 web window.
pub const BROWSER: &str = "yike-browser";
/// Its toolbar (后退, address, 加载, 停止同步; BF 392-435).
const BAR: &str = "yike-bar";
/// Its remote 弈客 page.
const PAGE: &str = "yike-page";
/// Platform → `yike-bar`: `{ url }` whenever the page address changes.
const ADDRESS_EVENT: &str = "yike-browser-address";
const BAR_HEIGHT: f64 = 34.0;
/// Java `body.substring(0,500000)` (BF 1123).
const MAX_BODY_CHARS: usize = 500_000;
/// Addresses, tags and messages.
const MAX_TEXT: usize = 4096;

/// The page's last reported address and the 对局室 poller to (re)install (BF
/// `pendingYikeRoomId` / `pendingYikeIntervalMs`).
#[derive(Default)]
struct PageState {
    address: Mutex<String>,
    poller: Mutex<Option<(u64, u64)>>,
}

/// The inline `yike-page` plugin (build.rs `InlinedPlugin`) with its one command.
pub fn plugin() -> TauriPlugin<Wry> {
    tauri::plugin::Builder::new("yike-page")
        .invoke_handler(tauri::generate_handler![report])
        .setup(|app, _| {
            app.manage(PageState::default());
            Ok(())
        })
        .build()
}

/// `https://yikeweiqi.com` or a subdomain, default port.
fn is_yike(url: &Url) -> bool {
    url.scheme() == "https"
        && url.port().is_none()
        && url.host_str().is_some_and(|h| h == "yikeweiqi.com" || h.ends_with(".yikeweiqi.com"))
}

fn page_url(text: &str) -> Option<Url> {
    Url::parse(text.trim()).ok().filter(is_yike)
}

/// What the page may report (`{ kind: "address" | "unite", … }`); anything else fails to
/// deserialize and is dropped.
#[derive(Deserialize)]
#[serde(tag = "kind", rename_all = "lowercase")]
enum Report {
    Address {
        url: String,
    },
    Unite {
        room: u64,
        tag: String,
        #[serde(default)]
        status: u16,
        #[serde(default)]
        body: String,
        #[serde(default)]
        message: String,
    },
}

/// The page bridge (Java `cefQuery` `yikeUnite:` in BF 157-171, plus address reports).
#[tauri::command]
async fn report(app: AppHandle, webview: Webview, report: Report) {
    if webview.label() != PAGE || !webview.url().is_ok_and(|url| is_yike(&url)) {
        return;
    }
    match report {
        Report::Address { url } => {
            if url.len() <= MAX_TEXT {
                if let Some(url) = page_url(&url) {
                    address_changed(&app, url.as_str());
                }
            }
        }
        Report::Unite { room, tag, status, body, message } => {
            // OnlineDialog.handleYikeUniteSgf knows `ping`, `error` and `resp` (OD 4153-4166).
            if !matches!(tag.as_str(), "ping" | "error" | "resp")
                || message.len() > MAX_TEXT
                || body.len() > 4 * MAX_BODY_CHARS
                || body.chars().count() > MAX_BODY_CHARS
            {
                return;
            }
            page_report(&app, PageReport::Unite { room, tag, status, body, message });
        }
    }
}

fn page_report(app: &AppHandle, report: PageReport) {
    let shared = app.state::<State>().inner().clone();
    let changed = shared.workbench.lock().unwrap().online_page(report);
    if changed {
        publish(app, &shared);
    }
}

/// BF `onAddressChange` (311-337): the toolbar's address field and the follow flow hear each
/// new page address.
fn address_changed(app: &AppHandle, url: &str) {
    {
        let state = app.state::<PageState>();
        let mut last = state.address.lock().unwrap();
        if *last == url {
            return;
        }
        url.clone_into(&mut *last);
    }
    let _ = app.emit_to(EventTarget::webview(BAR), ADDRESS_EVENT, serde_json::json!({ "url": url }));
    page_report(app, PageReport::Address { url: url.to_owned() });
}

/// The next page load reports its address even when it equals the last one (the toolbar's
/// field may have been edited).
fn forget_address(app: &AppHandle) {
    app.state::<PageState>().address.lock().unwrap().clear();
}

/// The follow flow's page work.
pub fn page_job(app: &AppHandle, shared: &Shared, job: OnlineJob) {
    let state = app.state::<PageState>();
    let page = app.get_webview(PAGE);
    match job {
        // BF installYikeUnitePoller (1087-1100): remembered for later page loads.
        OnlineJob::UnitePoller { room, interval_ms, .. } => {
            *state.poller.lock().unwrap() = Some((room, interval_ms));
            match page {
                Some(page) => {
                    let _ = page.eval(start_poller(room, interval_ms));
                }
                None => {
                    let unavailable = PageReport::Unite {
                        room,
                        tag: "unavailable".into(),
                        status: 0,
                        body: String::new(),
                        message: String::new(),
                    };
                    if shared.workbench.lock().unwrap().online_page(unavailable) {
                        publish(app, shared);
                    }
                }
            }
        }
        // BF stopYikeUnitePoller (1201-1212).
        OnlineJob::StopUnitePoller => {
            *state.poller.lock().unwrap() = None;
            if let Some(page) = page {
                let _ = page.eval(STOP_POLLER);
            }
        }
        // BF 327-331: `browser_.loadURL(url)` after an automatic room switch.
        OnlineJob::ReloadPage { url } => {
            if let (Some(page), Some(url)) = (page, page_url(&url)) {
                let _ = if page.url().is_ok_and(|current| current == url) { page.reload() } else { page.navigate(url) };
            }
        }
        OnlineJob::Get { .. } | OnlineJob::Send { .. } | OnlineJob::Connect { .. } | OnlineJob::Close { .. } => {}
    }
}

fn start_poller(room: u64, interval_ms: u64) -> String {
    // `Math.max(500, intervalMs)` (BF 1100).
    let period = interval_ms.max(500);
    format!("window.__lizzieYikeUnite&&window.__lizzieYikeUnite.start('{room}',{period});")
}

const STOP_POLLER: &str = "window.__lizzieYikeUnite&&window.__lizzieYikeUnite.stop();";

/// Main-frame document-start script of every 弈客 page document.
///
/// (1) Address reporter: load, `hashchange`, `popstate`, `history.pushState/replaceState`.
/// (2) The 对局室 poller of BF `installYikeUnitePoller` (1087-1199): hooks fetch,
/// XMLHttpRequest and WebSocket, captures the page's own
/// `game-server.yikeweiqi.com/game/info?id=<room>` request (with its login state) and replays
/// it every `max(500, interval)` ms, plus 80 ms after any 弈客 WebSocket message. Java injected
/// the hooks when following started and on each load; here they are in place from document
/// start and stay silent until `start` names the room, so the page's first request is never
/// missed. A request seen before `start` is adopted and replayed at once.
const PAGE_SCRIPT: &str = r#"(function(){
if(location.protocol!=='https:'||!/(^|\.)yikeweiqi\.com$/i.test(location.hostname))return;
if(window.__lizzieYikeUnite)return;
var invoke=function(report){try{var p=window.__TAURI_INTERNALS__.invoke('plugin:yike-page|report',{report:report});if(p&&p.catch)p.catch(function(){});}catch(e){}};
var lastHref='';
var reportAddress=function(){var h=location.href;if(h===lastHref)return;lastHref=h;invoke({kind:'address',url:h});};
window.addEventListener('hashchange',reportAddress,{passive:true});
window.addEventListener('popstate',reportAddress,{passive:true});
try{var origPush=history.pushState;history.pushState=function(){var r=origPush.apply(this,arguments);setTimeout(reportAddress,0);return r;};}catch(e){}
try{var origReplace=history.replaceState;history.replaceState=function(){var r=origReplace.apply(this,arguments);setTimeout(reportAddress,0);return r;};}catch(e){}
if(document.readyState==='loading'){document.addEventListener('DOMContentLoaded',reportAddress);}else{reportAddress();}
var ROOM='',PERIOD=1000,poller=null,debounce=null,saved=null,early=null;
var send=function(tag,body,room){var r=room===undefined?ROOM:room;if(ROOM===''||r!==ROOM)return;
if(tag==='resp'){invoke({kind:'unite',room:Number(r),tag:'resp',status:body.status,body:body.body,message:''});}
else{invoke({kind:'unite',room:Number(r),tag:tag,status:0,body:'',message:String(body).substring(0,2000)});}};
var matchHttp=function(u){return typeof u==='string'&&u.indexOf('game-server.yikeweiqi.com/game/info')>=0;};
var matchRoom=function(u){return ROOM!==''&&matchHttp(u)&&new RegExp('[?&]id='+ROOM+'(&|#|$)').test(u);};
var deliverResp=function(room,status,text){send('resp',{status:status,body:text.substring(0,500000)},room);};
var origFetch=window.fetch;
var XO=window.XMLHttpRequest;
var origXOpen=XO&&XO.prototype.open,origXSend=XO&&XO.prototype.send,origXSetHdr=XO&&XO.prototype.setRequestHeader;
var poll=function(){
if(!saved)return;
if(!matchRoom(saved.url)){saved=null;send('ping','cleared stale request');return;}
var s=saved,room=ROOM;
if(s.kind==='fetch'){try{
origFetch(s.req.clone()).then(function(r){return r.clone().text().then(function(t){deliverResp(room,r.status,t);});}).catch(function(e){send('error','poll fetch: '+e,room);});
}catch(e){send('error','poll fetch throw: '+e);}}
else if(s.kind==='xhr'){try{
var x=new XO();
origXOpen.call(x,s.method||'GET',s.url,true);
if(s.headers){for(var k in s.headers){try{origXSetHdr.call(x,k,s.headers[k]);}catch(e){}}}
x.addEventListener('load',function(){deliverResp(room,x.status,x.responseText||'');});
x.addEventListener('error',function(){send('error','poll xhr: network',room);});
origXSend.call(x,s.body||null);
}catch(e){send('error','poll xhr throw: '+e);}}
};
var startPolling=function(){
if(poller)return;
send('ping','start polling kind='+(saved?saved.kind:'waiting')+' period='+PERIOD);
poller=setInterval(poll,PERIOD);
};
var captured=function(entry,status,text){
if(matchRoom(entry.url)){deliverResp(ROOM,status,text);if(!saved){saved=entry;startPolling();}}
else if(ROOM===''&&matchHttp(entry.url)){early=entry;}
};
if(origFetch){window.fetch=function(input,init){
var url=typeof input==='string'?input:(input&&input.url?input.url:String(input||''));
var entry=null;
if(matchHttp(url)){try{var req=null;
if(input&&typeof input!=='string'&&typeof input.clone==='function'){req=input.clone();}
else if(typeof Request!=='undefined'){req=new Request(url,init||{});}
if(req)entry={kind:'fetch',url:url,req:req};
}catch(e){send('error','save fetch: '+e);}}
var p=origFetch.apply(this,arguments);
if(entry){p.then(function(r){r.clone().text().then(function(t){captured(entry,r.status,t);}).catch(function(){});}).catch(function(){});}
return p;};}
if(XO&&XO.prototype){var XP=XO.prototype;
XP.open=function(m,u){this.__yikeUrl=String(u);this.__yikeMethod=m;this.__yikeHdrs={};return origXOpen.apply(this,arguments);};
XP.setRequestHeader=function(k,v){try{this.__yikeHdrs=this.__yikeHdrs||{};this.__yikeHdrs[k]=v;}catch(e){}return origXSetHdr.apply(this,arguments);};
XP.send=function(body){var x=this;x.__yikeBody=body;
if(matchHttp(x.__yikeUrl)){x.addEventListener('load',function(){try{
captured({kind:'xhr',url:x.__yikeUrl,method:x.__yikeMethod||'GET',headers:x.__yikeHdrs||{},body:x.__yikeBody},x.status,x.responseText||'');
}catch(e){}});}
return origXSend.apply(this,arguments);};}
var WSO=window.WebSocket;
if(WSO){var WSP=function(url,protocols){
var ws=protocols?new WSO(url,protocols):new WSO(url);
if(String(url).indexOf('yikeweiqi.com')>=0){
send('ping','ws-open');
ws.addEventListener('message',function(){
if(saved){if(debounce)clearTimeout(debounce);debounce=setTimeout(function(){debounce=null;poll();},80);}
});}
return ws;};
WSP.prototype=WSO.prototype;WSP.CONNECTING=WSO.CONNECTING;WSP.OPEN=WSO.OPEN;WSP.CLOSING=WSO.CLOSING;WSP.CLOSED=WSO.CLOSED;
window.WebSocket=WSP;}
window.__lizzieYikeUnite={
start:function(room,period){
if(poller){clearInterval(poller);poller=null;}
ROOM=String(room);PERIOD=period;
if(saved&&!matchRoom(saved.url)){send('ping','cleared stale request');saved=null;}
send('ping','hook installed for room='+ROOM);
if(!saved&&early&&matchRoom(early.url)){saved=early;}
early=null;
startPolling();poll();
},
stop:function(){
if(poller){clearInterval(poller);poller=null;}
if(debounce){clearTimeout(debounce);debounce=null;}
saved=null;early=null;ROOM='';
}};
})();"#;

/// Opens 弈客直播中心 or brings it forward (`showAndActivate`, YLD 199-204): 920 × 420
/// centred on the main window (`setLocationRelativeTo`), fitted into the work area minus 20
/// (`fitOnScreen`, YLD 324-334), on top when the main window is (YLD 69). Async: building a
/// window in a synchronous command deadlocks on Windows.
#[tauri::command]
pub async fn open_yike_live(app: AppHandle, title: String) -> Result<(), ApiError> {
    if let Some(window) = app.get_webview_window(LIVE) {
        let _ = window.set_title(&title);
        let _ = window.unminimize();
        let _ = window.show();
        let _ = window.set_focus();
        return Ok(());
    }
    let (mut width, mut height) = (920.0_f64, 420.0_f64);
    let main = app.get_webview_window("main");
    let monitor = main
        .as_ref()
        .and_then(|window| window.current_monitor().ok().flatten())
        .or_else(|| app.primary_monitor().ok().flatten());
    let bounds = monitor.map(|monitor| {
        let scale = monitor.scale_factor();
        let work = monitor.work_area();
        let (origin, size) = (work.position.to_logical::<f64>(scale), work.size.to_logical::<f64>(scale));
        (origin.x, origin.y, size.width, size.height)
    });
    if let Some((_, _, bw, bh)) = bounds {
        width = width.min((bw - 20.0).max(1.0));
        height = height.min((bh - 20.0).max(1.0));
    }
    let mut builder = WebviewWindowBuilder::new(&app, LIVE, WebviewUrl::App("yikelive.html".into()))
        .title(title)
        .inner_size(width, height)
        .disable_drag_drop_handler();
    let centre = main.as_ref().and_then(|main| {
        let scale = main.scale_factor().ok()?;
        let (pos, size) = (main.outer_position().ok()?.to_logical::<f64>(scale), main.outer_size().ok()?.to_logical::<f64>(scale));
        Some((pos.x, pos.y, size.width, size.height))
    });
    if let Some((x, y, w, h)) = centre.or(bounds) {
        let (mut x, mut y) = (x + (w - width) / 2.0, y + (h - height) / 2.0);
        if let Some((bx, by, bw, bh)) = bounds {
            x = x.clamp(bx, (bx + bw - width).max(bx));
            y = y.clamp(by, (by + bh - height).max(by));
        }
        builder = builder.position(x, y);
    }
    let on_top = main.as_ref().is_some_and(|main| main.is_always_on_top().unwrap_or(false));
    builder
        .always_on_top(on_top)
        .build()
        .map_err(|e| ApiError::new(ErrorKind::Unavailable, format!("opening the 弈客 live center failed: {e}")))?;
    Ok(())
}

/// Java `LizzieFrame.bowser` / `BrowserFrame.openURL` (BF 653-672): reuses or creates the 弈客
/// web window, loads `url`, sets the base title, re-applies the saved placement
/// (`setFrameSize`) and shows it.
#[tauri::command]
pub async fn open_yike_browser(app: AppHandle, url: String, title: String) -> Result<(), ApiError> {
    let url = page_url(&url).ok_or_else(not_yike)?;
    let saved = app.state::<State>().workbench.lock().unwrap().persisted().yike_browser;
    forget_address(&app);
    if let Some(window) = app.get_window(BROWSER) {
        if let Some(page) = app.get_webview(PAGE) {
            page.navigate(url).map_err(failed)?;
        }
        let _ = window.set_title(&title);
        place(&window, saved);
        let _ = window.unminimize();
        let _ = window.show();
        let _ = window.set_focus();
        return Ok(());
    }
    let window = WindowBuilder::new(&app, BROWSER)
        .title(title)
        .inner_size(DEFAULT_SIZE.0, DEFAULT_SIZE.1)
        .position(DEFAULT_POSITION.0, DEFAULT_POSITION.1)
        .visible(false)
        .build()
        .map_err(failed)?;
    place(&window, saved);
    let scale = window.scale_factor().map_err(failed)?;
    let size = window.inner_size().map_err(failed)?.to_logical::<f64>(scale);
    let bar = WebviewBuilder::new(BAR, WebviewUrl::App(format!("yikebrowser.html?url={}", encode_component(url.as_str())).into()))
        .disable_drag_drop_handler();
    window
        .add_child(bar, LogicalPosition::new(0.0, 0.0), LogicalSize::new(size.width, BAR_HEIGHT))
        .map_err(failed)?;
    window
        .add_child(
            page_builder(&app, url),
            LogicalPosition::new(0.0, BAR_HEIGHT),
            LogicalSize::new(size.width, (size.height - BAR_HEIGHT).max(0.0)),
        )
        .map_err(failed)?;
    #[cfg(target_os = "linux")]
    pack_bar(&app);
    let laid_out = window.clone();
    window.on_window_event(move |event| {
        if matches!(event, WindowEvent::Resized(_) | WindowEvent::ScaleFactorChanged { .. }) {
            layout(&laid_out);
        }
    });
    window.show().map_err(failed)?;
    let _ = window.set_focus();
    Ok(())
}

/// Java `bowserX/Y/Width/Height` defaults (LizzieFrame.java:496-499).
const DEFAULT_POSITION: (f64, f64) = (-5.0, 0.0);
const DEFAULT_SIZE: (f64, f64) = (1240.0, 750.0);

/// BF `setFrameSize` (645-651): the placement saved when the window last closed, or Java's
/// default.
fn place(window: &Window, saved: Option<WindowPlacement>) {
    match saved {
        Some(p) => {
            let _ = window.set_size(PhysicalSize::new(p.width, p.height));
            let _ = window.set_position(PhysicalPosition::new(p.x, p.y));
        }
        None => {
            let _ = window.set_size(LogicalSize::new(DEFAULT_SIZE.0, DEFAULT_SIZE.1));
            let _ = window.set_position(LogicalPosition::new(DEFAULT_POSITION.0, DEFAULT_POSITION.1));
        }
    }
}

/// Linux: tauri-runtime-wry puts child webviews into the window's `GtkBox`, each expanding, and
/// their bounds apply only inside a `GtkFixed` (wry `add_to_container` / `set_bounds`); the bar
/// keeps its height and the page takes the rest, which `layout` cannot do there.
#[cfg(target_os = "linux")]
fn pack_bar(app: &AppHandle) {
    let Some(bar) = app.get_webview(BAR) else { return };
    let _ = bar.with_webview(|platform| {
        use gtk::prelude::*;
        let view = platform.inner();
        view.set_size_request(-1, BAR_HEIGHT as i32);
        if let Some(parent) = view.parent().and_then(|p| p.downcast::<gtk::Box>().ok()) {
            parent.set_child_packing(&view, false, false, 0, gtk::PackType::Start);
        }
    });
}

/// The toolbar strip across the top, the page below it (BF 450-453, `BorderLayout`).
fn layout(window: &Window) {
    let (Ok(size), Ok(scale)) = (window.inner_size(), window.scale_factor()) else {
        return;
    };
    let size = size.to_logical::<f64>(scale);
    for webview in window.webviews() {
        let (y, height) = if webview.label() == BAR {
            (0.0, BAR_HEIGHT)
        } else {
            (BAR_HEIGHT, (size.height - BAR_HEIGHT).max(0.0))
        };
        let _ = webview.set_position(LogicalPosition::new(0.0, y));
        let _ = webview.set_size(LogicalSize::new(size.width, height));
    }
}

fn page_builder(app: &AppHandle, url: Url) -> WebviewBuilder<Wry> {
    let popups = app.clone();
    WebviewBuilder::new(PAGE, WebviewUrl::External(url))
        .initialization_script(PAGE_SCRIPT)
        .disable_drag_drop_handler()
        .on_navigation(allowed_navigation)
        .on_new_window(move |url, _| {
            popup(&popups, url);
            NewWindowResponse::Deny
        })
        .on_page_load(|webview, payload| page_loaded(&webview, payload.url(), payload.event()))
}

/// Only 弈客 documents load in the page (and the inert `about:` / 弈客 `blob:` frames its
/// documents create); anything else is refused.
fn allowed_navigation(url: &Url) -> bool {
    match url.scheme() {
        "https" => is_yike(url),
        "about" => true,
        "blob" => Url::parse(url.path()).is_ok_and(|inner| is_yike(&inner)),
        _ => false,
    }
}

/// BF `onBeforePopup` (372-388): a 弈客 popup loads in the page itself; any other address goes
/// to the system browser.
fn popup(app: &AppHandle, url: Url) {
    if is_yike(&url) {
        if let Some(page) = app.get_webview(PAGE) {
            let _ = page.navigate(url);
        }
    } else if matches!(url.scheme(), "http" | "https") {
        let _ = app.opener().open_url(url.as_str(), None::<&str>);
    }
}

/// Each load reports its address; while following a 对局室 the poller is started in the new
/// document (BF onLoadStart / onLoadEnd 198-209).
fn page_loaded(webview: &Webview, url: &Url, event: PageLoadEvent) {
    if is_yike(url) {
        let app = webview.app_handle().clone();
        let url = url.to_string();
        // Load events arrive on the main thread; the workbench is not locked there.
        tauri::async_runtime::spawn_blocking(move || address_changed(&app, &url));
    }
    if matches!(event, PageLoadEvent::Finished) {
        let poller = *webview.state::<PageState>().poller.lock().unwrap();
        if let Some((room, interval_ms)) = poller {
            let _ = webview.eval(start_poller(room, interval_ms));
        }
    }
}

/// 后退 (BF 404-410: `if (browser_.canGoBack()) browser_.goBack()`).
#[tauri::command]
pub fn yike_browser_back(app: AppHandle) {
    if let Some(page) = app.get_webview(PAGE) {
        let _ = page.eval("history.back();");
    }
}

/// 加载 / Enter in the address field (BF 291-308, 413-422): loads `url` in the page.
#[tauri::command]
pub fn yike_browser_navigate(app: AppHandle, url: String) -> Result<(), ApiError> {
    let url = page_url(&url).ok_or_else(not_yike)?;
    let page = app.get_webview(PAGE).ok_or_else(|| ApiError::new(ErrorKind::Unavailable, "the 弈客 page is not open"))?;
    forget_address(&app);
    page.navigate(url).map_err(failed)
}

/// The window title: the base title, or `base  —  status` (BF `setSyncStatus` 1072-1084).
#[tauri::command]
pub fn yike_browser_set_title(app: AppHandle, title: String) {
    if let Some(window) = app.get_window(BROWSER) {
        let _ = window.set_title(&title);
    }
}

/// Both windows hide on close (YLD `HIDE_ON_CLOSE`, BF windowClosing 464-476).
pub fn hides_on_close(label: &str) -> bool {
    label == LIVE || label == BROWSER
}

/// The web window's placement is kept for its next opening (BF 468-473: `bowserX/Y/Width/
/// Height`), then the window hides.
pub fn hide(window: &Window) {
    if window.label() == BROWSER {
        if let (Ok(pos), Ok(size)) = (window.outer_position(), window.inner_size()) {
            let placement = WindowPlacement { x: pos.x, y: pos.y, width: size.width, height: size.height, maximized: false };
            window.app_handle().state::<State>().workbench.lock().unwrap().set_yike_browser_placement(placement);
        }
    }
    let _ = window.hide();
}

/// Application exit: the hidden windows must not keep it running.
pub fn destroy(app: &AppHandle) {
    for label in [LIVE, BROWSER] {
        if let Some(window) = app.get_window(label) {
            let _ = window.destroy();
        }
    }
}

/// `encodeURIComponent`.
fn encode_component(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    for b in text.bytes() {
        if b.is_ascii_alphanumeric() || b"-_.!~*'()".contains(&b) {
            out.push(char::from(b));
        } else {
            let _ = write!(out, "%{b:02X}");
        }
    }
    out
}

fn not_yike() -> ApiError {
    ApiError::new(ErrorKind::InvalidArgument, "only 弈客 addresses open in the 弈客 web window")
}

fn failed(error: tauri::Error) -> ApiError {
    ApiError::new(ErrorKind::Unavailable, format!("the 弈客 web window failed: {error}"))
}
