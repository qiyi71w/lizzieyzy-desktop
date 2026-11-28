//! Cloud account settings and operations. Account credentials belong to the shell adapter,
//! never to settings, engine profiles, replies, or diagnostic output.
use serde::{Deserialize, Serialize};
use ts_rs::TS;

mod catalog;
pub use catalog::{kata_weight, Catalog, DiscoverySource, Weight};

pub const VIP_ARGS: &str = "--platform all --engine-type go --gpu-type vip-share --kata-name katago-TENSORRT --kata-weight 10b512t";
pub const TOP_UP_PRESETS_FEN: [u32; 4] = [1000, 3000, 5000, 10000];

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(rename = "CloudAccountProvider")]
pub enum Provider { #[default] Local, Zhizi, Custom }

#[derive(Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(default, rename_all = "camelCase")]
pub struct CloudSettings {
    pub provider: Provider,
    pub identifier: String,
    pub remember_token: bool,
    pub remember_password: bool,
    pub args: String,
    /// Java's persistent custom websocket URL may itself contain authentication material.
    pub custom_url: String,
    pub catalog: Catalog,
    pub catalog_updated_at: u64,
}
impl Default for CloudSettings {
    fn default() -> Self {
        Self { provider: Provider::Local, identifier: String::new(), remember_token: false,
            remember_password: false, args: VIP_ARGS.into(), custom_url: String::new(),
            catalog: Catalog::default(), catalog_updated_at: 0 }
    }
}
impl std::fmt::Debug for CloudSettings {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("CloudSettings").field("provider", &self.provider)
            .field("identifier", &self.identifier).field("remember_token", &self.remember_token)
            .field("remember_password", &self.remember_password).field("args", &"[redacted]")
            .field("custom_url", &"[redacted]").field("catalog", &self.catalog)
            .field("catalog_updated_at", &self.catalog_updated_at).finish()
    }
}
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "snake_case")]
pub enum CodePurpose { FastLogin, ResetPassword }

/// Deliberately not Debug: deserialized commands can contain passwords or verification codes.
#[derive(Clone, PartialEq, Serialize, Deserialize, TS)]
#[serde(tag = "type")]
pub enum CloudCommand {
    State,
    PasswordLogin { identifier: String, password: String, remember_token: bool, remember_password: bool },
    CodeLogin { identifier: String, code: String, remember_token: bool },
    SendCode { identifier: String, purpose: CodePurpose },
    ResetPassword { identifier: String, code: String, password: String, remember_token: bool, remember_password: bool },
    Remember { token: bool, password: bool },
    Logout,
    Overview { force: bool },
    Usages { page: u32, page_size: u32, finished: Option<bool> },
    Credits { page: u32, page_size: u32, credit_type: Option<String> },
    RefreshCatalog { args: String },
    Products,
    CreateOrder { purchase: Purchase, confirmed: bool },
    ReadOrder { id: String },
    VerifyPayment { id: String },
}
impl std::fmt::Debug for CloudCommand {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("CloudCommand([redacted])")
    }
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(tag = "type")]
pub enum Purchase { TopUp { amount_fen: u32 }, Membership { product_name: String, auto_renew: bool } }

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
pub struct CloudState {
    pub authenticated: bool,
    pub has_password: bool,
    pub credential_backend: String,
    pub credential_available: bool,
    pub token_stored_securely: bool,
    pub password_stored_securely: bool,
    pub storage_warning: bool,
    pub generation: u64,
}
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
pub struct CloudReply { pub settings: CloudSettings, pub state: CloudState, pub result: CloudResult }
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, TS)]
#[serde(tag = "type")]
pub enum CloudResult {
    State,
    LoggedIn,
    CodeSent { retry_after_seconds: u32 },
    PasswordReset,
    LoggedOut,
    Overview { overview: Overview },
    Usages { page: UsagePage },
    Credits { page: CreditPage },
    Catalog { catalog: Catalog },
    Products { products: Vec<MembershipProduct>, top_up_presets_fen: Vec<u32> },
    Order { order: PaymentOrder },
    PaymentVerification { order: PaymentOrder, verification: PaymentVerification },
}
/// Only fixed, sanitized keys cross the shell seam; service response text is never displayed.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
pub struct CloudFailure {
    pub key: String,
    pub status: u16,
    pub retry_after_seconds: u32,
    pub retryable: bool,
    pub uncertain: bool,
    pub cancelled: bool,
    pub authentication_invalidated: bool,
    /// The model catalog's SSH server was refused before any credential was sent: its host key is
    /// not the trusted one, or the trusted keys could not be read (key `host_key_refused`).
    pub host_key: Option<crate::engine::ssh::RemoteFailure>,
}
impl CloudFailure {
    pub fn new(key: &str) -> Self {
        Self { key: key.into(), status: 0, retry_after_seconds: 0, retryable: false,
            uncertain: false, cancelled: false, authentication_invalidated: false, host_key: None }
    }
    pub fn cancelled() -> Self { Self { cancelled: true, ..Self::new("cancelled") } }
}
impl std::fmt::Display for CloudFailure {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result { write!(f, "cloud request failed ({})", self.key) }
}
impl std::error::Error for CloudFailure {}

/// Currency is an exact decimal string in yuan, not a binary floating point amount.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
pub struct AccountProfile {
    pub phone: String, pub email: String, pub membership: bool,
    pub membership_expires_at: Option<String>, pub membership_auto_renew: Option<bool>,
}
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
pub struct BalanceInfo {
    pub remaining_balance_yuan: String, pub total_cash_amount_yuan: String,
    pub yesterday_consumption_yuan: String, pub total_consumption_yuan: String,
    pub total_duration_seconds: u64, pub last24_hours_share_duration_seconds: u64,
    pub last24_hours_vip_duration_seconds: u64, pub current_connections: u32, pub current_nodes: u32,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
pub struct UsageRecord {
    pub started_at: Option<String>, pub ended_at: Option<String>, pub finished: bool, pub ready: bool,
    pub duration_seconds: u64, pub total_cost_yuan: String, pub gpu_type: String,
    pub engine_type: String, pub vip: bool, pub shared: bool,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
pub struct UsagePage { pub total: u64, pub page: u32, pub page_size: u32, pub items: Vec<UsageRecord> }
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
pub struct CreditRecord {
    pub credit_type: String, pub amount_yuan: String, pub source: String,
    pub product_name: String, pub created_at: Option<String>,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
pub struct CreditPage { pub total: u64, pub page: u32, pub page_size: u32, pub items: Vec<CreditRecord> }
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
pub struct Overview { pub account: AccountProfile, pub balance: BalanceInfo, pub recent_usage: UsagePage, pub loaded_at: u64 }
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
pub struct MembershipProduct { pub name: String, pub price_fen: u64, pub duration_months: u32 }
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize, TS)]
pub enum PaymentStatus { Pending, Success, Fail }
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize, TS)]
pub enum PaymentPurpose { BalanceTopUp, VipMembership }
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
pub struct PaymentOrder {
    pub id: String, pub amount_fen: u64, pub product_name: String, pub purpose: PaymentPurpose,
    pub status: PaymentStatus, pub code_url: String, pub paid_at: Option<String>, pub updated_at: Option<String>,
}
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
pub struct PaymentVerification {
    pub settled: bool, pub remaining_balance_changed: bool, pub cash_credit_found: bool, pub membership_advanced: bool,
}
/// Engine-only credential return. This type intentionally has no Serialize or Debug.
pub struct CloudSocketToken { pub socket_url: String, pub token: String }
