//! Fixed Java Zhizi response schema; untrusted objects are converted here, not in the UI.
use lizzie_core::cloud::*;
use rust_decimal::Decimal;
use serde_json::Value;
use std::str::FromStr;

pub(super) fn string(v: &Value, key: &str) -> String { v.get(key).and_then(Value::as_str).unwrap_or("").to_owned() }
pub(super) fn number(v: &Value, key: &str) -> u64 { v.get(key).and_then(Value::as_u64).unwrap_or(0) }
fn boolean(v: &Value, key: &str) -> bool { v.get(key).and_then(Value::as_bool).unwrap_or(false) }
pub(super) fn decimal(v: &Value, key: &str) -> Decimal {
    let raw = v.get(key).map(|value| value.as_str().map(str::to_owned).unwrap_or_else(|| value.to_string())).unwrap_or_default();
    Decimal::from_str(&raw).or_else(|_| Decimal::from_scientific(&raw)).unwrap_or_default()
}
fn amount(v: &Value, key: &str) -> String { decimal(v, key).to_string() }
fn instant(v: &Value, key: &str) -> Option<String> {
    chrono::DateTime::parse_from_rfc3339(&string(v, key)).ok().map(|v| v.with_timezone(&chrono::Utc).to_rfc3339())
}
pub(super) fn account(v: &Value) -> Result<AccountProfile, CloudFailure> {
    object(v)?;
    Ok(AccountProfile { phone: string(v, "phone"), email: string(v, "email"), membership: boolean(v, "isMembership"),
        membership_expires_at: instant(v, "membershipExpiresAt"), membership_auto_renew: v.get("membershipAutoRenew").and_then(Value::as_bool) })
}
pub(super) fn balance(v: &Value) -> Result<BalanceInfo, CloudFailure> {
    object(v)?;
    Ok(BalanceInfo { remaining_balance_yuan: amount(v,"remainingBalance"), total_cash_amount_yuan: amount(v,"totalCashAmount"),
        yesterday_consumption_yuan: amount(v,"yesterdayConsumption"), total_consumption_yuan: amount(v,"totalConsumption"),
        total_duration_seconds: number(v,"totalDuration"), last24_hours_share_duration_seconds: number(v,"last24HrsShareDuration"),
        last24_hours_vip_duration_seconds: number(v,"last24HrsVIPShareDuration"), current_connections: small(v,"currentNumOfMyConnections"), current_nodes: small(v,"currentNumOfNodes") })
}
fn small(v: &Value, key: &str) -> u32 { number(v,key).min(u32::MAX as u64) as u32 }
fn object(v: &Value) -> Result<(), CloudFailure> { if v.is_object() { Ok(()) } else { Err(CloudFailure::new("invalid_response")) } }
pub(super) fn usages(v: &Value, page: u32, size: u32) -> Result<UsagePage, CloudFailure> {
    object(v)?;
    let items: Vec<_> = v.get("items").and_then(Value::as_array).into_iter().flatten().filter(|v| v.is_object()).map(|v| UsageRecord {
        started_at: instant(v,"startedAt"), ended_at: instant(v,"endedAt"), finished: boolean(v,"finished"), ready: boolean(v,"ready"),
        duration_seconds: number(v,"duration"), total_cost_yuan: amount(v,"totalCost"), gpu_type: string(v,"gpuType"),
        engine_type: string(v,"engineType"), vip: boolean(v,"vip"), shared: boolean(v,"share"),
    }).collect();
    Ok(UsagePage { total: v.get("total").map(|_| number(v,"total")).unwrap_or(items.len() as u64),
        page: v.get("page").map(|_| small(v,"page")).unwrap_or(page),
        page_size: v.get("pageSize").map(|_| small(v,"pageSize").max(1)).unwrap_or(size), items })
}
pub(super) fn credits(v: &Value, page: u32, size: u32) -> Result<CreditPage, CloudFailure> {
    object(v)?;
    let items: Vec<_> = v.get("items").and_then(Value::as_array).into_iter().flatten().filter(|v| v.is_object()).map(|v| CreditRecord {
        credit_type: string(v,"creditType"), amount_yuan: amount(v,"amount"), source: string(v,"source"),
        product_name: string(v,"productName"), created_at: instant(v,"createdAt"),
    }).collect();
    Ok(CreditPage { total: v.get("total").map(|_| number(v,"total")).unwrap_or(items.len() as u64),
        page: v.get("page").map(|_| small(v,"page")).unwrap_or(page),
        page_size: v.get("pageSize").map(|_| small(v,"pageSize").max(1)).unwrap_or(size), items })
}
fn positive_integer(v: &Value, key: &str) -> Option<u64> {
    let value = decimal(v,key);
    if value <= Decimal::ZERO || !value.fract().is_zero() { return None; }
    value.normalize().to_string().parse().ok()
}
pub(super) fn products(v: &Value) -> Result<Vec<MembershipProduct>, CloudFailure> {
    let array = v.as_array().ok_or_else(|| CloudFailure::new("invalid_response"))?;
    let mut products: Vec<_> = array.iter().filter_map(|v| {
        if string(v,"type") != "MEMBERSHIP" { return None; }
        let name = string(v,"name").trim().to_owned();
        let duration_months = match name.as_str() { "MEMBERSHIP_1_MONTH" => 1, "MEMBERSHIP_3_MONTH" => 3,
            "MEMBERSHIP_6_MONTH" => 6, "MEMBERSHIP_12_MONTH" => 12, _ => return None };
        Some(MembershipProduct { name, duration_months, price_fen: positive_integer(v,"price")? })
    }).collect();
    products.sort_by_key(|p| p.duration_months);
    Ok(products)
}
pub(super) fn valid_order_id(id: &str) -> bool { id.len() == 24 && id.bytes().all(|b| b.is_ascii_hexdigit()) }
pub(super) fn order(v: &Value) -> Result<PaymentOrder, CloudFailure> {
    let invalid = || CloudFailure::new("invalid_response");
    let id = string(v,"id").trim().to_owned();
    if !valid_order_id(&id) { return Err(invalid()); }
    let amount_fen = positive_integer(v,"amount").ok_or_else(invalid)?;
    let status = match string(v,"paidStatus").trim() { "PENDING" => PaymentStatus::Pending, "SUCCESS" => PaymentStatus::Success, "FAIL" => PaymentStatus::Fail, _ => return Err(invalid()) };
    let product_name = string(v,"productName").trim().to_owned();
    let order_type = string(v,"orderType");
    if !order_type.trim().is_empty() && order_type.trim() != "PURCHASE_PRODUCT" { return Err(invalid()); }
    let purpose = if order_type.trim() == "PURCHASE_PRODUCT" || !product_name.is_empty() { PaymentPurpose::VipMembership } else { PaymentPurpose::BalanceTopUp };
    let code_url = v.get("nativePayRequest").map(|v| string(v,"codeURL")).unwrap_or_default().trim().to_owned();
    if status == PaymentStatus::Pending && (code_url.len() > 2048 || !code_url.get(..15).is_some_and(|s| s.eq_ignore_ascii_case("weixin://wxpay/"))) { return Err(invalid()); }
    Ok(PaymentOrder { id, amount_fen, status, product_name, purpose, code_url, paid_at: instant(v,"paidAt"), updated_at: instant(v,"updatedAt") })
}

#[derive(Clone)]
pub(super) struct Baseline { pub purpose: PaymentPurpose, pub balance: Option<BalanceInfo>, pub account: Option<AccountProfile>, pub observed_at: i64 }
pub(super) fn verify(baseline: &Baseline, order: &PaymentOrder, balance: Option<&BalanceInfo>, credits: Option<&CreditPage>, account: Option<&AccountProfile>) -> PaymentVerification {
    if baseline.purpose != order.purpose { return PaymentVerification::default(); }
    let parse = |v: &str| Decimal::from_str(v).unwrap_or_default();
    if let (Some(before), Some(after), Some(credits)) = (&baseline.balance, balance, credits) {
        let amount = Decimal::new(order.amount_fen.min(i64::MAX as u64) as i64, 2);
        let remaining_balance_changed = parse(&after.remaining_balance_yuan) > parse(&before.remaining_balance_yuan);
        let cash_total_increased = parse(&after.total_cash_amount_yuan) >= parse(&before.total_cash_amount_yuan) + amount;
        let cash_credit_found = credits.items.iter().any(|c| c.credit_type == "CASH" && c.source == "PAYMENT" && parse(&c.amount_yuan) == amount
            && c.created_at.as_ref().and_then(|s| chrono::DateTime::parse_from_rfc3339(s).ok()).is_some_and(|t| t.timestamp() >= baseline.observed_at - 5));
        return PaymentVerification { settled: remaining_balance_changed && cash_total_increased && cash_credit_found, remaining_balance_changed, cash_credit_found, membership_advanced: false };
    }
    if let (Some(before), Some(after)) = (&baseline.account, account) {
        let expires = |a: &AccountProfile| a.membership_expires_at.as_ref().and_then(|s| chrono::DateTime::parse_from_rfc3339(s).ok());
        let membership_advanced = after.membership && expires(after).is_some_and(|new| !before.membership || expires(before).is_none_or(|old| new > old));
        return PaymentVerification { settled: membership_advanced, membership_advanced, ..PaymentVerification::default() };
    }
    PaymentVerification::default()
}
