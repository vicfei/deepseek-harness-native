//! Native MCP client v0: stdio JSON-RPC 2.0 (newline-delimited), the
//! initialize -> tools/list -> tools/call lifecycle, and a Tool adapter so
//! server tools register into the kernel's Registry under the official
//! `mcp__<server>__<tool>` naming contract.

use std::io::{BufRead, BufReader, Write};
use std::process::{Child, Command, Stdio};

use serde_json::{json, Value};

use crate::tools::{Tool, ToolOutcome};

#[derive(Debug)]
pub enum McpError {
    Io(String),
    Protocol(String),
}

impl std::fmt::Display for McpError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            McpError::Io(m) => write!(f, "mcp io: {m}"),
            McpError::Protocol(m) => write!(f, "mcp protocol: {m}"),
        }
    }
}

/// One running stdio MCP server.
pub struct McpServer {
    pub name: String,
    child: Child,
    next_id: i64,
}

impl McpServer {
    pub fn spawn(
        name: &str,
        command: &str,
        args: &[String],
        env: &Value,
    ) -> Result<Self, McpError> {
        let child = Command::new(command)
            .args(args)
            .envs(
                env.as_object()
                    .map(|m| {
                        m.iter()
                            .filter_map(|(k, v)| v.as_str().map(|v| (k.clone(), v.to_string())))
                            .collect::<Vec<_>>()
                    })
                    .unwrap_or_default(),
            )
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::null())
            .spawn()
            .map_err(|e| McpError::Io(format!("spawn {command}: {e}")))?;
        Ok(McpServer {
            name: name.to_string(),
            child,
            next_id: 0,
        })
    }

    fn request(&mut self, method: &str, params: Value) -> Result<Value, McpError> {
        self.next_id += 1;
        let id = self.next_id;
        let msg = json!({ "jsonrpc": "2.0", "id": id, "method": method, "params": params });
        let mut stdin = self
            .child
            .stdin
            .take()
            .ok_or_else(|| McpError::Io("stdin gone".into()))?;
        stdin
            .write_all(format!("{msg}\n").as_bytes())
            .and_then(|_| stdin.flush())
            .map_err(|e| McpError::Io(e.to_string()))?;
        self.child.stdin.replace(stdin);

        let mut stdout = self
            .child
            .stdout
            .take()
            .ok_or_else(|| McpError::Io("stdout gone".into()))?;
        let mut reader = BufReader::new(&mut stdout);
        loop {
            let mut line = String::new();
            let n = reader
                .read_line(&mut line)
                .map_err(|e| McpError::Io(e.to_string()))?;
            if n == 0 {
                return Err(McpError::Io("server closed stdout".into()));
            }
            let parsed: Value = serde_json::from_str(line.trim())
                .map_err(|e| McpError::Protocol(format!("bad line: {e}")))?;
            // server notifications (no id) are skipped; we match our id
            if parsed.get("id").and_then(Value::as_i64) == Some(id) {
                self.child.stdout.replace(stdout);
                if let Some(err) = parsed.get("error") {
                    return Err(McpError::Protocol(err.to_string()));
                }
                return Ok(parsed["result"].clone());
            }
        }
    }

    fn notify(&mut self, method: &str) -> Result<(), McpError> {
        let msg = json!({ "jsonrpc": "2.0", "method": method });
        if let Some(stdin) = self.child.stdin.as_mut() {
            stdin
                .write_all(format!("{msg}\n").as_bytes())
                .and_then(|_| stdin.flush())
                .map_err(|e| McpError::Io(e.to_string()))?;
        }
        Ok(())
    }

    pub fn initialize(&mut self) -> Result<(), McpError> {
        self.request(
            "initialize",
            json!({
                "protocolVersion": "2024-11-05",
                "capabilities": {},
                "clientInfo": { "name": "dshn", "version": env!("CARGO_PKG_VERSION") }
            }),
        )?;
        self.notify("notifications/initialized")
    }

    /// tools/list descriptors: [{name, description, inputSchema}]
    pub fn tools_list(&mut self) -> Result<Vec<Value>, McpError> {
        let result = self.request("tools/list", json!({}))?;
        Ok(result
            .get("tools")
            .and_then(Value::as_array)
            .cloned()
            .unwrap_or_default())
    }

    pub fn call_tool(&mut self, tool: &str, arguments: &str) -> Result<String, McpError> {
        let args: Value = serde_json::from_str(arguments).unwrap_or(json!({}));
        let result = self.request("tools/call", json!({ "name": tool, "arguments": args }))?;
        // content blocks -> joined text; isError surfaces as an error string
        let text = result
            .get("content")
            .and_then(Value::as_array)
            .map(|blocks| {
                blocks
                    .iter()
                    .filter_map(|b| b.get("text").and_then(Value::as_str))
                    .collect::<Vec<_>>()
                    .join("\n")
            })
            .unwrap_or_default();
        if result.get("isError").and_then(Value::as_bool) == Some(true) {
            return Err(McpError::Protocol(text));
        }
        Ok(text)
    }
}

impl Drop for McpServer {
    fn drop(&mut self) {
        let _ = self.child.kill();
        let _ = self.child.wait();
    }
}

/// Adapter: one server tool as a kernel Tool, official naming contract.
pub type SharedServer = std::sync::Arc<std::sync::Mutex<McpServer>>;

pub struct McpTool {
    pub wire_name: String, // mcp__<server>__<tool>
    pub server_tool: String,
    pub description: String,
    pub input_schema: Value,
    pub server: SharedServer,
}

impl McpTool {
    pub fn new(server: SharedServer, tool: &Value) -> Self {
        let raw = tool.get("name").and_then(Value::as_str).unwrap_or_default();
        McpTool {
            wire_name: format!("mcp__{}_{}", server.lock().unwrap().name, raw),
            server_tool: raw.to_string(),
            description: tool
                .get("description")
                .and_then(Value::as_str)
                .unwrap_or("")
                .to_string(),
            input_schema: tool
                .get("inputSchema")
                .cloned()
                .unwrap_or(json!({ "type": "object" })),
            server,
        }
    }
}

impl Tool for McpTool {
    fn name(&self) -> &str {
        &self.wire_name
    }
    fn schema(&self) -> Value {
        json!({
            "type": "function",
            "function": {
                "name": self.wire_name,
                "description": self.description,
                "parameters": self.input_schema
            }
        })
    }
    fn execute(&self, arguments: &str) -> ToolOutcome {
        match self
            .server
            .lock()
            .unwrap()
            .call_tool(&self.server_tool, arguments)
        {
            Ok(text) => ToolOutcome {
                text,
                is_error: false,
            },
            Err(e) => ToolOutcome {
                text: e.to_string(),
                is_error: true,
            },
        }
    }
}
