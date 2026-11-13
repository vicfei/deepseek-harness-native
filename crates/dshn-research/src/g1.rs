//! G1 —— 研究工单完整性校验（准入第一道门）。
//!
//! 校验分层：**结构存在性由 serde 承担**——`ResearchTicket` 的必填字段都是
//! 非 `Option`，缺失或类型不符在 [`ResearchTicket::from_yaml_str`] 即失败；
//! **G1 承担语义完整性**——六组字段非空白、列表非空、逐条锚点/时点非空白、
//! `max_attempts ≥ 1`。这与胶水演示 `g1_validate.py` 的判定集合一致，
//! 只是「缺字段」的检测从运行时字典查找前移到了反序列化。
//!
//! 六组要素：假设陈述 / 文献证据 / 数据字段 / 检验计划 / 证伪条件 / 版本冻结。
//! 判决是结构化的 [`G1Report`]（违规清单 + 证据立场统计），不是布尔值——
//! 演示与后续 PR 要把「为什么被拦」讲给人听。

use std::fmt;

use crate::ticket::{ResearchTicket, Stance};

/// 单条 G1 违规：字段路径 + 人话原因。措辞与胶水演示保持一致，
/// 它们是路演话术的一部分。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Violation {
    /// 字段路径，如 `evidence[1].anchor`。
    pub field: String,
    pub detail: &'static str,
}

impl fmt::Display for Violation {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "✗ {}: {}", self.field, self.detail)
    }
}

/// G1 判决报告。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct G1Report {
    pub hypothesis_id: String,
    pub passed: bool,
    pub violations: Vec<Violation>,
    pub evidence_total: usize,
    pub evidence_supporting: usize,
    pub evidence_contradicting: usize,
    pub data_field_count: usize,
    pub max_attempts: u32,
}

impl G1Report {
    /// 渲染为演示同款的可读报告（多行，无 ANSI 色）。
    pub fn render(&self) -> String {
        let mut out = String::new();
        if !self.passed {
            out.push_str(&format!("G1 FAIL · {}\n", self.hypothesis_id));
            for v in &self.violations {
                out.push_str(&format!("  {v}\n"));
            }
            return out;
        }
        out.push_str(&format!("G1 PASS · {}\n", self.hypothesis_id));
        out.push_str("  ✓ 6 组必填字段完整\n");
        out.push_str(&format!(
            "  ✓ 证据锚点 {} 条（支持 {} / 反对 {}）\n",
            self.evidence_total, self.evidence_supporting, self.evidence_contradicting
        ));
        out.push_str(&format!(
            "  ✓ 数据字段 {} 个，均声明可用时点\n",
            self.data_field_count
        ));
        out.push_str(&format!(
            "  ✓ 检验计划含冻结测试集 + 多重检验登记（max_attempts={}）",
            self.max_attempts
        ));
        out
    }
}

impl fmt::Display for G1Report {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.render())
    }
}

const MISSING_OR_EMPTY: &str = "缺失或为空";
const ANCHOR_REQUIRED: &str = "缺失（证据必须有原文定位）";
const PIVOT_REQUIRED: &str = "缺失（无时点不计算）";
const ATTEMPTS_FLOOR: &str = "必须至少为 1（多重检验校正的分母）";
const CONDITION_BLANK: &str = "为空白（证伪条件必须可判定）";

fn non_blank(s: &str) -> bool {
    !s.trim().is_empty()
}

impl ResearchTicket {
    /// G1 完整性校验。不修改工单；违规逐条列出，全部通过才 `passed`。
    pub fn validate_g1(&self) -> G1Report {
        let mut violations = Vec::new();

        // 组 1：假设陈述（expected_direction 是枚举，存在性已由 serde 保证）。
        for (field, value) in [
            ("hypothesis_id", &self.hypothesis_id),
            ("economic_logic", &self.economic_logic),
            ("market", &self.market),
            ("holding_period", &self.holding_period),
        ] {
            if !non_blank(value) {
                violations.push(Violation {
                    field: field.to_string(),
                    detail: MISSING_OR_EMPTY,
                });
            }
        }

        // 组 2：文献证据非空，且每条有原文锚点。
        if self.evidence.is_empty() {
            violations.push(Violation {
                field: "evidence".to_string(),
                detail: MISSING_OR_EMPTY,
            });
        }
        for (i, e) in self.evidence.iter().enumerate() {
            if !non_blank(&e.anchor) {
                violations.push(Violation {
                    field: format!("evidence[{i}].anchor"),
                    detail: ANCHOR_REQUIRED,
                });
            }
        }

        // 组 3：数据字段非空，且每个声明可用时点（G2 判因果的锚点）。
        if self.data_fields.is_empty() {
            violations.push(Violation {
                field: "data_fields".to_string(),
                detail: MISSING_OR_EMPTY,
            });
        }
        for (i, d) in self.data_fields.iter().enumerate() {
            if !non_blank(&d.available_at) {
                violations.push(Violation {
                    field: format!("data_fields[{i}].available_at"),
                    detail: PIVOT_REQUIRED,
                });
            }
        }

        // 组 4：检验计划（期间/规则/方法非空白，尝试次数下限）。
        for (field, value) in [
            ("test_plan.dev_period", &self.test_plan.dev_period),
            (
                "test_plan.selection_period",
                &self.test_plan.selection_period,
            ),
            (
                "test_plan.final_test_period",
                &self.test_plan.final_test_period,
            ),
            (
                "test_plan.significance_rule",
                &self.test_plan.significance_rule,
            ),
            (
                "test_plan.multiple_testing_method",
                &self.test_plan.multiple_testing_method,
            ),
        ] {
            if !non_blank(value) {
                violations.push(Violation {
                    field: field.to_string(),
                    detail: MISSING_OR_EMPTY,
                });
            }
        }
        if self.test_plan.max_attempts < 1 {
            violations.push(Violation {
                field: "test_plan.max_attempts".to_string(),
                detail: ATTEMPTS_FLOOR,
            });
        }

        // 组 5：证伪条件非空且逐条非空白。
        if self.falsification_conditions.is_empty() {
            violations.push(Violation {
                field: "falsification_conditions".to_string(),
                detail: MISSING_OR_EMPTY,
            });
        }
        for (i, c) in self.falsification_conditions.iter().enumerate() {
            if !non_blank(c) {
                violations.push(Violation {
                    field: format!("falsification_conditions[{i}]"),
                    detail: CONDITION_BLANK,
                });
            }
        }

        // 组 6：版本冻结（random_seed 是 u64，存在性由 serde 保证）。
        if !non_blank(&self.version.data_snapshot) {
            violations.push(Violation {
                field: "version.data_snapshot".to_string(),
                detail: MISSING_OR_EMPTY,
            });
        }

        G1Report {
            hypothesis_id: self.hypothesis_id.clone(),
            passed: violations.is_empty(),
            violations,
            evidence_total: self.evidence.len(),
            evidence_supporting: self
                .evidence
                .iter()
                .filter(|e| e.stance == Stance::Supporting)
                .count(),
            evidence_contradicting: self
                .evidence
                .iter()
                .filter(|e| e.stance == Stance::Contradicting)
                .count(),
            data_field_count: self.data_fields.len(),
            max_attempts: self.test_plan.max_attempts,
        }
    }
}
