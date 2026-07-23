use std::io::{self, BufRead, Write};

use anyhow::{Context, Result};
use serde_json::Value;

use crate::api::{ApiClient, Endpoints};
use crate::{credentials, state};

pub fn run(installation_id: &str, endpoints: &Endpoints) -> Result<()> {
    let local = state::load()?;
    let installation = local.installations.get(installation_id).with_context(|| {
        format!("unknown local installation {installation_id}; run byrectl login")
    })?;
    if installation.mcp_url != endpoints.mcp_url {
        anyhow::bail!(
            "installation endpoint {} does not match selected endpoint {}; rerun login",
            installation.mcp_url,
            endpoints.mcp_url
        );
    }
    let api_key = credentials::load(installation_id, installation.credential_storage)?;
    let api = ApiClient::new(endpoints)?;
    let stdin = io::stdin();
    let mut stdout = io::stdout().lock();
    let mut session_id: Option<String> = None;
    let mut protocol_version: Option<String> = None;

    for line in stdin.lock().lines() {
        let line = line.context("read MCP stdio request")?;
        if line.trim().is_empty() {
            continue;
        }
        let message: Value = serde_json::from_str(&line).context("invalid MCP JSON-RPC request")?;
        let requested_protocol = message
            .get("params")
            .and_then(|params| params.get("protocolVersion"))
            .and_then(Value::as_str)
            .map(str::to_owned);
        let response = api.forward_mcp(
            &api_key,
            session_id.as_deref(),
            protocol_version.as_deref(),
            &line,
        )?;
        if let Some(next) = response.session_id {
            session_id = Some(next);
        }
        if requested_protocol.is_some() {
            protocol_version = requested_protocol;
        }
        if response.no_content {
            continue;
        }
        if response.content_type.starts_with("text/event-stream") {
            write_sse_data(&mut stdout, &response.body)?;
        } else {
            write_json_line(&mut stdout, response.body.trim())?;
        }
    }
    if let Some(session_id) = session_id {
        api.close_mcp(&api_key, &session_id);
    }
    Ok(())
}

fn write_sse_data(output: &mut impl Write, body: &str) -> Result<()> {
    let mut event_data = Vec::new();
    for line in body.lines().chain(std::iter::once("")) {
        if let Some(data) = line.strip_prefix("data:") {
            event_data.push(data.trim_start());
        } else if line.is_empty() && !event_data.is_empty() {
            write_json_line(output, &event_data.join("\n"))?;
            event_data.clear();
        }
    }
    output.flush().context("flush MCP SSE response")
}

fn write_json_line(output: &mut impl Write, payload: &str) -> Result<()> {
    let value: Value = serde_json::from_str(payload).context("invalid MCP JSON-RPC response")?;
    serde_json::to_writer(&mut *output, &value).context("write MCP JSON-RPC response")?;
    writeln!(output).context("terminate MCP JSON-RPC response")?;
    output.flush().context("flush MCP JSON-RPC response")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn extracts_only_sse_data_records() {
        let mut output = Vec::new();
        write_sse_data(
            &mut output,
            "event: message\ndata: {\"jsonrpc\":\"2.0\",\"id\":1}\n\n",
        )
        .expect("parse SSE");
        assert_eq!(
            String::from_utf8(output).expect("UTF-8"),
            "{\"jsonrpc\":\"2.0\",\"id\":1}\n"
        );
    }
}
