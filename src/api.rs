use std::time::Duration;

use anyhow::{Context, Result, bail};
use reqwest::StatusCode;
use reqwest::blocking::{Client, Response};
use serde::{Deserialize, Serialize};
use serde_json::Value;

const PRODUCTION_API_BASE: &str = "https://api.byre.cc";
const DEVELOPMENT_API_BASE: &str = "http://127.0.0.1:8000";
const DEVELOPMENT_MCP_URL: &str = "http://127.0.0.1:8001/mcp";

#[derive(Clone, Debug)]
pub struct Endpoints {
    pub api_base: &'static str,
    pub mcp_url: &'static str,
}

impl Endpoints {
    pub fn for_mode(development: bool) -> Self {
        if development {
            Self {
                api_base: DEVELOPMENT_API_BASE,
                mcp_url: DEVELOPMENT_MCP_URL,
            }
        } else {
            Self {
                api_base: PRODUCTION_API_BASE,
                mcp_url: "https://api.byre.cc/mcp",
            }
        }
    }
}

pub struct ApiClient {
    client: Client,
    endpoints: Endpoints,
}

impl ApiClient {
    pub fn new(endpoints: &Endpoints) -> Result<Self> {
        let client = Client::builder()
            .connect_timeout(Duration::from_secs(10))
            .timeout(Duration::from_secs(90))
            .user_agent(concat!("byrectl/", env!("CARGO_PKG_VERSION")))
            .build()
            .context("build HTTP client")?;
        Ok(Self {
            client,
            endpoints: endpoints.clone(),
        })
    }

    pub fn create_device_code(&self) -> Result<DeviceCodeResponse> {
        let response = self
            .client
            .post(self.url("/v1/auth/device/code"))
            .json(&DeviceCodeRequest {
                client_id: "byrectl",
                client_version: env!("CARGO_PKG_VERSION"),
                requested_environment: "test",
            })
            .send()
            .context("request a device code")?;
        decode_success(response, "request a device code")
    }

    pub fn poll_device_token(&self, device_code: &str) -> Result<DevicePoll> {
        let response = self
            .client
            .post(self.url("/v1/auth/device/token"))
            .json(&DeviceTokenRequest { device_code })
            .send()
            .context("poll device authorization")?;
        if response.status().is_success() {
            return response
                .json::<DeviceTokenResponse>()
                .map(DevicePoll::Authorized)
                .context("decode device authorization response");
        }
        let status = response.status();
        let error = response
            .json::<DeviceError>()
            .context("decode device authorization error")?;
        match error.error.as_str() {
            "authorization_pending" => Ok(DevicePoll::Pending),
            "slow_down" => Ok(DevicePoll::SlowDown),
            "access_denied" => Ok(DevicePoll::Denied),
            "expired_token" => Ok(DevicePoll::Expired),
            _ => bail!("device authorization failed ({status}): {}", error.error),
        }
    }

    pub fn activate(
        &self,
        installation_token: &str,
        request: &ActivateRequest<'_>,
    ) -> Result<ActivateResponse> {
        let response = self
            .client
            .post(self.url("/v1/installations/activate"))
            .bearer_auth(installation_token)
            .json(request)
            .send()
            .context("activate installation")?;
        decode_success(response, "activate installation")
    }

    pub fn complete(&self, installation_id: &str, api_key: &str) -> Result<()> {
        let response = self
            .client
            .post(self.url(&format!("/v1/installations/{installation_id}/complete")))
            .bearer_auth(api_key)
            .send()
            .context("complete installation")?;
        if response.status().is_success() {
            return Ok(());
        }
        let status = response.status();
        let body = response.text().unwrap_or_default();
        bail!(
            "complete installation failed ({status}): {}",
            concise(&body)
        )
    }

    pub fn installation_status(
        &self,
        installation_id: &str,
        api_key: &str,
    ) -> Result<InstallationStatusResponse> {
        let response = self
            .client
            .get(self.url(&format!("/v1/installations/{installation_id}")))
            .bearer_auth(api_key)
            .send()
            .context("read installation status")?;
        decode_success(response, "read installation status")
    }

    pub fn revoke(&self, installation_id: &str, api_key: &str) -> Result<()> {
        let response = self
            .client
            .delete(self.url(&format!("/v1/installations/{installation_id}")))
            .bearer_auth(api_key)
            .send()
            .context("revoke installation")?;
        if response.status().is_success() {
            return Ok(());
        }
        let status = response.status();
        let body = response.text().unwrap_or_default();
        bail!("revoke installation failed ({status}): {}", concise(&body))
    }

    pub fn forward_mcp(
        &self,
        api_key: &str,
        session_id: Option<&str>,
        protocol_version: Option<&str>,
        body: &str,
    ) -> Result<McpResponse> {
        let mut request = self
            .client
            .post(self.endpoints.mcp_url)
            .bearer_auth(api_key)
            .header("Accept", "application/json, text/event-stream")
            .header("Content-Type", "application/json")
            .body(body.to_owned());
        if let Some(session_id) = session_id {
            request = request.header("Mcp-Session-Id", session_id);
        }
        if let Some(protocol_version) = protocol_version {
            request = request.header("MCP-Protocol-Version", protocol_version);
        }
        let response = request.send().context("forward MCP request")?;
        let status = response.status();
        let next_session_id = response
            .headers()
            .get("Mcp-Session-Id")
            .and_then(|value| value.to_str().ok())
            .map(str::to_owned);
        let content_type = response
            .headers()
            .get("Content-Type")
            .and_then(|value| value.to_str().ok())
            .unwrap_or_default()
            .to_owned();
        let body = response.text().context("read MCP response")?;
        if !status.is_success() {
            bail!("MCP upstream failed ({status}): {}", concise(&body));
        }
        Ok(McpResponse {
            body,
            content_type,
            session_id: next_session_id,
            no_content: status == StatusCode::ACCEPTED || status == StatusCode::NO_CONTENT,
        })
    }

    pub fn close_mcp(&self, api_key: &str, session_id: &str) {
        let _ = self
            .client
            .delete(self.endpoints.mcp_url)
            .bearer_auth(api_key)
            .header("Mcp-Session-Id", session_id)
            .send();
    }

    pub fn verify_mcp(&self, api_key: &str) -> Result<()> {
        let request = serde_json::json!({
            "jsonrpc": "2.0",
            "id": "byrectl-preflight",
            "method": "initialize",
            "params": {
                "protocolVersion": "2025-06-18",
                "capabilities": {},
                "clientInfo": {
                    "name": "byrectl",
                    "version": env!("CARGO_PKG_VERSION")
                }
            }
        });
        let response = self.forward_mcp(api_key, None, None, &request.to_string())?;
        if let Some(session_id) = response.session_id.as_deref() {
            self.close_mcp(api_key, session_id);
        }
        if response.no_content {
            bail!("MCP verification returned no initialization response")
        }
        let payload = if response.content_type.starts_with("text/event-stream") {
            first_sse_data(&response.body).context("MCP verification SSE contained no data")?
        } else {
            response.body
        };
        let value: Value =
            serde_json::from_str(&payload).context("decode MCP verification response")?;
        if let Some(error) = value.get("error") {
            bail!(
                "MCP verification was rejected: {}",
                concise(&error.to_string())
            )
        }
        if value.get("result").is_none() {
            bail!("MCP verification response did not contain a result")
        }
        Ok(())
    }

    fn url(&self, path: &str) -> String {
        format!("{}{path}", self.endpoints.api_base)
    }
}

fn decode_success<T: for<'de> Deserialize<'de>>(response: Response, action: &str) -> Result<T> {
    let status = response.status();
    if status.is_success() {
        return response
            .json::<T>()
            .with_context(|| format!("decode response while attempting to {action}"));
    }
    let body = response.text().unwrap_or_default();
    bail!("failed to {action} ({status}): {}", concise(&body))
}

fn concise(body: &str) -> String {
    serde_json::from_str::<Value>(body)
        .ok()
        .and_then(|value| {
            value
                .get("detail")
                .or_else(|| value.get("error"))
                .and_then(Value::as_str)
                .map(str::to_owned)
        })
        .unwrap_or_else(|| body.chars().take(200).collect())
}

fn first_sse_data(body: &str) -> Option<String> {
    let mut data = Vec::new();
    for line in body.lines().chain(std::iter::once("")) {
        if let Some(value) = line.strip_prefix("data:") {
            data.push(value.trim_start());
        } else if line.is_empty() && !data.is_empty() {
            return Some(data.join("\n"));
        }
    }
    None
}

#[derive(Serialize)]
struct DeviceCodeRequest<'a> {
    client_id: &'a str,
    client_version: &'a str,
    requested_environment: &'a str,
}

#[derive(Debug, Deserialize)]
pub struct DeviceCodeResponse {
    pub device_code: String,
    pub user_code: String,
    pub verification_uri: String,
    pub verification_uri_complete: String,
    pub expires_in: u64,
    pub interval: u64,
}

#[derive(Serialize)]
struct DeviceTokenRequest<'a> {
    device_code: &'a str,
}

#[derive(Debug, Deserialize)]
pub struct DeviceTokenResponse {
    pub installation_token: String,
    #[allow(dead_code)]
    pub expires_in: u64,
}

#[derive(Debug, Deserialize)]
pub struct InstallationStatusResponse {
    pub installation_id: String,
    pub status: String,
    pub clients: Vec<String>,
    pub cli_version: String,
    pub skill_version: Option<String>,
    pub api_key_id: String,
    pub api_key_status: String,
    pub api_key_expires_at: Option<String>,
    pub scopes: Vec<String>,
    pub platforms: Vec<String>,
}

#[derive(Debug, Deserialize)]
struct DeviceError {
    error: String,
}

pub enum DevicePoll {
    Pending,
    SlowDown,
    Denied,
    Expired,
    Authorized(DeviceTokenResponse),
}

#[derive(Debug, Serialize)]
pub struct ActivateRequest<'a> {
    pub device_id: &'a str,
    pub clients: &'a [String],
    pub cli_version: &'a str,
    pub skill_version: Option<&'a str>,
}

#[derive(Debug, Deserialize)]
pub struct ActivateResponse {
    pub installation_id: String,
    pub api_key_id: String,
    pub api_key: String,
    pub expires_at: String,
    pub scopes: Vec<String>,
    pub platforms: Vec<String>,
}

pub struct McpResponse {
    pub body: String,
    pub content_type: String,
    pub session_id: Option<String>,
    pub no_content: bool,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reads_first_complete_sse_data_event() {
        assert_eq!(
            first_sse_data("event: message\ndata: {\"result\":\ndata: {}}\n\n"),
            Some("{\"result\":\n{}}".to_owned())
        );
    }
}
