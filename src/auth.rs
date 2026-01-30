use anyhow::{Context, Result, anyhow};
use reqwest::Client;
use serde::Deserialize;

#[derive(Debug, Clone)]
pub struct AuthConfig {
    pub access_token: Option<String>,
    pub client_id: Option<String>,
    pub client_secret: Option<String>,
    pub refresh_token: Option<String>,
}

#[derive(Debug, Deserialize)]
struct TokenResponse {
    access_token: String,
}

pub fn normalize_customer_id(value: &str) -> String {
    value.chars().filter(|c| c.is_ascii_digit()).collect()
}

pub async fn resolve_access_token(config: &AuthConfig) -> Result<String> {
    if let Some(token) = config.access_token.as_ref() {
        if !token.trim().is_empty() {
            return Ok(token.trim().to_string());
        }
    }

    let client_id = config
        .client_id
        .as_ref()
        .ok_or_else(|| anyhow!("GOOGLE_ADS_CLIENT_ID missing"))?;
    let client_secret = config
        .client_secret
        .as_ref()
        .ok_or_else(|| anyhow!("GOOGLE_ADS_CLIENT_SECRET missing"))?;
    let refresh_token = config
        .refresh_token
        .as_ref()
        .ok_or_else(|| anyhow!("GOOGLE_ADS_REFRESH_TOKEN missing"))?;

    let client = Client::new();
    let resp = client
        .post("https://oauth2.googleapis.com/token")
        .form(&[
            ("grant_type", "refresh_token"),
            ("client_id", client_id.as_str()),
            ("client_secret", client_secret.as_str()),
            ("refresh_token", refresh_token.as_str()),
        ])
        .send()
        .await
        .context("request OAuth token")?;

    let status = resp.status();
    let body = resp.text().await.context("read OAuth response")?;
    if !status.is_success() {
        return Err(anyhow!("oauth http {}: {}", status, body));
    }

    let token: TokenResponse = serde_json::from_str(&body).context("decode OAuth response")?;
    Ok(token.access_token)
}
