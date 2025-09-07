use crate::{CoroState, Imports, ModuleInstance, PotentialCoroCallResult, Result, Store, SuspendedFunc};
use tinywasm_types::{ResumeArgument, TinyWasmModule};

/// A WebAssembly Module
///
/// See <https://webassembly.github.io/spec/core/syntax/modules.html#syntax-module>
#[derive(Debug, Clone)]
pub struct Module(pub(crate) TinyWasmModule);

impl From<&TinyWasmModule> for Module {
    fn from(data: &TinyWasmModule) -> Self {
        Self(data.clone())
    }
}

impl From<TinyWasmModule> for Module {
    fn from(data: TinyWasmModule) -> Self {
        Self(data)
    }
}

impl Module {
    #[cfg(feature = "parser")]
    /// Parse a module from bytes. Requires `parser` feature.
    pub fn parse_bytes(wasm: &[u8]) -> Result<Self> {
        let parser = tinywasm_parser::Parser::new();
        let data = parser.parse_module_bytes(wasm)?;
        Ok(data.into())
    }

    #[cfg(all(feature = "parser", feature = "std"))]
    /// Parse a module from a file. Requires `parser` and `std` features.
    pub fn parse_file(path: impl AsRef<crate::std::path::Path> + Clone) -> Result<Self> {
        let parser = tinywasm_parser::Parser::new();
        let data = parser.parse_module_file(path)?;
        Ok(data.into())
    }

    #[cfg(all(feature = "parser", feature = "std"))]
    /// Parse a module from a stream. Requires `parser` and `std` features.
    pub fn parse_stream(stream: impl crate::std::io::Read) -> Result<Self> {
        let parser = tinywasm_parser::Parser::new();
        let data = parser.parse_module_stream(stream)?;
        Ok(data.into())
    }

    /// Instantiate the module in the given store
    ///
    /// Runs the start function if it exists
    ///
    /// If you want to run the start function yourself, use `ModuleInstance::instantiate`
    ///
    /// See <https://webassembly.github.io/spec/core/exec/modules.html#exec-instantiation>
    pub fn instantiate(self, store: &mut Store, imports: Option<Imports>) -> Result<ModuleInstance> {
        let instance = ModuleInstance::instantiate(store, self, imports)?;
        let _ = instance.start(store)?;
        Ok(instance)
    }

    /// same as [Self::instantiate] but accounts for possibility of start function suspending, in which case it returns
    /// [PotentialCoroCallResult::Suspended]. You can call [CoroState::resume] on it at any time to resume instantiation
    pub fn instantiate_coro(
        self,
        store: &mut Store,
        imports: Option<Imports>,
    ) -> Result<PotentialCoroCallResult<ModuleInstance, IncompleteModule>> {
        let instance = ModuleInstance::instantiate(store, self, imports)?;
        let core_res = match instance.start_coro(store)? {
            Some(res) => res,
            None => return Ok(PotentialCoroCallResult::Return(instance)),
        };
        Ok(match core_res {
            crate::PotentialCoroCallResult::Return(_) => PotentialCoroCallResult::Return(instance),
            crate::PotentialCoroCallResult::Suspended(suspend_reason, state) => {
                PotentialCoroCallResult::Suspended(suspend_reason, IncompleteModule(Some(HitTheFloor(instance, state))))
            }
        })
    }
}

/// a corostate that results in [ModuleInstance] when finished
#[derive(Debug)]
pub struct IncompleteModule(Option<HitTheFloor>);

#[derive(Debug)]
struct HitTheFloor(ModuleInstance, SuspendedFunc);

impl CoroState<ModuleInstance, &mut Store> for IncompleteModule {
    fn resume(&mut self, ctx: &mut Store, arg: ResumeArgument) -> Result<crate::CoroStateResumeResult<ModuleInstance>> {
        let mut body: HitTheFloor = match self.0.take() {
            Some(body) => body,
            None => return Err(crate::Error::InvalidResume),
        };
        let coro_res = match body.1.resume(ctx, arg) {
            Ok(res) => res,
            Err(e) => {
                self.0 = Some(body);
                return Err(e);
            }
        };
        match coro_res {
            crate::CoroStateResumeResult::Return(_) => {
                let res = body.0;
                Ok(crate::CoroStateResumeResult::Return(res))
            }
            crate::CoroStateResumeResult::Suspended(suspend_reason) => {
                self.0 = Some(body); // ...once told me
                Ok(crate::CoroStateResumeResult::Suspended(suspend_reason))
            }
        }
    }
}
