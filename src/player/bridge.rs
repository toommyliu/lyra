use ruffle_core::{
    context::UpdateContext,
    external::{ExternalInterfaceProvider, Value},
};

pub(super) struct Bridge;

impl ExternalInterfaceProvider for Bridge {
    fn call_method(&self, _context: &mut UpdateContext<'_>, _name: &str, _args: &[Value]) -> Value {
        Value::Undefined
    }

    fn on_callback_available(&self, name: &str) {
        tracing::debug!(callback = name, "loader callback registered");
    }

    fn get_id(&self) -> Option<String> {
        None
    }
}
