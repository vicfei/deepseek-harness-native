//! dshn-research — 因子鉴真的研究工单协议内核。
//!
//! 研究工单（[`ResearchTicket`]）是因子假设进入统计检验之前的机器可校验
//! 准入单元：假设、证据、数据时点、检验计划、证伪条件、版本冻结六组要素
//! 一次钉死。胶水演示（`~/factor-auth-demo`，7 步闭环）里的手工 YAML 由
//! 本 crate 固化为 Rust 类型：
//!
//! - [`ticket`]：serde schema + YAML 读写。结构存在性（缺字段、类型错、
//!   非法枚举值）在反序列化时失败；封闭词表（方向、立场）枚举化，开放
//!   词表（source_type、校正方法）保持字符串。
//! - [`g1`]：G1 完整性门控——六组要素的语义完整性（非空白、非空、锚点
//!   与时点逐条检查），产出结构化判决 [`G1Report`](g1::G1Report)。
//!
//! 路线图（issue #15 系列）：PR-2 G2 数据因果门控（PIT 注册表 + 字段
//! 匹配）、PR-3 G3 统计检验、PR-4 证伪反例库（JSONL + 相似拦截），
//! 均落在本 crate。

pub mod g1;
pub mod ticket;

pub use g1::{G1Report, Violation};
pub use ticket::{
    DataField, Direction, Evidence, ResearchTicket, Responsibility, Stance, TestPlan, TicketError,
    VersionPin,
};
