//! `racklog-mcp` — Model Context Protocol server for RACKLOG.
//!
//! Wraps the RACKLOG REST API as a stdio-based MCP server so any
//! MCP-aware LLM client (Claude Desktop, Continue.dev, Cursor,
//! Cline, etc.) can answer questions like "where's my Pinecil"
//! or "what cables under 1m do I have" against the live inventory.
//!
//! # Usage
//!
//! ```text
//! RACKLOG_URL=http://127.0.0.1:8080 \
//! RACKLOG_API_KEY=rl_…              \
//!     racklog-mcp
//! ```
//!
//! Wire-format is newline-delimited JSON-RPC 2.0 on stdin/stdout
//! per the MCP spec. Logs (server start, tool errors) go to stderr
//! so the protocol channel stays clean.

use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::io::{BufRead, Write};

const PROTOCOL_VERSION: &str = "2024-11-05";
const SERVER_NAME: &str = "racklog";
const SERVER_VERSION: &str = env!("CARGO_PKG_VERSION");

#[derive(Debug, Deserialize)]
struct JsonRpcRequest {
    #[allow(dead_code)]
    jsonrpc: String,
    id: Option<Value>,
    method: String,
    #[serde(default)]
    params: Value,
}

#[derive(Debug, Serialize)]
struct JsonRpcResponse {
    jsonrpc: &'static str,
    id: Option<Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    result: Option<Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    error: Option<JsonRpcError>,
}

#[derive(Debug, Serialize)]
struct JsonRpcError {
    code: i64,
    message: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    data: Option<Value>,
}

struct Client {
    base: String,
    api_key: Option<String>,
    http: reqwest::Client,
}

impl Client {
    fn from_env() -> Self {
        let base = std::env::var("RACKLOG_URL")
            .unwrap_or_else(|_| "http://127.0.0.1:8080".to_string())
            .trim_end_matches('/')
            .to_string();
        let api_key = std::env::var("RACKLOG_API_KEY").ok();
        let http = reqwest::Client::builder()
            .timeout(std::time::Duration::from_secs(10))
            .user_agent(concat!("racklog-mcp/", env!("CARGO_PKG_VERSION")))
            .build()
            .expect("reqwest client");
        Self {
            base,
            api_key,
            http,
        }
    }

    async fn get(&self, path: &str) -> Result<Value, String> {
        let url = format!("{}{}", self.base, path);
        let mut req = self.http.get(&url);
        if let Some(k) = &self.api_key {
            req = req.bearer_auth(k);
        }
        let resp = req.send().await.map_err(|e| format!("network: {e}"))?;
        if !resp.status().is_success() {
            return Err(format!("upstream returned {}", resp.status()));
        }
        resp.json::<Value>()
            .await
            .map_err(|e| format!("decode: {e}"))
    }
}

fn tools_descriptor() -> Value {
    // Each tool is a name + description + JSON-schema input shape.
    // Keep schemas minimal — descriptions are what the LLM actually
    // reads to decide when to fire each tool.
    json!({
        "tools": [
            {
                "name": "list_items",
                "description": "List items in the inventory catalog. Optional filters: category, supplier id, search query (matches sku and name), limit/offset for pagination.",
                "inputSchema": {
                    "type": "object",
                    "properties": {
                        "category": {"type": "string"},
                        "supplier": {"type": "string"},
                        "q":        {"type": "string"},
                        "limit":    {"type": "integer", "minimum": 1, "maximum": 5000},
                        "offset":   {"type": "integer", "minimum": 0}
                    }
                }
            },
            {
                "name": "get_item",
                "description": "Fetch a single item by id, including stock by location, variants, lots, serial numbers, tags.",
                "inputSchema": {
                    "type": "object",
                    "properties": { "id": {"type": "string"} },
                    "required": ["id"]
                }
            },
            {
                "name": "lookup_barcode",
                "description": "Resolve a barcode against the local catalog first, then any configured external SKU provider (UPCitemDB, DigiKey).",
                "inputSchema": {
                    "type": "object",
                    "properties": { "barcode": {"type": "string"} },
                    "required": ["barcode"]
                }
            },
            {
                "name": "stats",
                "description": "Dashboard KPIs: total SKUs, total units, $ on-hand, low/out of stock counts, open POs, open SOs, pending transfers, scheduled counts.",
                "inputSchema": { "type": "object", "properties": {} }
            },
            {
                "name": "list_locations",
                "description": "List physical locations (rack → shelf → bin) with their bin codes.",
                "inputSchema": { "type": "object", "properties": {} }
            },
            {
                "name": "list_suppliers",
                "description": "List vendors with lead time, rating, open-PO count, total spend.",
                "inputSchema": { "type": "object", "properties": {} }
            },
            {
                "name": "recent_activity",
                "description": "Most recent entries from the tamper-evident audit log. Use limit to control how many.",
                "inputSchema": {
                    "type": "object",
                    "properties": { "limit": {"type": "integer", "minimum": 1, "maximum": 5000} }
                }
            },
            {
                "name": "verify_chain",
                "description": "Re-run the SHA-256 audit chain check. Returns valid/invalid plus the head hash for external pinning.",
                "inputSchema": { "type": "object", "properties": {} }
            },
            {
                "name": "forecast_reorder",
                "description": "What needs reordering: items below their min_qty, with avg daily consumption, days_to_stockout, and per-supplier draft POs.",
                "inputSchema": {
                    "type": "object",
                    "properties": { "lookback": {"type": "integer", "minimum": 7, "maximum": 365} }
                }
            }
        ]
    })
}

async fn dispatch_tool(client: &Client, name: &str, args: &Value) -> Result<Value, String> {
    let path = match name {
        "list_items" => {
            let mut p = "/api/items?".to_string();
            if let Some(v) = args.get("category").and_then(|v| v.as_str()) {
                p.push_str(&format!("category={}&", urlencoding::encode(v)));
            }
            if let Some(v) = args.get("supplier").and_then(|v| v.as_str()) {
                p.push_str(&format!("supplier={}&", urlencoding::encode(v)));
            }
            if let Some(v) = args.get("q").and_then(|v| v.as_str()) {
                p.push_str(&format!("q={}&", urlencoding::encode(v)));
            }
            if let Some(v) = args.get("limit").and_then(|v| v.as_i64()) {
                p.push_str(&format!("limit={}&", v));
            }
            if let Some(v) = args.get("offset").and_then(|v| v.as_i64()) {
                p.push_str(&format!("offset={}&", v));
            }
            p
        }
        "get_item" => {
            let id = args
                .get("id")
                .and_then(|v| v.as_str())
                .ok_or_else(|| "id is required".to_string())?;
            format!("/api/items/{}", urlencoding::encode(id))
        }
        "lookup_barcode" => {
            let bc = args
                .get("barcode")
                .and_then(|v| v.as_str())
                .ok_or_else(|| "barcode is required".to_string())?;
            format!("/api/lookup/{}", urlencoding::encode(bc))
        }
        "stats" => "/api/stats".to_string(),
        "list_locations" => "/api/locations".to_string(),
        "list_suppliers" => "/api/suppliers".to_string(),
        "recent_activity" => {
            let lim = args.get("limit").and_then(|v| v.as_i64()).unwrap_or(50);
            format!("/api/activity?limit={}", lim)
        }
        "verify_chain" => "/api/activity/verify".to_string(),
        "forecast_reorder" => {
            let lb = args.get("lookback").and_then(|v| v.as_i64()).unwrap_or(90);
            format!("/api/forecast/reorder?lookback={}", lb)
        }
        other => return Err(format!("unknown tool: {other}")),
    };
    client.get(&path).await
}

fn tool_result(value: Value) -> Value {
    // MCP wraps tool output in a content array of text blocks.
    json!({
        "content": [
            { "type": "text", "text": serde_json::to_string_pretty(&value).unwrap_or_default() }
        ]
    })
}

async fn handle_request(client: &Client, req: JsonRpcRequest) -> JsonRpcResponse {
    let result = match req.method.as_str() {
        "initialize" => Ok(json!({
            "protocolVersion": PROTOCOL_VERSION,
            "capabilities": { "tools": {} },
            "serverInfo": {
                "name": SERVER_NAME,
                "version": SERVER_VERSION,
            },
        })),
        "tools/list" => Ok(tools_descriptor()),
        "tools/call" => {
            let name = req
                .params
                .get("name")
                .and_then(|v| v.as_str())
                .unwrap_or("");
            let args = req
                .params
                .get("arguments")
                .cloned()
                .unwrap_or_else(|| json!({}));
            match dispatch_tool(client, name, &args).await {
                Ok(v) => Ok(tool_result(v)),
                Err(e) => Err(JsonRpcError {
                    code: -32000,
                    message: e,
                    data: None,
                }),
            }
        }
        // Notifications carry no id; caller does not expect a reply.
        // The dispatcher in main filters those out before reaching
        // here, but we accept ping just in case.
        "ping" => Ok(json!({})),
        other => Err(JsonRpcError {
            code: -32601,
            message: format!("method not found: {other}"),
            data: None,
        }),
    };
    match result {
        Ok(v) => JsonRpcResponse {
            jsonrpc: "2.0",
            id: req.id,
            result: Some(v),
            error: None,
        },
        Err(e) => JsonRpcResponse {
            jsonrpc: "2.0",
            id: req.id,
            result: None,
            error: Some(e),
        },
    }
}

#[tokio::main(flavor = "current_thread")]
async fn main() -> std::io::Result<()> {
    eprintln!(
        "racklog-mcp {} talking to {}",
        SERVER_VERSION,
        std::env::var("RACKLOG_URL").as_deref().unwrap_or("http://127.0.0.1:8080")
    );

    let client = Client::from_env();
    let stdin = std::io::stdin();
    let stdout = std::io::stdout();
    let mut out = stdout.lock();

    for line in stdin.lock().lines() {
        let line = line?;
        if line.trim().is_empty() {
            continue;
        }
        // Parse and dispatch. Any parse error yields a JSON-RPC
        // error response without an id — the spec allows this for
        // unparseable input.
        let req: JsonRpcRequest = match serde_json::from_str(&line) {
            Ok(r) => r,
            Err(e) => {
                let resp = JsonRpcResponse {
                    jsonrpc: "2.0",
                    id: None,
                    result: None,
                    error: Some(JsonRpcError {
                        code: -32700,
                        message: format!("parse error: {e}"),
                        data: None,
                    }),
                };
                writeln!(out, "{}", serde_json::to_string(&resp)?)?;
                out.flush()?;
                continue;
            }
        };

        // Notifications (no id) get no response.
        if req.id.is_none() && req.method.starts_with("notifications/") {
            continue;
        }

        let resp = handle_request(&client, req).await;
        writeln!(out, "{}", serde_json::to_string(&resp)?)?;
        out.flush()?;
    }
    Ok(())
}
