//! The kernel: components with coeffect specs, signal change classification,
//! and declarative reconciliation. This is where the paper's two dimensions
//! meet — reconfiguration deactivates (inverses run) then re-activates
//! (apply re-runs), and unrelated components are provably untouched.

use std::{collections::BTreeMap, sync::Mutex};

use serde_json::Value;

use crate::{ctx::Ctx, signal::Entry, SignalKey, SignalValue};

/// What a component consumes: the signal keys whose changes re-drive it.
#[derive(Debug, Clone, PartialEq)]
pub struct Spec {
    pub id: String,
    pub signals: Vec<SignalKey>,
}

pub type ApplyFn = Box<dyn Fn(&Ctx, &SignalValue) -> Result<(), String> + Send>;

struct Component {
    spec: Spec,
    apply: ApplyFn,
    ctx: Ctx,
    config: SignalValue,
}

#[derive(Debug, Clone, PartialEq)]
pub enum KernelEvent {
    Mounted { id: String },
    Unmounted { id: String },
    Reactivated { id: String },
    SignalSet { key: String },
    Rejected { id: String, reason: String },
}

/// A typed contribution service — how the theorem tests observe state.
#[derive(Debug, Clone, PartialEq, serde::Serialize)]
pub struct Contribution {
    pub id: String,
    pub value: Value,
}

pub struct Kernel {
    root: Ctx,
    signals: Mutex<BTreeMap<SignalKey, SignalValue>>,
    components: Mutex<BTreeMap<String, Component>>,
    events: Mutex<Vec<KernelEvent>>,
}

impl Kernel {
    pub fn new() -> Self {
        Kernel {
            root: Ctx::new_root(),
            signals: Mutex::new(BTreeMap::new()),
            components: Mutex::new(BTreeMap::new()),
            events: Mutex::new(Vec::new()),
        }
    }

    pub fn root(&self) -> &Ctx {
        &self.root
    }

    pub fn set_signal(&self, key: &str, value: SignalValue) {
        self.signals.lock().unwrap().insert(key.into(), value);
        self.events
            .lock()
            .unwrap()
            .push(KernelEvent::SignalSet { key: key.into() });
        // classify the change against every spec; re-drive only the affected
        let affected: Vec<String> = self
            .components
            .lock()
            .unwrap()
            .values()
            .filter(|c| c.spec.signals.iter().any(|s| s == key))
            .map(|c| c.spec.id.clone())
            .collect();
        for id in affected {
            let _ = self.reactivate(&id);
        }
    }

    pub fn signal(&self, key: &str) -> Option<SignalValue> {
        self.signals.lock().unwrap().get(key).cloned()
    }

    /// Activate a component under a fresh child scope: apply runs with the
    /// current signal snapshot and its own config; every effect it registers
    /// lives in that scope and is reverted by unmount/reactivate.
    pub fn mount(&self, spec: Spec, apply: ApplyFn, config: SignalValue) -> Result<(), String> {
        {
            let components = self.components.lock().unwrap();
            if components.contains_key(&spec.id) {
                return Err(format!("component {} already mounted", spec.id));
            }
        }
        let ctx = self.root.child();
        let snapshot = self.signals.lock().unwrap().clone();
        for (k, v) in &snapshot {
            ctx.provide(SignalCell {
                key: k.clone(),
                value: v.clone(),
            });
        }
        if let Err(reason) = apply(&ctx, &config) {
            ctx.dispose();
            self.events.lock().unwrap().push(KernelEvent::Rejected {
                id: spec.id.clone(),
                reason: reason.clone(),
            });
            return Err(reason);
        }
        let mounted_id = spec.id.clone();
        self.components.lock().unwrap().insert(
            mounted_id.clone(),
            Component {
                spec,
                apply,
                ctx,
                config,
            },
        );
        self.events
            .lock()
            .unwrap()
            .push(KernelEvent::Mounted { id: mounted_id });
        Ok(())
    }

    /// Complete revert: the component's scope applies every inverse it holds.
    pub fn unmount(&self, id: &str) -> bool {
        let Some(component) = self.components.lock().unwrap().remove(id) else {
            return false;
        };
        component.ctx.dispose();
        self.events
            .lock()
            .unwrap()
            .push(KernelEvent::Unmounted { id: id.into() });
        true
    }

    fn reactivate(&self, id: &str) -> Result<(), String> {
        let Some(component) = self.components.lock().unwrap().remove(id) else {
            return Err(format!("component {id} not mounted"));
        };
        component.ctx.dispose(); // temporal: revert its effects first
        let spec = component.spec;
        let config = component.config;
        self.mount(spec, component.apply, config)?;
        self.events
            .lock()
            .unwrap()
            .push(KernelEvent::Reactivated { id: id.into() });
        Ok(())
    }

    /// Declarative reconciliation over a cordis.yml-shaped entry list:
    /// mount new ids, unmount gone ids, re-drive ids whose config changed.
    pub fn reconcile(&self, entries: &[Entry]) -> Result<(), String> {
        let desired: BTreeMap<String, Value> = entries
            .iter()
            .map(|e| (e.id.clone(), e.config.clone()))
            .collect();
        let current: BTreeMap<String, Value> = self
            .components
            .lock()
            .unwrap()
            .iter()
            .map(|(id, c)| (id.clone(), c.config.clone()))
            .collect();
        for id in current.keys() {
            if !desired.contains_key(id) {
                self.unmount(id);
            }
        }
        let mut first_error: Option<String> = None;
        for (id, config) in &desired {
            match current.get(id) {
                None => {
                    let entry = entries.iter().find(|e| &e.id == id).expect("desired entry");
                    match self.take_factory(entry).and_then(|apply| {
                        self.mount(
                            Spec {
                                id: id.clone(),
                                signals: vec![format!("config:{id}")],
                            },
                            apply,
                            config.clone(),
                        )
                    }) {
                        Err(e) if first_error.is_none() => first_error = Some(e),
                        _ => {}
                    }
                }
                Some(old) if old != config => {
                    self.set_signal(&format!("config:{id}"), config.clone());
                }
                _ => {}
            }
        }
        match first_error {
            Some(e) => Err(e),
            None => Ok(()),
        }
    }

    fn take_factory(&self, _entry: &Entry) -> Result<ApplyFn, String> {
        // Component code is native Rust; the loader resolves names to
        // registered factories in the harness-kernel layer (P3). For the
        // paradigm crate, tests mount components directly.
        Err("loader factories live in the harness kernel layer (P3)".into())
    }

    /// State digest used by the theorem tests: signals + the component set.
    pub fn state_digest(&self) -> (BTreeMap<String, Value>, Vec<String>) {
        let signals = self.signals.lock().unwrap().clone();
        let ids = self.components.lock().unwrap().keys().cloned().collect();
        (signals, ids)
    }

    /// Contributions visible from live component scopes, innermost-wins.
    pub fn contributions(&self) -> Vec<Contribution> {
        self.components
            .lock()
            .unwrap()
            .values()
            .filter_map(|c| c.ctx.get::<Contribution>())
            .collect()
    }

    pub fn events(&self) -> Vec<KernelEvent> {
        self.events.lock().unwrap().clone()
    }
}

/// A single visible signal cell, provided into component scopes. Read by
/// the harness-kernel layer (P3) via `Ctx::get::<SignalCell>`.
#[allow(dead_code)]
#[derive(Clone)]
pub struct SignalCell {
    pub key: SignalKey,
    pub value: SignalValue,
}

impl Default for Kernel {
    fn default() -> Self {
        Self::new()
    }
}
