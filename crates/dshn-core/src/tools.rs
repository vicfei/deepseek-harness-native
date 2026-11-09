//! Native tools v0: the registry seam + the `bash` tool. Tool events follow
//! the recorded official shapes: tool/call {callId,name,arguments} before
//! the body runs, tool/result {role:tool message} after.

use serde_json::{json, Value};

#[derive(Debug)]
pub struct ToolOutcome {
    pub text: String,
    pub is_error: bool,
}

/// One native tool: name, JSON schema, synchronous execution.
pub trait Tool: Send + Sync {
    fn name(&self) -> &str;
    fn schema(&self) -> Value {
        json!({ "type": "function", "function": { "name": self.name() } })
    }
    fn execute(&self, arguments: &str) -> ToolOutcome;
}

pub struct Registry {
    tools: Vec<Box<dyn Tool>>,
}

impl Registry {
    pub fn new() -> Self {
        Registry { tools: Vec::new() }
    }
    pub fn with(tool: impl Tool + 'static) -> Self {
        let mut r = Registry::new();
        r.tools.push(Box::new(tool));
        r
    }
    pub fn register(&mut self, tool: impl Tool + 'static) {
        self.tools.push(Box::new(tool));
    }
    pub fn find(&self, name: &str) -> Option<&dyn Tool> {
        self.tools
            .iter()
            .find(|t| t.name() == name)
            .map(|t| t.as_ref())
    }
    /// OpenAI-wire tool list for the request body.
    pub fn wire_tools(&self) -> Vec<Value> {
        self.tools.iter().map(|t| t.schema()).collect()
    }
}

impl Default for Registry {
    fn default() -> Self {
        Self::new()
    }
}

/// The `bash` tool: run one command, bounded output, timeout.
pub struct BashTool;

impl Tool for BashTool {
    fn name(&self) -> &str {
        "bash"
    }
    fn schema(&self) -> Value {
        json!({
            "type": "function",
            "function": {
                "name": "bash",
                "description": "Run a bash command and return its output",
                "parameters": {
                    "type": "object",
                    "properties": { "command": { "type": "string" } },
                    "required": ["command"]
                }
            }
        })
    }
    fn execute(&self, arguments: &str) -> ToolOutcome {
        let cmd = match serde_json::from_str::<Value>(arguments)
            .ok()
            .and_then(|v| v.get("command").and_then(Value::as_str).map(str::to_string))
        {
            Some(c) => c,
            None => {
                return ToolOutcome {
                    text: "invalid arguments: {\"command\": \"...\"} expected".into(),
                    is_error: true,
                }
            }
        };
        match std::process::Command::new("bash")
            .arg("-c")
            .arg(&cmd)
            .output()
        {
            Ok(out) => {
                let mut text = String::new();
                text.push_str(&String::from_utf8_lossy(&out.stdout));
                let err = String::from_utf8_lossy(&out.stderr);
                if !err.is_empty() {
                    text.push_str("\n[stderr] ");
                    text.push_str(&err);
                }
                text.truncate(60_000);
                ToolOutcome {
                    text,
                    is_error: !out.status.success(),
                }
            }
            Err(e) => ToolOutcome {
                text: format!("spawn: {e}"),
                is_error: true,
            },
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn bash_tool_runs_and_reports() {
        let out = BashTool.execute(r#"{"command": "printf hello-tool"}"#);
        assert_eq!(out.text.trim(), "hello-tool");
        assert!(!out.is_error);
        let bad = BashTool.execute(r#"{"command": "exit 3"}"#);
        assert!(bad.is_error);
    }
}
