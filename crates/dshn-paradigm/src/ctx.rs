//! Ctx — one unified context type: a scope tree mediating services and
//! effects. Every effect registers its runtime-held inverse here; disposing a
//! scope applies inverses in reverse registration order (children first).

use std::{
    any::{Any, TypeId},
    collections::HashMap,
    sync::{Arc, Mutex},
};

type Inverse = Box<dyn FnOnce() + Send>;

struct EffectSlot {
    inverse: Option<Inverse>,
}

#[derive(Default)]
struct Scope {
    services: HashMap<TypeId, Box<dyn Any + Send>>,
    effects: Vec<Arc<Mutex<EffectSlot>>>,
}

struct Inner {
    parent: Option<Ctx>,
    scope: Mutex<Scope>,
}

#[derive(Clone)]
pub struct Ctx(Arc<Inner>);

impl Ctx {
    pub fn new_root() -> Self {
        Ctx(Arc::new(Inner {
            parent: None,
            scope: Mutex::new(Scope::default()),
        }))
    }

    /// A child scope: sees parent services (resolution walks up), owns its
    /// own effects. Disposing the child never touches the parent.
    pub fn child(&self) -> Ctx {
        Ctx(Arc::new(Inner {
            parent: Some(self.clone()),
            scope: Mutex::new(Scope::default()),
        }))
    }

    pub fn provide<T: Any + Send>(&self, value: T) {
        self.0
            .scope
            .lock()
            .unwrap()
            .services
            .insert(TypeId::of::<T>(), Box::new(value));
    }

    /// Resolve a service, walking up the scope tree. Clone-out keeps the
    /// context exclusively the mediator (components never hold handles into
    /// each other's state).
    pub fn get<T: Any + Send + Clone>(&self) -> Option<T> {
        if let Some(found) = self
            .0
            .scope
            .lock()
            .unwrap()
            .services
            .get(&TypeId::of::<T>())
        {
            return found.downcast_ref::<T>().cloned();
        }
        self.0.parent.as_ref().and_then(Ctx::get)
    }

    /// Register a revertible effect: the runtime holds the inverse until
    /// this scope is disposed (LIFO). This is the paper's runtime-held
    /// inverse — deliberately NOT tied to any guard's lifetime.
    pub fn on_dispose(&self, inverse: Inverse) {
        self.0
            .scope
            .lock()
            .unwrap()
            .effects
            .push(Arc::new(Mutex::new(EffectSlot {
                inverse: Some(inverse),
            })));
    }

    /// Apply every held inverse in reverse registration order. Children of
    /// this scope are disposed first (they were mounted after it).
    pub fn dispose(self) {
        let effects: Vec<Arc<Mutex<EffectSlot>>> = {
            let mut scope = self.0.scope.lock().unwrap();
            scope.services.clear();
            std::mem::take(&mut scope.effects)
        };
        for slot in effects.into_iter().rev() {
            if let Some(inverse) = slot.lock().unwrap().inverse.take() {
                inverse();
            }
        }
    }
}
