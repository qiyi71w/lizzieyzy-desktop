use super::{data, http::Http};
use lizzie_core::cloud::{Catalog, CloudFailure};
use lizzie_core::engine::ssh::{HostKeyCheck, KnownHosts};
use russh::{client, ChannelMsg, Disconnect};
use std::sync::{mpsc, Arc};
use std::time::Duration;

const WORLD: &str = "https://ikatago-fairyland.oss-cn-beijing.aliyuncs.com/world.json";
/// Java obtains the SSH endpoint from HTTPS world/account metadata and uses its separately
/// provisioned connect-account password, not the user's login password or engine transport.
/// The server's host key is checked against the remote engines' trusted keys before that
/// password is sent (Java B2 `SshCommandSession.open` with `SshKnownHosts`).
pub(super) fn refresh(http: &Http, base: &str, token: &str, world: Option<&str>, known_hosts: &Arc<KnownHosts>) -> Result<Catalog, CloudFailure> {
    let connect = http.get(&format!("{base}/api/cluster/account/connectAccount/fetch"), token)?;
    let username = data::string(&connect,"connectUsername");
    let password = data::string(&connect,"connectPassword");
    if username.trim().is_empty() || password.is_empty() { return Err(CloudFailure::new("invalid_response")); }
    let metadata = http.get(world.unwrap_or(WORLD), "")?;
    let platform = metadata.get("platforms").and_then(|v| v.as_array()).and_then(|v| v.iter().find(|v| data::string(v,"name") == "all"))
        .ok_or_else(|| CloudFailure::new("invalid_catalog"))?;
    let mut metadata_base = platform.get("http").map(|v| data::string(v,"getUrl")).unwrap_or_default();
    if metadata_base.trim().is_empty() {
        let oss = platform.get("oss").ok_or_else(|| CloudFailure::new("invalid_catalog"))?;
        let bucket = data::string(oss,"bucket");
        let endpoint = data::string(oss,"bucketEndpoint");
        if bucket.is_empty() || endpoint.is_empty() { return Err(CloudFailure::new("invalid_catalog")); }
        metadata_base = format!("https://{bucket}.{endpoint}");
    }
    let mut encoder = reqwest::Url::parse("http://localhost/").map_err(|_| CloudFailure::new("invalid_catalog"))?;
    encoder.query_pairs_mut().append_pair("",username.trim());
    let encoded = encoder.query().unwrap_or("").trim_start_matches('=').replace('+',"%20");
    let url = format!("{}/users/{encoded}.ssh.json",metadata_base.trim_end_matches('/'));
    let endpoint = http.get(&url, "")?;
    let host = data::string(&endpoint,"host").trim().to_owned();
    let user = data::string(&endpoint,"user").trim().to_owned();
    let port = endpoint.get("port").and_then(|v| v.as_u64()).unwrap_or(22);
    if host.is_empty() || user.is_empty() || !(1..=65535).contains(&port) { return Err(CloudFailure::new("invalid_catalog")); }
    let cancellation = http.clone();
    let known_hosts = known_hosts.clone();
    let (tx, rx) = mpsc::sync_channel(1);
    tauri::async_runtime::spawn(async move {
        let result = tokio::select! {
            result = tokio::time::timeout(Duration::from_secs(10), query(known_hosts, &host, port as u16, &user, &password)) =>
                result.unwrap_or_else(|_| Err(CloudFailure::new("catalog_timeout"))),
            _ = cancellation.wait_cancelled() => Err(CloudFailure::cancelled()),
        };
        let _ = tx.send(result);
    });
    loop {
        http.current()?;
        match rx.recv_timeout(Duration::from_millis(25)) {
            Ok(result) => return Catalog::from_server(&result?),
            Err(mpsc::RecvTimeoutError::Timeout) => {},
            Err(_) => return Err(CloudFailure::new("catalog_unavailable")),
        }
    }
}
async fn query(known_hosts: Arc<KnownHosts>, host: &str, port: u16, user: &str, password: &str) -> Result<serde_json::Value, CloudFailure> {
    let fail = |_| CloudFailure::new("catalog_unavailable");
    let config = Arc::new(client::Config { nodelay: true, ..Default::default() });
    let check = HostKeyCheck::new(known_hosts, host, port);
    let refused = check.refusal();
    let mut session = match client::connect(config, (host,port), check).await {
        Ok(session) => session,
        Err(_) => return Err(match refused.take() {
            Some(refusal) => CloudFailure { host_key: Some(refusal), ..CloudFailure::new("host_key_refused") },
            None => CloudFailure::new("catalog_unavailable"),
        }),
    };
    if !session.authenticate_password(user,password).await.map_err(fail)?.success() {
        return Err(CloudFailure::new("catalog_authentication_failed"));
    }
    let mut channel = session.channel_open_session().await.map_err(fail)?;
    channel.exec(true, "query-server").await.map_err(fail)?;
    let mut bytes = Vec::new();
    while let Some(msg) = channel.wait().await {
        match msg {
            ChannelMsg::Data { data } => {
                if bytes.len() + data.len() > 512 * 1024 { return Err(CloudFailure::new("invalid_catalog")); }
                bytes.extend_from_slice(&data);
            }
            ChannelMsg::Close => break,
            _ => {},
        }
    }
    let _ = session.disconnect(Disconnect::ByApplication,"","en").await;
    serde_json::from_slice(&bytes).map_err(|_| CloudFailure::new("invalid_catalog"))
}
