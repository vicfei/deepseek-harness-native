//! G1 端到端测试：两张胶水演示工单为 fixture。
//!
//! 判定分层在测试里的体现：结构性缺失（少字段、非法枚举值）走
//! `from_yaml_str` 失败路径；语义不完整（空白、空列表、缺锚点/时点）
//! 通过「解析 → 改写结构 → 重序列化 → 重解析 → G1」的变异路径覆盖。

use dshn_research::{Direction, ResearchTicket, Stance};

fn fixture(name: &str) -> String {
    std::fs::read_to_string(format!("tests/fixtures/{name}")).expect("fixture present")
}

fn remutate(t: &ResearchTicket) -> ResearchTicket {
    let yaml = t.to_yaml_string().expect("serialize");
    ResearchTicket::from_yaml_str(&yaml).expect("re-parse")
}

// ─── 结构层：serde 严格性 ─────────────────────────────────────

#[test]
fn parses_demo_ticket_h001_with_typed_fields() {
    let t = ResearchTicket::from_yaml_str(&fixture("H-2026-001.yaml")).expect("parse");
    assert_eq!(t.hypothesis_id, "H-2026-001");
    assert_eq!(t.expected_direction, Direction::Positive);
    assert_eq!(t.evidence.len(), 3);
    assert_eq!(t.evidence[0].stance, Stance::Supporting);
    assert_eq!(t.evidence[2].stance, Stance::Contradicting);
    assert_eq!(t.data_fields.len(), 3);
    assert_eq!(
        t.data_fields[2].name, "analyst_forecast_next_year",
        "G2 演示的涉案字段必须在场"
    );
    assert_eq!(t.test_plan.max_attempts, 20);
    assert_eq!(t.version.random_seed, 20260101);
    assert_eq!(
        t.version.code_version.as_deref(),
        Some("TBD（沙箱生成后登记）")
    );
    assert_eq!(
        t.responsibility.as_ref().and_then(|r| r.reviewer.clone()),
        Some("（待填）".into())
    );
}

#[test]
fn missing_required_field_is_a_deserialization_error() {
    // 删掉 market 行：结构存在性由 serde 承担，不进 G1。
    let broken = fixture("H-2026-001.yaml")
        .lines()
        .filter(|l| !l.starts_with("market:"))
        .collect::<Vec<_>>()
        .join("\n");
    let err = ResearchTicket::from_yaml_str(&broken)
        .expect_err("market is required")
        .to_string();
    assert!(err.contains("market"), "error should name the field: {err}");
}

#[test]
fn unknown_stance_fails_loudly() {
    let broken =
        fixture("H-2026-001.yaml").replacen(r#"stance: "supporting""#, r#"stance: "neutral""#, 1);
    let err = ResearchTicket::from_yaml_str(&broken)
        .expect_err("neutral is not in the vocabulary")
        .to_string();
    assert!(
        err.to_lowercase().contains("stance"),
        "error should name the enum: {err}"
    );
}

#[test]
fn missing_falsification_block_is_a_deserialization_error() {
    let raw = fixture("H-2026-001.yaml");
    let cut = raw
        .find("falsification_conditions:")
        .expect("block present");
    let trimmed = &raw[..cut];
    let err = ResearchTicket::from_yaml_str(trimmed)
        .expect_err("falsification group is required")
        .to_string();
    assert!(err.contains("falsification_conditions"), "got: {err}");
}

#[test]
fn yaml_roundtrip_is_lossless() {
    let t = ResearchTicket::from_yaml_str(&fixture("H-2026-001.yaml")).expect("parse");
    assert_eq!(remutate(&t), t);
}

// ─── 语义层：G1 完整性 ────────────────────────────────────────

#[test]
fn g1_passes_h001_and_reports_demo_counts() {
    let t = ResearchTicket::from_yaml_str(&fixture("H-2026-001.yaml")).expect("parse");
    let r = t.validate_g1();
    assert!(r.passed, "violations: {:?}", r.violations);
    assert_eq!(r.evidence_total, 3);
    assert_eq!(r.evidence_supporting, 2);
    assert_eq!(r.evidence_contradicting, 1);
    assert_eq!(r.data_field_count, 3);
    assert_eq!(r.max_attempts, 20);
    let text = r.render();
    assert!(text.starts_with("G1 PASS · H-2026-001"));
    assert!(text.contains("证据锚点 3 条（支持 2 / 反对 1）"));
    assert!(text.contains("max_attempts=20"));
}

#[test]
fn g1_passes_h005_g1_is_not_the_similarity_gate() {
    // H-2026-005 应通过 G1：拦截它是证伪反例库（PR-4）的职责，不是 G1 的。
    let t = ResearchTicket::from_yaml_str(&fixture("H-2026-005.yaml")).expect("parse");
    let r = t.validate_g1();
    assert!(r.passed, "violations: {:?}", r.violations);
    assert_eq!(r.evidence_supporting, 1);
    assert_eq!(r.evidence_contradicting, 0);
}

#[test]
fn g1_rejects_blank_evidence_anchor() {
    let mut t = ResearchTicket::from_yaml_str(&fixture("H-2026-001.yaml")).expect("parse");
    t.evidence[1].anchor = "   ".into();
    let r = remutate(&t).validate_g1();
    assert!(!r.passed);
    assert_eq!(r.violations.len(), 1);
    assert_eq!(r.violations[0].field, "evidence[1].anchor");
    assert!(r.violations[0].detail.contains("原文定位"));
}

#[test]
fn g1_rejects_empty_evidence_list() {
    let mut t = ResearchTicket::from_yaml_str(&fixture("H-2026-001.yaml")).expect("parse");
    t.evidence.clear();
    let r = remutate(&t).validate_g1();
    assert!(!r.passed);
    assert!(r.violations.iter().any(|v| v.field == "evidence"));
}

#[test]
fn g1_rejects_missing_available_at() {
    let mut t = ResearchTicket::from_yaml_str(&fixture("H-2026-001.yaml")).expect("parse");
    t.data_fields[2].available_at = String::new();
    let r = remutate(&t).validate_g1();
    assert!(!r.passed);
    assert_eq!(r.violations[0].field, "data_fields[2].available_at");
    assert!(r.violations[0].detail.contains("无时点不计算"));
}

#[test]
fn g1_rejects_blank_hypothesis_fields() {
    let mut t = ResearchTicket::from_yaml_str(&fixture("H-2026-001.yaml")).expect("parse");
    t.economic_logic = "  \n ".into();
    t.holding_period = String::new();
    let r = remutate(&t).validate_g1();
    assert!(!r.passed);
    assert!(r.violations.iter().any(|v| v.field == "economic_logic"));
    assert!(r.violations.iter().any(|v| v.field == "holding_period"));
}

#[test]
fn g1_rejects_zero_attempts_and_blank_method() {
    let mut t = ResearchTicket::from_yaml_str(&fixture("H-2026-001.yaml")).expect("parse");
    t.test_plan.max_attempts = 0;
    t.test_plan.multiple_testing_method = "  ".into();
    let r = remutate(&t).validate_g1();
    assert!(!r.passed);
    assert_eq!(
        r.violations
            .iter()
            .filter(|v| v.field.starts_with("test_plan."))
            .count(),
        2
    );
}

#[test]
fn g1_rejects_empty_or_blank_falsification_conditions() {
    let mut t = ResearchTicket::from_yaml_str(&fixture("H-2026-001.yaml")).expect("parse");
    t.falsification_conditions.clear();
    let r = remutate(&t).validate_g1();
    assert!(r
        .violations
        .iter()
        .any(|v| v.field == "falsification_conditions"));

    let mut t2 = ResearchTicket::from_yaml_str(&fixture("H-2026-001.yaml")).expect("parse");
    t2.falsification_conditions = vec!["  ".into()];
    let r2 = remutate(&t2).validate_g1();
    assert!(
        r2.violations
            .iter()
            .any(|v| v.field == "falsification_conditions[0]"),
        "blank condition must be flagged: {:?}",
        r2.violations
    );
}

#[test]
fn g1_rejects_blank_data_snapshot() {
    let mut t = ResearchTicket::from_yaml_str(&fixture("H-2026-001.yaml")).expect("parse");
    t.version.data_snapshot = String::new();
    let r = remutate(&t).validate_g1();
    assert!(r
        .violations
        .iter()
        .any(|v| v.field == "version.data_snapshot"));
}

#[test]
fn g1_failure_report_lists_every_violation() {
    let mut t = ResearchTicket::from_yaml_str(&fixture("H-2026-001.yaml")).expect("parse");
    t.evidence[0].anchor = String::new();
    t.data_fields[1].available_at = String::new();
    t.test_plan.max_attempts = 0;
    let r = remutate(&t).validate_g1();
    assert!(!r.passed);
    assert_eq!(r.violations.len(), 3);
    let text = r.render();
    assert!(text.starts_with("G1 FAIL · H-2026-001"));
    for expected in [
        "evidence[0].anchor",
        "data_fields[1].available_at",
        "test_plan.max_attempts",
    ] {
        assert!(
            text.contains(expected),
            "rendered report should list {expected}:\n{text}"
        );
    }
}
