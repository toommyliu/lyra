use std::{cell::Cell, rc::Rc};

use ruffle_core::{
    context::UpdateContext,
    external::{ExternalInterfaceProvider, Value},
};

#[derive(Clone, Default)]
pub(super) struct Bridge {
    loaded: Rc<Cell<bool>>,
}

impl Bridge {
    pub(super) fn loaded(&self) -> bool {
        self.loaded.get()
    }
}

impl ExternalInterfaceProvider for Bridge {
    fn call_method(&self, _context: &mut UpdateContext<'_>, name: &str, _args: &[Value]) -> Value {
        if name == "onLoaded" {
            self.loaded.set(true);
        }
        Value::Undefined
    }

    fn on_callback_available(&self, name: &str) {
        tracing::debug!(callback = name, "loader callback registered");
    }

    fn get_id(&self) -> Option<String> {
        None
    }
}
