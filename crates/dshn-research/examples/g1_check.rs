//! G1 校验入口（胶水演示 `g1_validate.py` 的 Rust 雏形）。
//!
//! ```sh
//! cargo run -p dshn-research --example g1_check -- tests/fixtures/H-2026-001.yaml
//! ```
//!
//! 退出码：0 = G1 PASS，1 = FAIL（含解析失败），2 = 用法错误。

use std::{env, process::ExitCode};

use dshn_research::ResearchTicket;

fn main() -> ExitCode {
    let Some(path) = env::args().nth(1) else {
        eprintln!("usage: g1_check <ticket.yaml>");
        return ExitCode::from(2);
    };
    let ticket = match ResearchTicket::from_path(&path) {
        Ok(t) => t,
        Err(e) => {
            eprintln!("G1 FAIL · {path}（无法解析）\n  ✗ {e}");
            return ExitCode::FAILURE;
        }
    };
    let report = ticket.validate_g1();
    println!("{report}");
    if report.passed {
        ExitCode::SUCCESS
    } else {
        ExitCode::FAILURE
    }
}
