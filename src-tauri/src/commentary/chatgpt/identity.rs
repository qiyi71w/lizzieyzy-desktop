use base64::{engine::general_purpose::URL_SAFE_NO_PAD, Engine};
use lizzie_core::commentary::{chatgpt::failure, CommentaryFailure};
use ring::signature::{RsaPublicKeyComponents, RSA_PKCS1_2048_8192_SHA256};
use serde_json::Value;

pub struct Identity {
    pub subject: String,
    pub email: String,
}

/// Parsing causes and unverified claims must never escape this function.
pub fn verify(
    token: &str,
    keys: &Value,
    client: &str,
    nonce: Option<&str>,
    subject: Option<&str>,
    now: u64,
) -> Result<Identity, CommentaryFailure> {
    let invalid = || failure("identity");
    if token.len() > 2_097_152 {
        return Err(invalid());
    }
    let mut parts = token.split('.');
    let header = parts.next().ok_or_else(invalid)?;
    let payload = parts.next().ok_or_else(invalid)?;
    let signature = parts.next().ok_or_else(invalid)?;
    if parts.next().is_some() {
        return Err(invalid());
    }
    let decode = |text: &str| -> Result<Value, CommentaryFailure> {
        serde_json::from_slice(&URL_SAFE_NO_PAD.decode(text).map_err(|_| invalid())?).map_err(|_| invalid())
    };
    let header = decode(header)?;
    if header["alg"] != "RS256" {
        return Err(invalid());
    }
    let kid = header["kid"].as_str().filter(|v| !v.is_empty()).ok_or_else(invalid)?;
    let key = keys["keys"]
        .as_array()
        .ok_or_else(invalid)?
        .iter()
        .find(|key| key["kid"] == kid && key["kty"] == "RSA")
        .ok_or_else(invalid)?;
    let modulus = URL_SAFE_NO_PAD
        .decode(key["n"].as_str().ok_or_else(invalid)?)
        .map_err(|_| invalid())?;
    let exponent = URL_SAFE_NO_PAD
        .decode(key["e"].as_str().ok_or_else(invalid)?)
        .map_err(|_| invalid())?;
    let signed = token.rsplit_once('.').ok_or_else(invalid)?.0;
    let signature = URL_SAFE_NO_PAD.decode(signature).map_err(|_| invalid())?;
    RsaPublicKeyComponents {
        n: modulus,
        e: exponent,
    }
    .verify(&RSA_PKCS1_2048_8192_SHA256, signed.as_bytes(), &signature)
    .map_err(|_| invalid())?;
    let claims = decode(payload)?;
    let audiences: Vec<&str> = match &claims["aud"] {
        Value::String(value) => vec![value.as_str()],
        Value::Array(values) => values
            .iter()
            .map(|v| v.as_str().ok_or_else(invalid))
            .collect::<Result<_, _>>()?,
        _ => return Err(invalid()),
    };
    let claim_subject = claims["sub"]
        .as_str()
        .filter(|v| !v.trim().is_empty())
        .ok_or_else(invalid)?;
    if claims["iss"] != "https://auth.openai.com"
        || !audiences.contains(&client)
        || claims["exp"].as_u64().is_none_or(|expiry| expiry <= now)
        || claims
            .get("nbf")
            .is_some_and(|v| v.as_u64().is_none_or(|v| v > now + 30))
        || claims
            .get("iat")
            .is_some_and(|v| v.as_u64().is_none_or(|v| v > now + 30))
        || nonce.is_some_and(|v| claims["nonce"].as_str() != Some(v))
        || subject.is_some_and(|v| claim_subject != v)
        || claims.get("azp").is_some_and(|v| v.as_str() != Some(client))
        || (audiences.len() > 1 && claims["azp"].as_str() != Some(client))
    {
        return Err(invalid());
    }
    Ok(Identity {
        subject: claim_subject.into(),
        email: claims["email"].as_str().unwrap_or("ChatGPT").into(),
    })
}
