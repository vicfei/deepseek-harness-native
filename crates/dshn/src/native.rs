//! `dshn --native "task"` — the first fully native path: DeepSeek over HTTPS
//! straight into the Rust loop. No Node, no sidecar, no JS anywhere.

use std::path::Path;

use dshn_core::session::Header;
use dshn_core::{
    discover, run_turn, BashTool, DeepSeekProvider, McpServer, McpTool, Registry, SkillsTool,
    TurnInput,
};

pub fn run(
    task: &str,
    home: &Path,
    preset: dshn_core::policy::ApprovalPreset,
) -> Result<String, String> {
    let api_key = std::env::var("DEEPSEEK_API_KEY")
        .map_err(|_| "DEEPSEEK_API_KEY not set (the native path needs it directly)".to_string())?;
    let provider = DeepSeekProvider::new(api_key, "deepseek-flash");

    let input = TurnInput {
        task: task.to_string(),
        provider: "deepseek-official".into(),
        model: provider.model.clone(),
        max_tokens: 8192,
        reasoning_effort: "high".into(),
    };

    let skills = discover(home);
    let skill_entries: Vec<serde_json::Value> = skills
        .iter()
        .map(|s| serde_json::json!({ "name": s.name, "description": s.description }))
        .collect();
    let mut registry = Registry::with(BashTool);
    if !skills.is_empty() {
        registry.register(SkillsTool::new(skills));
    }
    // MCP servers from <home>/mcp.json (v0 config surface, ledgered):
    // {"servers": {"<name>": {"command": "...", "args": [...], "env": {}}}}
    if let Ok(cfg_raw) = std::fs::read_to_string(home.join("mcp.json")) {
        if let Ok(cfg) = serde_json::from_str::<serde_json::Value>(&cfg_raw) {
            if let Some(servers) = cfg.get("servers").and_then(|s| s.as_object()) {
                for (name, spec) in servers {
                    let Some(command) = spec.get("command").and_then(|v| v.as_str()) else {
                        continue;
                    };
                    let args: Vec<String> = spec
                        .get("args")
                        .and_then(|a| a.as_array())
                        .map(|a| {
                            a.iter()
                                .filter_map(|v| v.as_str().map(String::from))
                                .collect()
                        })
                        .unwrap_or_default();
                    let env = spec.get("env").cloned().unwrap_or(serde_json::json!({}));
                    let started = McpServer::spawn(name, command, &args, &env).and_then(|mut s| {
                        s.initialize()?;
                        let tools = s.tools_list()?;
                        Ok((std::sync::Arc::new(std::sync::Mutex::new(s)), tools))
                    });
                    match started {
                        Ok((shared, tools)) => {
                            eprintln!("-- mcp server `{name}`: {} tools", tools.len());
                            for tool in &tools {
                                registry.register(McpTool::new(shared.clone(), tool));
                            }
                        }
                        Err(e) => eprintln!("-- mcp server `{name}` failed: {e}"),
                    }
                }
            }
        }
    }

    let mut t = now_ms();
    let mut seq = -1i64;
    let mut total_chunks = 0usize;
    let mut events = dshn_core::policy::policy_events(
        preset,
        &mut || {
            t += 1;
            t
        },
        &mut seq,
    );
    events.extend(run_turn(
        &input,
        &registry,
        &skill_entries,
        preset,
        &mut |messages| {
            provider
                .stream_chat_messages(messages, &registry.wire_tools(), input.max_tokens, |_| {})
                .inspect(|chunks| total_chunks += chunks.len())
                .map_err(|e| e.to_string())
        },
        &mut || {
            t += 1;
            t
        },
        &mut seq,
    )?);

    // persist the native session log (plain JSONL v0; the zstd container
    // writer arrives with the persistence slice — ledgered)
    let id = format!("session-native-{}", hex_suffix());
    let dir = home.join("sessions-native");
    std::fs::create_dir_all(&dir).map_err(|e| e.to_string())?;
    let mut out = String::new();
    let header = Header {
        kind: "session".into(),
        version: 4,
        id: id.clone(),
        created_at: now_ms(),
        cwd: std::env::current_dir()
            .map(|p| p.display().to_string())
            .unwrap_or_default(),
        is_seeded: false,
        delegation_depth: 0,
    };
    out.push_str(&serde_json::to_string(&header).unwrap());
    out.push('\n');
    for e in &events {
        out.push_str(&serde_json::to_string(&e).unwrap());
        out.push('\n');
    }
    let path = dir.join(format!("{id}.jsonl"));
    std::fs::write(&path, &out).map_err(|e| e.to_string())?;

    let final_text: Option<String> = events.iter().rev().find_map(|e| {
        if e.kind != "assistant/message" {
            return None;
        }
        let text: String = e
            .data
            .pointer("/message/content")
            .and_then(|c| c.as_array())
            .map(|blocks| {
                blocks
                    .iter()
                    .filter(|b| b.get("type").and_then(|t| t.as_str()) == Some("text"))
                    .filter_map(|b| b.get("text").and_then(|t| t.as_str()))
                    .collect()
            })
            .unwrap_or_default();
        (!text.is_empty()).then_some(text)
    });
    eprintln!(
        "-- native session {} · {} events · {} chunks · log {}",
        id,
        events.len(),
        total_chunks,
        path.display()
    );
    final_text.ok_or_else(|| "no assistant text in the stream".to_string())
}

fn now_ms() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_millis() as i64)
        .unwrap_or(0)
}

fn hex_suffix() -> String {
    let mut b = [0u8; 6];
    if let Ok(mut f) = std::fs::File::open("/dev/urandom") {
        use std::io::Read;
        let _ = f.read_exact(&mut b);
    }
    b.iter().map(|x| format!("{x:02x}")).collect()
}
