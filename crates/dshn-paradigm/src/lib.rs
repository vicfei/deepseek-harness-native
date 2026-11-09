//! dshn-paradigm — the context paradigm ([arXiv 2608.25512](https://arxiv.org/abs/2608.25512)) in Rust.
//!
//! Faithful mechanisms, in the paper's own terms:
//!
//! - **Revertible effects** (temporal composability): every context
//!   transformation carries an inverse that the runtime holds
//!   ([`Ctx::on_dispose`]); removing a component applies its inverses in
//!   reverse registration order, completely reverting its side effects.
//! - **Reactive coeffects** (spatial composability): a component declares
//!   which context slices (signals) it consumes ([`Spec`]); every signal
//!   change is classified against those specs to drive the component's
//!   deactivation (inverses run) and re-activation (apply re-runs with new
//!   values). Unaffected components are untouched — the observational
//!   equivalence the paper's metatheory carries from one component to a
//!   whole interleaved system.
//! - **One unified context** ([`Ctx`]): a scope tree mediating services,
//!   effects and signals; components never touch each other directly.
//! - **Declarative loader** ([`Kernel::reconcile`]): a cordis.yml-shaped
//!   entry list reconciled by diff — mount new, unmount gone, reconfigure
//!   changed (a config change is a coeffect change; reconciliation IS
//!   spatial composability applied to configuration, HMR in miniature).
//!
//! The two theorems are executable here as property tests (`tests/`):
//! unmounting restores the exact pre-mount state; interleaved components do
//! not disturb one another; random operation sequences always equal the
//! reference fold of active components.

mod ctx;
mod kernel;
mod signal;

pub use ctx::Ctx;
pub use kernel::{Contribution, Kernel, KernelEvent, Spec};
pub use signal::{Entry, SignalKey, SignalValue};
