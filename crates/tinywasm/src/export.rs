use alloc::{boxed::Box, format};
use tinywasm_types::{Export, ExternalKind};

use crate::{Error, Result};

#[derive(Debug)]
pub struct ExportInstance(pub(crate) Box<[Export]>);

impl ExportInstance {
    pub fn func(&self, name: &str) -> Result<&Export> {
        self.0
            .iter()
            .find(|e| e.name == name.into() && e.kind == ExternalKind::Func)
            .ok_or(Error::Other(format!("export {} not found", name)))
    }
}
