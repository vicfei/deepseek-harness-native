//! 研究工单（research ticket）——因子研究的机器可校验准入单元。
//!
//! 一张工单把一个因子假设的六组要素钉死在提交时刻：假设陈述（含经济学逻辑与
//! 预期方向）、文献证据（每条必须带原文锚点）、数据字段（每个必须声明 PIT
//! 可用时点）、检验计划（三段式期间 + 显著性规则 + 多重检验登记）、证伪条件、
//! 版本冻结（数据快照 / 代码版本 / 随机种子）。后续门控都读这张工单：
//! G2 拿 `data_fields[].available_at` 对照因子代码的实际取数时点判因果，
//! G3 拿 `test_plan` 的冻结测试集与显著性规则出判决，证伪归档拿
//! `falsification_conditions` 与 `hypothesis_id` 回填反例库。
//!
//! Schema 的 YAML 形态与胶水演示 (`~/factor-auth-demo/tickets/`) 完全一致，
//! 两张演示工单以 fixture 形式进入本 crate 的测试。

use std::fmt;
use std::path::Path;

use serde::{Deserialize, Serialize};

/// 一张研究工单。字段整体分两层校验：
/// 结构存在性由 serde 承担（缺字段 = 反序列化错误，路径精确），
/// 语义完整性由 [`crate::g1`] 承担（非空、非空白、枚举合法之外的规则）。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ResearchTicket {
    /// 假设编号，全生命周期主键（工单、反例库、入池记录共用）。
    pub hypothesis_id: String,
    /// 经济学逻辑：为什么这个效应应该存在。自由文本，G1 只查非空白。
    pub economic_logic: String,
    /// 预期因子方向，G3 判「方向反转证伪」的依据。
    pub expected_direction: Direction,
    /// 市场与股票池，如「A股 · 中证800」。
    pub market: String,
    /// 调仓周期，如「月度调仓」。
    pub holding_period: String,
    /// 文献证据，至少一条，每条必须有原文锚点。
    pub evidence: Vec<Evidence>,
    /// 数据字段声明，至少一个，每个必须声明可用时点。
    pub data_fields: Vec<DataField>,
    /// 检验计划：冻结的三段式期间与多重检验纪律。
    pub test_plan: TestPlan,
    /// 事前登记的证伪条件：满足任何一条即判死，进反例库。
    pub falsification_conditions: Vec<String>,
    /// 版本冻结：复现的最小充分集。
    pub version: VersionPin,
    /// 责任人。演示阶段允许留空，G1 不把它当准入条件。
    #[serde(default)]
    pub responsibility: Option<Responsibility>,
}

/// 因子预期方向。封闭词表：未知值在反序列化时即失败，不会溜进门控。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Direction {
    Positive,
    Negative,
}

/// 单条证据对假设的立场。G1 报告按它统计支持/反对票。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Stance {
    Supporting,
    Contradicting,
}

/// 一条文献证据。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Evidence {
    /// 开放词表：研报 / 论文 / 公告 / …。不枚举化，保持前向兼容。
    pub source_type: String,
    /// 出处引用（作者·机构·题名·年份）。
    pub citation: String,
    /// 原文锚点（页码·表号·结论句）。证据必须可定位，G1 强制非空白。
    pub anchor: String,
    pub stance: Stance,
}

/// 工单声明的数据字段。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct DataField {
    pub name: String,
    pub definition: String,
    /// PIT 可用时点声明（如「公告日期 + 1 交易日」）。G2 判数据因果的锚点。
    pub available_at: String,
}

/// 检验计划：三段式期间分离 + 显著性纪律。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct TestPlan {
    /// 开发期（因子构造与迭代）。
    pub dev_period: String,
    /// 选择期（候选间比较）。
    pub selection_period: String,
    /// 冻结测试期（终审，只许跑一次）。
    pub final_test_period: String,
    /// 显著性规则的完整表述，如「月度 RankIC 双侧 p < 0.05（Bonferroni 校正）」。
    pub significance_rule: String,
    /// 事前登记的最大尝试次数：多重检验校正的分母之一。
    pub max_attempts: u32,
    /// 校正方法名。PR-3 (G3) 之前保持开放词表。
    pub multiple_testing_method: String,
}

/// 版本冻结。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct VersionPin {
    /// PIT 数据快照标识（如 `pit_v2026_11_08`）。
    pub data_snapshot: String,
    /// 因子代码版本。允许 `TBD` 占位（沙箱生成后登记），故可缺省。
    #[serde(default)]
    pub code_version: Option<String>,
    /// 随机种子。
    pub random_seed: u64,
}

/// 责任人登记。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Responsibility {
    #[serde(default)]
    pub researcher: Option<String>,
    #[serde(default)]
    pub reviewer: Option<String>,
}

/// 工单读写错误：I/O 或 YAML 结构（缺字段 / 类型错 / 非法枚举值）。
#[derive(Debug)]
pub enum TicketError {
    Io(std::io::Error),
    /// 反序列化失败。 serde 错误文本已含精确字段路径。
    Yaml(String),
}

impl fmt::Display for TicketError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            TicketError::Io(e) => write!(f, "io error: {e}"),
            TicketError::Yaml(s) => write!(f, "ticket yaml: {s}"),
        }
    }
}

impl std::error::Error for TicketError {}

impl From<std::io::Error> for TicketError {
    fn from(e: std::io::Error) -> Self {
        TicketError::Io(e)
    }
}

impl ResearchTicket {
    /// 从 YAML 文本解析。缺必填字段、类型不符、枚举值非法都在这里失败。
    pub fn from_yaml_str(s: &str) -> Result<Self, TicketError> {
        serde_yaml_ng::from_str(s).map_err(|e| TicketError::Yaml(e.to_string()))
    }

    /// 从 YAML 文件解析。
    pub fn from_path(path: impl AsRef<Path>) -> Result<Self, TicketError> {
        let raw = std::fs::read_to_string(path)?;
        Self::from_yaml_str(&raw)
    }

    /// 序列化回 YAML（round-trip 测试与工具链写盘用）。
    pub fn to_yaml_string(&self) -> Result<String, TicketError> {
        serde_yaml_ng::to_string(self).map_err(|e| TicketError::Yaml(e.to_string()))
    }
}
