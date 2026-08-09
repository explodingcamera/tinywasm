use tinywasm_types::TypeAddr;

#[derive(Clone, Copy)]
#[cfg_attr(feature = "debug", derive(Debug))]
pub(crate) struct TagInstance {
    pub(crate) type_addr: TypeAddr,
}
