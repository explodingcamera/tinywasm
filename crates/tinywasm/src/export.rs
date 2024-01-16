use alloc::boxed::Box;
use tinywasm_types::{Export, ExternalKind};

#[derive(Debug)]
/// Exports of a module instance
// TODO: Maybe use a BTreeMap instead?
pub struct ExportInstance(pub(crate) Box<[Export]>);

impl ExportInstance {
    /// Get an export by name
    pub fn get(&self, name: &str, ty: ExternalKind) -> Option<&Export> {
        self.0.iter().find(|e| e.name == name.into() && e.kind == ty)
    }

    pub(crate) fn get_untyped(&self, name: &str) -> Option<&Export> {
        self.0.iter().find(|e| e.name == name.into())
    }
}
