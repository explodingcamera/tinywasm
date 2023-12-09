use alloc::{boxed::Box, format};
use tinywasm_types::{Export, ExternalKind};

use crate::{Error, Result};

#[derive(Debug)]
/// Exports of a module instance
pub struct ExportInstance(pub(crate) Box<[Export]>);

impl ExportInstance {
    /// Get an export by name
    pub fn get(&self, name: &str, ty: ExternalKind) -> Result<&Export> {
        self.0
            .iter()
            .find(|e| e.name == name.into() && e.kind == ty)
            .ok_or(Error::Other(format!("export {} not found", name)))
    }
}
