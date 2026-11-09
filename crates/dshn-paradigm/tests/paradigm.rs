//! The paper's two theorems, executable.

use std::sync::{Arc, Mutex};

use dshn_paradigm::{Ctx, Entry, Kernel, Spec};
use serde_json::{json, Value};

#[derive(Default)]
struct World {
    inverses_ran: Mutex<Vec<String>>,
}

#[allow(clippy::type_complexity)]
fn contributor(
    world: Arc<World>,
    id: String,
    value: Value,
) -> Box<dyn Fn(&Ctx, &Value) -> Result<(), String> + Send> {
    Box::new(move |ctx: &Ctx, config: &Value| {
        let v = config.get("value").cloned().unwrap_or(value.clone());
        ctx.provide(dshn_paradigm::Contribution {
            id: id.to_string(),
            value: v.clone(),
        });
        let w = world.clone();
        let tag = format!("{id}:{v}");
        ctx.on_dispose(Box::new(move || w.inverses_ran.lock().unwrap().push(tag)));
        Ok(())
    })
}

/// TEMPORAL COMPOSABILITY: unmounting a component completely reverts its
/// side effects — the kernel state digest equals the pre-mount digest, and
/// inverses ran (our proxy for "every context transformation carried its
/// inverse": the runtime held them until disposal).
#[test]
fn temporal_unmount_reverts_everything() {
    let world = Arc::new(World::default());
    let kernel = Kernel::new();
    kernel.set_signal("mode", json!("a"));
    let before = kernel.state_digest();

    kernel
        .mount(
            Spec {
                id: "comp".into(),
                signals: vec!["mode".into()],
            },
            contributor(world.clone(), "comp".into(), json!(1)),
            json!({"value": 7}),
        )
        .unwrap();
    assert_eq!(kernel.contributions().len(), 1);
    assert!(kernel.unmount("comp"));

    assert_eq!(
        kernel.state_digest(),
        before,
        "unmount must restore the exact pre-mount state"
    );
    assert_eq!(
        *world.inverses_ran.lock().unwrap(),
        vec!["comp:7"],
        "the runtime-held inverse ran on unmount"
    );
}

/// SPATIAL COMPOSABILITY: a signal change re-drives only the components
/// whose spec consumes it; the other component is observationally untouched.
#[test]
fn spatial_change_disturbs_only_consumers() {
    let world = Arc::new(World::default());
    let kernel = Kernel::new();
    kernel
        .mount(
            Spec {
                id: "a".into(),
                signals: vec!["sig-a".into()],
            },
            contributor(world.clone(), "a".into(), json!(0)),
            json!({"value": 10}),
        )
        .unwrap();
    kernel
        .mount(
            Spec {
                id: "b".into(),
                signals: vec!["sig-b".into()],
            },
            contributor(world.clone(), "b".into(), json!(0)),
            json!({"value": 20}),
        )
        .unwrap();

    let events_before = kernel.events().len();
    kernel.set_signal("sig-b", json!(1));

    // b was deactivated (inverse ran) and re-activated with the new snapshot;
    // a's contribution is unchanged and NO event touched a.
    assert_eq!(
        *world.inverses_ran.lock().unwrap(),
        vec!["b:20"],
        "only b's inverse ran"
    );
    let a_contrib = kernel
        .contributions()
        .into_iter()
        .find(|c| c.id == "a")
        .unwrap();
    assert_eq!(
        a_contrib.value,
        json!(10),
        "a is observationally undisturbed"
    );
    let touched_a = kernel.events()[events_before..]
        .iter()
        .any(|e| matches!(e, dshn_paradigm::KernelEvent::Reactivated { id } if id == "a"));
    assert!(!touched_a, "a must not be re-driven by b's signal change");
}

/// METATHEORY IN MINIATURE: random operation sequences over a small world —
/// the live state must always equal the fold of active components (the
/// reference model), and unmount must always restore the prior state.
#[test]
fn random_sequences_match_reference_model() {
    let mut rng = 0x5eed_1234_u64;
    let mut next = move || {
        rng ^= rng << 13;
        rng ^= rng >> 7;
        rng ^= rng << 17;
        rng
    };
    let world = Arc::new(World::default());
    let kernel = Kernel::new();
    let mut reference: std::collections::BTreeMap<String, i64> = Default::default();

    for step in 0..500 {
        match next() % 4 {
            0 | 1 => {
                let id = format!("c{}", next() % 4);
                let value = (next() % 100) as i64;
                let mounted = kernel
                    .mount(
                        Spec {
                            id: id.clone(),
                            signals: vec![format!("s{}", next() % 3)],
                        },
                        contributor(world.clone(), id.clone(), json!(0)),
                        json!({ "value": value }),
                    )
                    .is_ok();
                if mounted {
                    reference.insert(id, value);
                }
            }
            2 => {
                let id = format!("c{}", next() % 4);
                let signals_before = kernel.state_digest().0;
                if kernel.unmount(&id) {
                    reference.remove(&id);
                    let (signals_after, ids_after) = kernel.state_digest();
                    assert_eq!(
                        signals_after, signals_before,
                        "step {step}: unmount must not disturb signals"
                    );
                    assert!(
                        !ids_after.contains(&id),
                        "step {step}: unmounted component must be gone"
                    );
                }
            }
            _ => {
                let key = format!("s{}", next() % 3);
                kernel.set_signal(&key, json!(next() % 5));
            }
        }
        // live state == fold of active components (reference model)
        let live: std::collections::BTreeMap<String, i64> = kernel
            .contributions()
            .into_iter()
            .map(|c| (c.id, c.value.as_i64().unwrap_or(0)))
            .collect();
        let expected: std::collections::BTreeMap<String, i64> =
            reference.clone().into_iter().collect();
        assert_eq!(
            live, expected,
            "step {step}: live state must equal the reference fold"
        );
    }
}

/// Declarative reconciliation diff: unmount gone, mount new, re-drive changed.
#[test]
fn reconcile_diffs_a_cordis_shaped_list() {
    let world = Arc::new(World::default());
    let kernel = Kernel::new();
    // seed one component directly (loader factories arrive with the P3 layer)
    kernel
        .mount(
            Spec {
                id: "keep".into(),
                signals: vec!["config:keep".into()],
            },
            contributor(world.clone(), "keep".into(), json!(0)),
            json!({"value": 1}),
        )
        .unwrap();

    let entries: Vec<Entry> = serde_yaml::from_str(
        "- id: keep\n  name: demo\n  config: {value: 2}\n- id: fresh\n  name: demo\n  config: {value: 9}\n",
    )
    .unwrap();
    // reconcile mounts `fresh` via a factory — which the paradigm crate
    // defers to the harness layer, so we assert the classification itself:
    // `keep` config changed -> re-driven; nothing else errors silently.
    let err = kernel.reconcile(&entries).unwrap_err();
    assert!(
        err.contains("factories live in the harness kernel"),
        "{err}"
    );
    // `keep` was re-driven by its config change before the factory error:
    assert!(kernel
        .events()
        .iter()
        .any(|e| matches!(e, dshn_paradigm::KernelEvent::Reactivated { id } if id == "keep")));
}
