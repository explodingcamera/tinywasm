use core::{
    cell::RefCell,
    sync::atomic::{AtomicUsize, Ordering},
};

use alloc::{boxed::Box, format, rc::Rc, string::ToString, vec, vec::Vec};
use tinywasm_types::*;

use crate::{
    runtime::{self, DefaultRuntime},
    Error, Function, ModuleInstance, RawWasmValue, Result, Trap,
};

// global store id counter
static STORE_ID: AtomicUsize = AtomicUsize::new(0);

/// Global state that can be manipulated by WebAssembly programs
///
/// Data should only be addressable by the module that owns it
///
/// Note that the state doesn't do any garbage collection - so it will grow
/// indefinitely if you keep adding modules to it. When calling temporary
/// functions, you should create a new store and then drop it when you're done (e.g. in a request handler)
///
///  See <https://webassembly.github.io/spec/core/exec/runtime.html#store>
#[derive(Debug)]
pub struct Store {
    id: usize,
    module_instances: Vec<ModuleInstance>,
    module_instance_count: usize,

    pub(crate) data: StoreData,
    pub(crate) runtime: Runtime,
}

#[derive(Debug, Clone, Copy)]
pub(crate) enum Runtime {
    Default,
}

impl Store {
    /// Create a new store
    pub fn new() -> Self {
        Self::default()
    }

    /// Get a module instance by the internal id
    pub fn get_module_instance(&self, addr: ModuleInstanceAddr) -> Option<&ModuleInstance> {
        log::debug!("existing module instances: {:?}", self.module_instances.len());
        self.module_instances.get(addr as usize)
    }

    /// Create a new store with the given runtime
    pub(crate) fn runtime(&self) -> runtime::DefaultRuntime {
        match self.runtime {
            Runtime::Default => DefaultRuntime::default(),
        }
    }
}

impl PartialEq for Store {
    fn eq(&self, other: &Self) -> bool {
        self.id == other.id
    }
}

impl Default for Store {
    fn default() -> Self {
        let id = STORE_ID.fetch_add(1, Ordering::Relaxed);

        Self {
            id,
            module_instances: Vec::new(),
            module_instance_count: 0,
            data: StoreData::default(),
            runtime: Runtime::Default,
        }
    }
}

#[derive(Debug, Default)]
/// Global state that can be manipulated by WebAssembly programs
///
/// Data should only be addressable by the module that owns it
/// See <https://webassembly.github.io/spec/core/exec/runtime.html#store>
pub(crate) struct StoreData {
    pub(crate) funcs: Vec<Rc<FunctionInstance>>,
    pub(crate) tables: Vec<Rc<RefCell<TableInstance>>>,
    pub(crate) memories: Vec<Rc<RefCell<MemoryInstance>>>,
    pub(crate) globals: Vec<Rc<RefCell<GlobalInstance>>>,
    pub(crate) elements: Vec<ElementInstance>,
    pub(crate) datas: Vec<DataInstance>,
}

impl Store {
    /// Get the store's ID (unique per process)
    pub fn id(&self) -> usize {
        self.id
    }

    pub(crate) fn next_module_instance_idx(&self) -> ModuleInstanceAddr {
        self.module_instance_count as ModuleInstanceAddr
    }

    pub(crate) fn add_instance(&mut self, instance: ModuleInstance) -> Result<()> {
        assert!(instance.id() == self.module_instance_count as ModuleInstanceAddr);
        self.module_instances.push(instance);
        self.module_instance_count += 1;
        Ok(())
    }

    /// Add functions to the store, returning their addresses in the store
    pub(crate) fn init_funcs(
        &mut self,
        funcs: Vec<(u32, WasmFunction)>,
        idx: ModuleInstanceAddr,
    ) -> Result<Vec<FuncAddr>> {
        let func_count = self.data.funcs.len();
        let mut func_addrs = Vec::with_capacity(func_count);

        for (i, (type_idx, func)) in funcs.into_iter().enumerate() {
            self.data.funcs.push(Rc::new(FunctionInstance {
                func: Function::Wasm(func),
                _type_idx: type_idx,
                owner: idx,
            }));
            func_addrs.push((i + func_count) as FuncAddr);
        }

        Ok(func_addrs)
    }

    /// Add tables to the store, returning their addresses in the store
    pub(crate) fn init_tables(&mut self, tables: Vec<TableType>, idx: ModuleInstanceAddr) -> Result<Vec<TableAddr>> {
        let table_count = self.data.tables.len();
        let mut table_addrs = Vec::with_capacity(table_count);
        for (i, table) in tables.into_iter().enumerate() {
            self.data.tables.push(Rc::new(RefCell::new(TableInstance::new(table, idx))));
            table_addrs.push((i + table_count) as TableAddr);
        }
        Ok(table_addrs)
    }

    /// Add memories to the store, returning their addresses in the store
    pub(crate) fn init_memories(&mut self, memories: Vec<MemoryType>, idx: ModuleInstanceAddr) -> Result<Vec<MemAddr>> {
        let mem_count = self.data.memories.len();
        let mut mem_addrs = Vec::with_capacity(mem_count);
        for (i, mem) in memories.into_iter().enumerate() {
            if let MemoryArch::I64 = mem.arch {
                return Err(Error::UnsupportedFeature("64-bit memories".to_string()));
            }
            log::info!("adding memory: {:?}", mem);
            self.data.memories.push(Rc::new(RefCell::new(MemoryInstance::new(mem, idx))));

            mem_addrs.push((i + mem_count) as MemAddr);
        }
        Ok(mem_addrs)
    }

    /// Add globals to the store, returning their addresses in the store
    pub(crate) fn init_globals(&mut self, globals: Vec<Global>, idx: ModuleInstanceAddr) -> Result<Vec<Addr>> {
        let global_count = self.data.globals.len();
        let mut global_addrs = Vec::with_capacity(global_count);
        for (i, global) in globals.iter().enumerate() {
            self.data.globals.push(Rc::new(RefCell::new(GlobalInstance::new(
                global.ty,
                self.eval_const(&global.init)?,
                idx,
            ))));
            global_addrs.push((i + global_count) as Addr);
        }

        Ok(global_addrs)
    }

    /// Add elements to the store, returning their addresses in the store
    /// Should be called after the tables have been added
    pub(crate) fn init_elements(
        &mut self,
        table_addrs: &[TableAddr],
        func_addrs: &[FuncAddr],
        elements: Vec<Element>,
        idx: ModuleInstanceAddr,
    ) -> Result<(Box<[Addr]>, Option<Trap>)> {
        let elem_count = self.data.elements.len();
        let mut elem_addrs = Vec::with_capacity(elem_count);
        for (i, element) in elements.into_iter().enumerate() {
            let init = element
                .items
                .iter()
                .map(|item| {
                    item.addr().ok_or_else(|| {
                        Error::UnsupportedFeature(format!("const expression other than ref: {:?}", item))
                    })
                })
                .collect::<Result<Vec<_>>>()?;

            log::error!("element kind: {:?}", element.kind);

            let items = match element.kind {
                // doesn't need to be initialized, can be initialized lazily using the `table.init` instruction
                ElementKind::Passive => Some(init),

                // this one is not available to the runtime but needs to be initialized to declare references
                ElementKind::Declared => {
                    // a. Execute the instruction elm.drop i
                    None
                }

                // this one is active, so we need to initialize it (essentially a `table.init` instruction)
                ElementKind::Active { offset, table } => {
                    let offset = self.eval_i32_const(&offset)?;
                    let table_addr = table_addrs
                        .get(table as usize)
                        .copied()
                        .ok_or_else(|| Error::Other(format!("table {} not found for element {}", table, i)))?;

                    if let Some(table) = self.data.tables.get_mut(table_addr as usize) {
                        // In wasm 2.0, it's possible to call a function that hasn't been instantiated yet,
                        // when using a partially initialized active element segments.
                        // This isn't mentioned in the spec, but the "unofficial" testsuite has a test for it:
                        // https://github.com/WebAssembly/testsuite/blob/5a1a590603d81f40ef471abba70a90a9ae5f4627/linking.wast#L264-L276
                        // I have NO IDEA why this is allowed, but it is.
                        if let Err(Error::Trap(trap)) = table.borrow_mut().init(func_addrs, offset, &init) {
                            return Ok((elem_addrs.into_boxed_slice(), Some(trap)));
                        }
                    } else {
                        return Err(Error::Other(format!("table {} not found for element {}", table, i)));
                    }

                    // f. Execute the instruction elm.drop i
                    None
                }
            };

            self.data.elements.push(ElementInstance::new(element.kind, idx, items));
            elem_addrs.push((i + elem_count) as Addr);
        }

        // this should be optimized out by the compiler
        Ok((elem_addrs.into_boxed_slice(), None))
    }

    /// Add data to the store, returning their addresses in the store
    pub(crate) fn init_datas(
        &mut self,
        mem_addrs: &[MemAddr],
        datas: Vec<Data>,
        idx: ModuleInstanceAddr,
    ) -> Result<(Box<[Addr]>, Option<Trap>)> {
        let data_count = self.data.datas.len();
        let mut data_addrs = Vec::with_capacity(data_count);
        for (i, data) in datas.into_iter().enumerate() {
            use tinywasm_types::DataKind::*;
            match data.kind {
                Active { mem: mem_addr, offset } => {
                    // a. Assert: memidx == 0
                    if mem_addr != 0 {
                        return Err(Error::UnsupportedFeature("data segments for non-zero memories".to_string()));
                    }

                    let mem_addr = mem_addrs
                        .get(mem_addr as usize)
                        .copied()
                        .ok_or_else(|| Error::Other(format!("memory {} not found for data segment {}", mem_addr, i)))?;

                    let offset = self.eval_i32_const(&offset)?;

                    let mem =
                        self.data.memories.get_mut(mem_addr as usize).ok_or_else(|| {
                            Error::Other(format!("memory {} not found for data segment {}", mem_addr, i))
                        })?;

                    // See comment for active element sections in the function above why we need to do this here
                    if let Err(Error::Trap(trap)) = mem.borrow_mut().store(offset as usize, 0, &data.data) {
                        return Ok((data_addrs.into_boxed_slice(), Some(trap)));
                    }

                    // drop the data
                    continue;
                }
                Passive => {}
            }

            self.data.datas.push(DataInstance::new(data.data.to_vec(), idx));
            data_addrs.push((i + data_count) as Addr);
        }

        // this should be optimized out by the compiler
        Ok((data_addrs.into_boxed_slice(), None))
    }

    pub(crate) fn add_global(&mut self, ty: GlobalType, value: RawWasmValue, idx: ModuleInstanceAddr) -> Result<Addr> {
        self.data.globals.push(Rc::new(RefCell::new(GlobalInstance::new(ty, value, idx))));
        Ok(self.data.globals.len() as Addr - 1)
    }

    pub(crate) fn add_table(&mut self, table: TableType, idx: ModuleInstanceAddr) -> Result<TableAddr> {
        self.data.tables.push(Rc::new(RefCell::new(TableInstance::new(table, idx))));
        Ok(self.data.tables.len() as TableAddr - 1)
    }

    pub(crate) fn add_mem(&mut self, mem: MemoryType, idx: ModuleInstanceAddr) -> Result<MemAddr> {
        if let MemoryArch::I64 = mem.arch {
            return Err(Error::UnsupportedFeature("64-bit memories".to_string()));
        }
        self.data.memories.push(Rc::new(RefCell::new(MemoryInstance::new(mem, idx))));
        Ok(self.data.memories.len() as MemAddr - 1)
    }

    pub(crate) fn add_func(&mut self, func: Function, type_idx: TypeAddr, idx: ModuleInstanceAddr) -> Result<FuncAddr> {
        self.data.funcs.push(Rc::new(FunctionInstance { func, _type_idx: type_idx, owner: idx }));
        Ok(self.data.funcs.len() as FuncAddr - 1)
    }

    /// Evaluate a constant expression, only supporting i32 globals and i32.const
    pub(crate) fn eval_i32_const(&self, const_instr: &tinywasm_types::ConstInstruction) -> Result<i32> {
        use tinywasm_types::ConstInstruction::*;
        let val = match const_instr {
            I32Const(i) => *i,
            GlobalGet(addr) => {
                let addr = *addr as usize;
                let global = self.data.globals[addr].clone();
                let val = global.borrow().value;
                i32::from(val)
            }
            _ => return Err(Error::Other("expected i32".to_string())),
        };
        Ok(val)
    }

    /// Evaluate a constant expression
    pub(crate) fn eval_const(&self, const_instr: &tinywasm_types::ConstInstruction) -> Result<RawWasmValue> {
        use tinywasm_types::ConstInstruction::*;
        let val = match const_instr {
            F32Const(f) => RawWasmValue::from(*f),
            F64Const(f) => RawWasmValue::from(*f),
            I32Const(i) => RawWasmValue::from(*i),
            I64Const(i) => RawWasmValue::from(*i),
            GlobalGet(addr) => {
                let addr = *addr as usize;
                let global = self.data.globals[addr].clone();
                let val = global.borrow().value;
                val
            }
            RefNull(t) => RawWasmValue::from(t.default_value()),
            RefFunc(idx) => RawWasmValue::from(*idx as i64),
        };
        Ok(val)
    }

    /// Get the function at the actual index in the store
    pub(crate) fn get_func(&self, addr: usize) -> Result<&Rc<FunctionInstance>> {
        self.data.funcs.get(addr).ok_or_else(|| Error::Other(format!("function {} not found", addr)))
    }

    /// Get the memory at the actual index in the store
    pub(crate) fn get_mem(&self, addr: usize) -> Result<&Rc<RefCell<MemoryInstance>>> {
        self.data.memories.get(addr).ok_or_else(|| Error::Other(format!("memory {} not found", addr)))
    }

    /// Get the table at the actual index in the store
    pub(crate) fn get_table(&self, addr: usize) -> Result<&Rc<RefCell<TableInstance>>> {
        self.data.tables.get(addr).ok_or_else(|| Error::Other(format!("table {} not found", addr)))
    }

    /// Get the element at the actual index in the store
    pub(crate) fn get_elem(&self, addr: usize) -> Result<&ElementInstance> {
        self.data.elements.get(addr).ok_or_else(|| Error::Other(format!("element {} not found", addr)))
    }

    /// Get the global at the actual index in the store
    pub fn get_global_val(&self, addr: usize) -> Result<RawWasmValue> {
        self.data
            .globals
            .get(addr)
            .ok_or_else(|| Error::Other(format!("global {} not found", addr)))
            .map(|global| global.borrow().value)
    }

    pub(crate) fn set_global_val(&mut self, addr: usize, value: RawWasmValue) -> Result<()> {
        self.data
            .globals
            .get(addr)
            .ok_or_else(|| Error::Other(format!("global {} not found", addr)))
            .map(|global| global.borrow_mut().value = value)
    }
}

#[derive(Debug)]
/// A WebAssembly Function Instance
///
/// See <https://webassembly.github.io/spec/core/exec/runtime.html#function-instances>
pub struct FunctionInstance {
    pub(crate) func: Function,
    pub(crate) _type_idx: TypeAddr,
    pub(crate) owner: ModuleInstanceAddr, // index into store.module_instances, none for host functions
}

// TODO: check if this actually helps
#[inline(always)]
#[cold]
const fn cold() {}

impl FunctionInstance {
    pub(crate) fn assert_wasm(&self) -> Result<&WasmFunction> {
        match &self.func {
            Function::Wasm(w) => Ok(w),
            Function::Host(_) => {
                cold();
                Err(Error::Other("expected wasm function".to_string()))
            }
        }
    }
}

#[derive(Debug, Clone, Copy)]
pub(crate) enum TableElement {
    Uninitialized,
    Initialized(Addr),
}

impl TableElement {
    pub(crate) fn addr(&self) -> Option<Addr> {
        match self {
            TableElement::Uninitialized => None,
            TableElement::Initialized(addr) => Some(*addr),
        }
    }
}

const MAX_TABLE_SIZE: u32 = 10000000;

/// A WebAssembly Table Instance
///
/// See <https://webassembly.github.io/spec/core/exec/runtime.html#table-instances>
#[derive(Debug)]
pub(crate) struct TableInstance {
    pub(crate) elements: Vec<TableElement>,
    pub(crate) kind: TableType,
    pub(crate) _owner: ModuleInstanceAddr, // index into store.module_instances
}

impl TableInstance {
    pub(crate) fn new(kind: TableType, owner: ModuleInstanceAddr) -> Self {
        Self { elements: vec![TableElement::Uninitialized; kind.size_initial as usize], kind, _owner: owner }
    }

    pub(crate) fn get_wasm_val(&self, addr: usize) -> Result<WasmValue> {
        let val = self.get(addr)?.addr();

        Ok(match self.kind.element_type {
            ValType::RefFunc => val.map(WasmValue::RefFunc).unwrap_or(WasmValue::RefNull(ValType::RefFunc)),
            ValType::RefExtern => val.map(WasmValue::RefExtern).unwrap_or(WasmValue::RefNull(ValType::RefExtern)),
            _ => unimplemented!("unsupported table type: {:?}", self.kind.element_type),
        })
    }

    pub(crate) fn get(&self, addr: usize) -> Result<&TableElement> {
        self.elements.get(addr).ok_or_else(|| Error::Trap(Trap::UndefinedElement { index: addr }))
    }

    pub(crate) fn set(&mut self, table_idx: usize, value: Addr) -> Result<()> {
        self.grow_to_fit(table_idx + 1)
            .ok_or_else(|| {
                Error::Trap(crate::Trap::TableOutOfBounds { offset: table_idx, len: 1, max: self.elements.len() })
            })
            .and_then(|_| {
                self.elements[table_idx] = TableElement::Initialized(value);
                Ok(())
            })
    }

    pub(crate) fn grow_to_fit(&mut self, new_size: usize) -> Option<()> {
        if new_size > self.elements.len() {
            if new_size <= self.kind.size_max.unwrap_or(MAX_TABLE_SIZE) as usize {
                self.elements.resize(new_size, TableElement::Uninitialized);
            } else {
                return None;
            }
        }
        Some(())
    }

    pub(crate) fn size(&self) -> i32 {
        self.elements.len() as i32
    }

    pub(crate) fn init(&mut self, func_addrs: &[u32], offset: i32, init: &[Option<Addr>]) -> Result<()> {
        let init = init
            .iter()
            .map(|item| match item {
                None => TableElement::Uninitialized,
                Some(item) => TableElement::Initialized(match self.kind.element_type == ValType::RefFunc {
                    true => *func_addrs.get(*item as usize).expect(
                        "error initializing table: function not found. This should have been caught by the validator",
                    ),
                    false => *item,
                }),
            })
            .collect::<Vec<_>>();

        let offset = offset as usize;
        let end = offset.checked_add(init.len()).ok_or_else(|| {
            Error::Trap(crate::Trap::TableOutOfBounds { offset, len: init.len(), max: self.elements.len() })
        })?;

        if end > self.elements.len() || end < offset {
            return Err(crate::Trap::TableOutOfBounds { offset, len: init.len(), max: self.elements.len() }.into());
        }

        self.elements[offset..end].copy_from_slice(&init);
        log::debug!("table: {:?}", self.elements);
        Ok(())
    }
}

pub(crate) const PAGE_SIZE: usize = 65536;
pub(crate) const MAX_PAGES: usize = 65536;
pub(crate) const MAX_SIZE: usize = PAGE_SIZE * MAX_PAGES;

/// A WebAssembly Memory Instance
///
/// See <https://webassembly.github.io/spec/core/exec/runtime.html#memory-instances>
#[derive(Debug)]
pub(crate) struct MemoryInstance {
    pub(crate) kind: MemoryType,
    pub(crate) data: Vec<u8>,
    pub(crate) page_count: usize,
    pub(crate) _owner: ModuleInstanceAddr, // index into store.module_instances
}

impl MemoryInstance {
    pub(crate) fn new(kind: MemoryType, owner: ModuleInstanceAddr) -> Self {
        assert!(kind.page_count_initial <= kind.page_count_max.unwrap_or(MAX_PAGES as u64));
        log::debug!("initializing memory with {} pages", kind.page_count_initial);

        Self {
            kind,
            data: vec![0; PAGE_SIZE * kind.page_count_initial as usize],
            page_count: kind.page_count_initial as usize,
            _owner: owner,
        }
    }

    pub(crate) fn store(&mut self, addr: usize, _align: usize, data: &[u8]) -> Result<()> {
        let end = addr.checked_add(data.len()).ok_or_else(|| {
            Error::Trap(crate::Trap::MemoryOutOfBounds { offset: addr, len: data.len(), max: self.data.len() })
        })?;

        if end > self.data.len() || end < addr {
            return Err(Error::Trap(crate::Trap::MemoryOutOfBounds {
                offset: addr,
                len: data.len(),
                max: self.data.len(),
            }));
        }

        // WebAssembly doesn't require alignment for stores
        self.data[addr..end].copy_from_slice(data);
        Ok(())
    }

    pub(crate) fn max_pages(&self) -> usize {
        self.kind.page_count_max.unwrap_or(MAX_PAGES as u64) as usize
    }

    pub(crate) fn load(&self, addr: usize, _align: usize, len: usize) -> Result<&[u8]> {
        let end = addr
            .checked_add(len)
            .ok_or_else(|| Error::Trap(crate::Trap::MemoryOutOfBounds { offset: addr, len, max: self.max_pages() }))?;

        if end > self.data.len() {
            return Err(Error::Trap(crate::Trap::MemoryOutOfBounds { offset: addr, len, max: self.data.len() }));
        }

        // WebAssembly doesn't require alignment for loads
        Ok(&self.data[addr..end])
    }

    pub(crate) fn size(&self) -> i32 {
        log::debug!("memory pages: {}", self.page_count);
        log::debug!("memory size: {}", self.page_count * PAGE_SIZE);
        self.page_count as i32
    }

    pub(crate) fn grow(&mut self, delta: i32) -> Option<i32> {
        let current_pages = self.size();
        let new_pages = current_pages + delta;

        if new_pages < 0 || new_pages > MAX_PAGES as i32 {
            return None;
        }

        if new_pages as usize > self.max_pages() {
            log::info!("memory size out of bounds: {}", new_pages);
            return None;
        }

        let new_size = new_pages as usize * PAGE_SIZE;
        if new_size > MAX_SIZE {
            return None;
        }

        // Zero initialize the new pages
        self.data.resize(new_size, 0);
        self.page_count = new_pages as usize;

        log::debug!("memory was {} pages", current_pages);
        log::debug!("memory grown by {} pages", delta);
        log::debug!("memory grown to {} pages", self.page_count);

        Some(current_pages)
    }
}

/// A WebAssembly Global Instance
///
/// See <https://webassembly.github.io/spec/core/exec/runtime.html#global-instances>
#[derive(Debug)]
pub(crate) struct GlobalInstance {
    pub(crate) value: RawWasmValue,
    pub(crate) _ty: GlobalType,
    pub(crate) _owner: ModuleInstanceAddr, // index into store.module_instances
}

impl GlobalInstance {
    pub(crate) fn new(ty: GlobalType, value: RawWasmValue, owner: ModuleInstanceAddr) -> Self {
        Self { _ty: ty, value, _owner: owner }
    }
}

/// A WebAssembly Element Instance
///
/// See <https://webassembly.github.io/spec/core/exec/runtime.html#element-instances>
#[derive(Debug)]
pub(crate) struct ElementInstance {
    pub(crate) kind: ElementKind,
    pub(crate) items: Option<Vec<Option<u32>>>, // none is the element was dropped
    _owner: ModuleInstanceAddr,                 // index into store.module_instances
}

impl ElementInstance {
    pub(crate) fn new(kind: ElementKind, owner: ModuleInstanceAddr, items: Option<Vec<Option<u32>>>) -> Self {
        Self { kind, _owner: owner, items }
    }
}

/// A WebAssembly Data Instance
///
/// See <https://webassembly.github.io/spec/core/exec/runtime.html#data-instances>
#[derive(Debug)]
pub(crate) struct DataInstance {
    pub(crate) _data: Vec<u8>,
    pub(crate) _owner: ModuleInstanceAddr, // index into store.module_instances
}

impl DataInstance {
    pub(crate) fn new(data: Vec<u8>, owner: ModuleInstanceAddr) -> Self {
        Self { _data: data, _owner: owner }
    }
}
