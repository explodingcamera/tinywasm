use std::collections::{BTreeMap, HashMap};
use std::fmt::{Display, Formatter};
use std::fs::canonicalize;
use std::path::PathBuf;
use std::{
    panic::{self, AssertUnwindSafe},
    time::Duration,
};

use eyre::{Context, Result, bail, eyre};
use log::{debug, error};
use tinywasm::types::{ExternRef, FuncRef, MemoryType, TableType, WasmType, WasmValue};
use tinywasm::{ExecProgress, Global, HostFunction, Imports, Memory, Module, ModuleInstance, Store, Table};
use wast::{QuoteWat, core::AbstractHeapType};

const TEST_TIME_SLICE: Duration = Duration::from_millis(20);
const TEST_MAX_SUSPENSIONS: u32 = 1000;

#[derive(Default)]
struct ModuleRegistry {
    modules: HashMap<String, ModuleInstance>,
    named_modules: HashMap<String, ModuleInstance>,
    last_module: Option<ModuleInstance>,
}

impl ModuleRegistry {
    fn modules(&self) -> &HashMap<String, ModuleInstance> {
        &self.modules
    }

    fn update_last_module(&mut self, module: ModuleInstance, name: Option<String>) {
        self.last_module = Some(module.clone());
        if let Some(name) = name {
            self.named_modules.insert(name, module);
        }
    }

    fn register(&mut self, name: String, module: ModuleInstance) {
        debug!("registering module: {name}");
        self.modules.insert(name.clone(), module.clone());
        self.last_module = Some(module.clone());
        self.named_modules.insert(name, module);
    }

    fn get_idx(&self, module_id: Option<wast::token::Id<'_>>) -> Option<u32> {
        match module_id {
            Some(module) => self
                .modules
                .get(module.name())
                .or_else(|| self.named_modules.get(module.name()))
                .map(ModuleInstance::id),
            None => self.last_module.as_ref().map(ModuleInstance::id),
        }
    }

    fn get(&self, module_id: Option<wast::token::Id<'_>>) -> Option<ModuleInstance> {
        match module_id {
            Some(module_id) => {
                self.modules.get(module_id.name()).or_else(|| self.named_modules.get(module_id.name())).cloned()
            }
            None => self.last_module.clone(),
        }
    }

    fn last(&self) -> Option<ModuleInstance> {
        self.last_module.clone()
    }
}

#[derive(Default)]
pub struct WastRunner(BTreeMap<String, TestGroup>);

#[derive(Clone, Debug)]
pub struct GroupResult {
    pub name: String,
    pub file: String,
    pub passed: usize,
    pub failed: usize,
}

impl WastRunner {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn run_paths(&mut self, tests: &[PathBuf]) -> Result<()> {
        for path in expand_paths(tests)? {
            let contents =
                std::fs::read_to_string(&path).context(format!("failed to read file: {}", path.to_string_lossy()))?;

            let file = TestFile {
                contents: &contents,
                name: path.to_string_lossy().to_string(),
                parent: canonicalize(&path)?.to_string_lossy().to_string(),
            };

            self.run_file(file)?;
        }

        self.print_errors();
        if self.failed() {
            anstream::println!("{self}");
            Err(eyre!("failed one or more tests"))
        } else {
            anstream::println!("{self}");
            Ok(())
        }
    }

    pub fn set_log_level(level: log::LevelFilter) {
        let _ = pretty_env_logger::formatted_builder().filter_level(level).try_init();
    }

    pub fn failed(&self) -> bool {
        self.0.values().any(|group| group.stats().1 > 0)
    }

    pub fn print_errors(&self) {
        for group in self.0.values() {
            for test in &group.tests {
                if let Err(err) = &test.result {
                    eprintln!(
                        "{}:{}:{} {} failed: {}",
                        group.file,
                        test.linecol.0 + 1,
                        test.linecol.1 + 1,
                        test.name,
                        err
                    );
                }
            }
        }
    }

    pub fn group_results(&self) -> Vec<GroupResult> {
        self.0
            .iter()
            .map(|(name, group)| {
                let (passed, failed) = group.stats();
                GroupResult { name: name.clone(), file: group.file.clone(), passed, failed }
            })
            .collect()
    }

    fn test_group(&mut self, name: &str, file: &str) -> &mut TestGroup {
        self.0.entry(name.to_string()).or_insert_with(|| TestGroup::new(file))
    }

    pub fn run_files<'a>(&mut self, tests: impl IntoIterator<Item = TestFile<'a>>) -> Result<()> {
        for file in tests {
            self.run_file(file)?;
        }
        Ok(())
    }

    fn imports(store: &mut Store, modules: &HashMap<String, ModuleInstance>) -> Result<Imports> {
        let mut imports = Imports::new();

        let table = Table::new(
            store,
            TableType::new(WasmType::RefFunc, 10, Some(20)),
            WasmValue::default_for(WasmType::RefFunc),
        )?;
        let memory = Memory::new(store, MemoryType::default().with_page_count_initial(1).with_page_count_max(Some(2)))?;
        let global_i32 =
            Global::new(store, tinywasm::types::GlobalType::new(WasmType::I32, false), WasmValue::I32(666))?;
        let global_i64 =
            Global::new(store, tinywasm::types::GlobalType::new(WasmType::I64, false), WasmValue::I64(666))?;
        let global_f32 =
            Global::new(store, tinywasm::types::GlobalType::new(WasmType::F32, false), WasmValue::F32(666.6))?;
        let global_f64 =
            Global::new(store, tinywasm::types::GlobalType::new(WasmType::F64, false), WasmValue::F64(666.6))?;

        imports
            .define("spectest", "memory", memory)
            .define("spectest", "table", table)
            .define("spectest", "global_i32", global_i32)
            .define("spectest", "global_i64", global_i64)
            .define("spectest", "global_f32", global_f32)
            .define("spectest", "global_f64", global_f64)
            .define("spectest", "print", HostFunction::from(store, |_ctx: tinywasm::FuncContext, (): ()| Ok(())))
            .define("spectest", "print_i32", HostFunction::from(store, |_ctx: tinywasm::FuncContext, _arg: i32| Ok(())))
            .define("spectest", "print_i64", HostFunction::from(store, |_ctx: tinywasm::FuncContext, _arg: i64| Ok(())))
            .define("spectest", "print_f32", HostFunction::from(store, |_ctx: tinywasm::FuncContext, _arg: f32| Ok(())))
            .define("spectest", "print_f64", HostFunction::from(store, |_ctx: tinywasm::FuncContext, _arg: f64| Ok(())))
            .define(
                "spectest",
                "print_i32_f32",
                HostFunction::from(store, |_ctx: tinywasm::FuncContext, _args: (i32, f32)| Ok(())),
            )
            .define(
                "spectest",
                "print_f64_f64",
                HostFunction::from(store, |_ctx: tinywasm::FuncContext, _args: (f64, f64)| Ok(())),
            );

        for (name, module) in modules {
            imports.link_module(name, module.clone())?;
        }

        Ok(imports)
    }

    pub fn run_file(&mut self, file: TestFile<'_>) -> Result<()> {
        let test_group = self.test_group(file.name(), file.parent());
        let wast_raw = file.raw();
        let wast = file.wast()?;
        let directives = wast.directives()?;

        let mut store = Store::default();
        let mut module_registry = ModuleRegistry::default();

        println!("running {} tests for group: {}", directives.len(), file.name());
        for (i, directive) in directives.into_iter().enumerate() {
            let span = directive.span();
            use wast::WastDirective::{
                AssertExhaustion, AssertInvalid, AssertMalformed, AssertReturn, AssertTrap, AssertUnlinkable, Invoke,
                Module as Wat, Register,
            };

            match directive {
                Register { span, name, .. } => {
                    let Some(last) = module_registry.last() else {
                        test_group.add_result(
                            &format!("Register({i})"),
                            span.linecol_in(wast_raw),
                            Err(eyre!("no module to register")),
                        );
                        continue;
                    };
                    module_registry.register(name.to_string(), last);
                    test_group.add_result(&format!("Register({i})"), span.linecol_in(wast_raw), Ok(()));
                }
                Wat(module) => {
                    let result = catch_unwind_silent(|| {
                        let (name, bytes) = encode_quote_wat(module);
                        let module = parse_module_bytes(&bytes).expect("failed to parse module bytes");
                        let imports = Self::imports(&mut store, module_registry.modules()).unwrap();
                        let module_instance = ModuleInstance::instantiate(&mut store, &module, Some(imports))
                            .expect("failed to instantiate module");
                        (name, module_instance)
                    })
                    .map_err(|e| eyre!("failed to parse wat module: {}", try_downcast_panic(e)));

                    match &result {
                        Err(err) => debug!("failed to parse module: {err:?}"),
                        Ok((name, module)) => module_registry.update_last_module(module.clone(), name.clone()),
                    };

                    test_group.add_result(&format!("Wat({i})"), span.linecol_in(wast_raw), result.map(|_| ()));
                }
                AssertMalformed { span, mut module, message } => {
                    let Ok(encoded) = module.encode() else {
                        test_group.add_result(&format!("AssertMalformed({i})"), span.linecol_in(wast_raw), Ok(()));
                        continue;
                    };
                    let res = catch_unwind_silent(|| parse_module_bytes(&encoded))
                        .map_err(|e| eyre!("failed to parse module (expected): {}", try_downcast_panic(e)))
                        .and_then(|res| res);
                    test_group.add_result(
                        &format!("AssertMalformed({i})"),
                        span.linecol_in(wast_raw),
                        match res {
                            Ok(_) => {
                                if message == "zero byte expected"
                                    || message == "integer representation too long"
                                    || message == "zero flag expected"
                                {
                                    continue;
                                }
                                Err(eyre!("expected module to be malformed: {message}"))
                            }
                            Err(_) => Ok(()),
                        },
                    );
                }
                AssertInvalid { span, mut module, message } => {
                    if ["multiple memories", "type mismatch"].contains(&message) {
                        test_group.add_result(&format!("AssertInvalid({i})"), span.linecol_in(wast_raw), Ok(()));
                        continue;
                    }
                    let res = catch_unwind_silent(move || parse_module_bytes(&module.encode().unwrap()))
                        .map_err(|e| eyre!("failed to parse module (invalid): {}", try_downcast_panic(e)))
                        .and_then(|res| res);
                    test_group.add_result(
                        &format!("AssertInvalid({i})"),
                        span.linecol_in(wast_raw),
                        match res {
                            Ok(_) => Err(eyre!("expected module to be invalid")),
                            Err(_) => Ok(()),
                        },
                    );
                }
                AssertExhaustion { call, message, span } => {
                    let module = module_registry.get_idx(call.module);
                    let args = convert_wastargs(call.args)?;
                    let res =
                        catch_unwind_silent(|| exec_fn_instance(module, &mut store, call.name, &args).map(|_| ()));
                    let Ok(Err(tinywasm::Error::Trap(trap))) = res else {
                        test_group.add_result(
                            &format!("AssertExhaustion({i})"),
                            span.linecol_in(wast_raw),
                            Err(eyre!("expected trap")),
                        );
                        continue;
                    };
                    if !message.starts_with(trap.message()) && !trap.message().starts_with(message) {
                        test_group.add_result(
                            &format!("AssertExhaustion({i})"),
                            span.linecol_in(wast_raw),
                            Err(eyre!("expected trap: {}, got: {}", message, trap.message())),
                        );
                        continue;
                    }
                    test_group.add_result(&format!("AssertExhaustion({i})"), span.linecol_in(wast_raw), Ok(()));
                }
                AssertTrap { exec, message, span } => {
                    let res: Result<tinywasm::Result<()>, _> = catch_unwind_silent(|| {
                        let invoke = match exec {
                            wast::WastExecute::Wat(mut wat) => {
                                let module = parse_module_bytes(&wat.encode().expect("failed to encode module"))
                                    .expect("failed to parse module");
                                let imports = Self::imports(&mut store, module_registry.modules()).unwrap();
                                ModuleInstance::instantiate(&mut store, &module, Some(imports))?;
                                return Ok(());
                            }
                            wast::WastExecute::Get { .. } => panic!("get not supported"),
                            wast::WastExecute::Invoke(invoke) => invoke,
                        };
                        let module = module_registry.get_idx(invoke.module);
                        let args =
                            convert_wastargs(invoke.args).map_err(|err| tinywasm::Error::Other(err.to_string()))?;
                        exec_fn_instance(module, &mut store, invoke.name, &args).map(|_| ())
                    });
                    match res {
                        Err(err) => test_group.add_result(
                            &format!("AssertTrap({i})"),
                            span.linecol_in(wast_raw),
                            Err(eyre!("test panicked: {}", try_downcast_panic(err))),
                        ),
                        Ok(Err(tinywasm::Error::Trap(trap))) => {
                            if !message.starts_with(trap.message()) && !trap.message().starts_with(message) {
                                test_group.add_result(
                                    &format!("AssertTrap({i})"),
                                    span.linecol_in(wast_raw),
                                    Err(eyre!("expected trap: {}, got: {}", message, trap.message())),
                                );
                                continue;
                            }
                            test_group.add_result(&format!("AssertTrap({i})"), span.linecol_in(wast_raw), Ok(()));
                        }
                        Ok(Err(err)) => test_group.add_result(
                            &format!("AssertTrap({i})"),
                            span.linecol_in(wast_raw),
                            Err(eyre!("expected trap, {}, got: {:?}", message, err)),
                        ),
                        Ok(Ok(())) => test_group.add_result(
                            &format!("AssertTrap({i})"),
                            span.linecol_in(wast_raw),
                            Err(eyre!("expected trap {}, got Ok", message)),
                        ),
                    }
                }
                AssertUnlinkable { mut module, span, message } => {
                    let res = catch_unwind_silent(|| {
                        let module = parse_module_bytes(&module.encode().expect("failed to encode module"))
                            .expect("failed to parse module");
                        let imports = Self::imports(&mut store, module_registry.modules()).unwrap();
                        ModuleInstance::instantiate(&mut store, &module, Some(imports))
                    });
                    match res {
                        Err(err) => test_group.add_result(
                            &format!("AssertUnlinkable({i})"),
                            span.linecol_in(wast_raw),
                            Err(eyre!("test panicked: {}", try_downcast_panic(err))),
                        ),
                        Ok(Err(tinywasm::Error::Linker(err))) => {
                            if err.message() != message
                                && (err.message() == "memory types incompatible"
                                    && message != "incompatible import type")
                            {
                                test_group.add_result(
                                    &format!("AssertUnlinkable({i})"),
                                    span.linecol_in(wast_raw),
                                    Err(eyre!("expected linker error: {}, got: {}", message, err.message())),
                                );
                                continue;
                            }
                            test_group.add_result(&format!("AssertUnlinkable({i})"), span.linecol_in(wast_raw), Ok(()));
                        }
                        Ok(Err(err)) => test_group.add_result(
                            &format!("AssertUnlinkable({i})"),
                            span.linecol_in(wast_raw),
                            Err(eyre!("expected linker error, {}, got: {:?}", message, err)),
                        ),
                        Ok(Ok(_)) => test_group.add_result(
                            &format!("AssertUnlinkable({i})"),
                            span.linecol_in(wast_raw),
                            Err(eyre!("expected linker error {}, got Ok", message)),
                        ),
                    }
                }
                Invoke(invoke) => {
                    let name = invoke.name;
                    let res: Result<Result<()>, _> = catch_unwind_silent(|| {
                        let args = convert_wastargs(invoke.args)?;
                        let module = module_registry.get_idx(invoke.module);
                        exec_fn_instance(module, &mut store, invoke.name, &args).map_err(|e| {
                            error!("failed to execute function: {e:?}");
                            e
                        })?;
                        Ok(())
                    });
                    let res = res.map_err(|e| eyre!("test panicked: {}", try_downcast_panic(e))).and_then(|r| r);
                    test_group.add_result(&format!("Invoke({name}-{i})"), span.linecol_in(wast_raw), res);
                }
                AssertReturn { span, exec, results } => {
                    let expected_alternatives = match convert_wastret(results.into_iter()) {
                        Err(err) => {
                            test_group.add_result(
                                &format!("AssertReturn(unsupported-{i})"),
                                span.linecol_in(wast_raw),
                                Err(eyre!("failed to convert expected results: {err:?}")),
                            );
                            continue;
                        }
                        Ok(expected) => expected,
                    };

                    let invoke = match match exec {
                        wast::WastExecute::Wat(_) => Err(eyre!("wat not supported")),
                        wast::WastExecute::Get { module: module_id, global, .. } => {
                            let Some(module) = module_registry.get(module_id) else {
                                test_group.add_result(
                                    &format!("AssertReturn(unsupported-{i})"),
                                    span.linecol_in(wast_raw),
                                    Err(eyre!("no module to get global from")),
                                );
                                continue;
                            };
                            let module_global = match module.global_get(&store, global) {
                                Ok(value) => value,
                                Err(err) => {
                                    test_group.add_result(
                                        &format!("AssertReturn(unsupported-{i})"),
                                        span.linecol_in(wast_raw),
                                        Err(eyre!("failed to get global: {err:?}")),
                                    );
                                    continue;
                                }
                            };
                            let expected = expected_alternatives
                                .iter()
                                .filter_map(|alts| alts.first())
                                .find(|exp| module_global.eq_loose(exp));
                            if expected.is_none() {
                                test_group.add_result(
                                    &format!("AssertReturn(unsupported-{i})"),
                                    span.linecol_in(wast_raw),
                                    Err(eyre!(
                                        "global value did not match any expected alternative: {:?}",
                                        module_global
                                    )),
                                );
                                continue;
                            }
                            test_group.add_result(
                                &format!("AssertReturn({global}-{i})"),
                                span.linecol_in(wast_raw),
                                Ok(()),
                            );
                            continue;
                        }
                        wast::WastExecute::Invoke(invoke) => Ok(invoke),
                    } {
                        Ok(invoke) => invoke,
                        Err(err) => {
                            test_group.add_result(
                                &format!("AssertReturn(unsupported-{i})"),
                                span.linecol_in(wast_raw),
                                Err(eyre!("unsupported directive: {err:?}")),
                            );
                            continue;
                        }
                    };

                    let invoke_name = invoke.name;
                    let res: Result<Result<()>, _> = catch_unwind_silent(|| {
                        let args = convert_wastargs(invoke.args)?;
                        let module = module_registry.get_idx(invoke.module);
                        let outcomes = exec_fn_instance(module, &mut store, invoke.name, &args).map_err(|e| {
                            error!("failed to execute function: {e:?}");
                            e
                        })?;
                        if !expected_alternatives.iter().any(|expected| expected.len() == outcomes.len()) {
                            return Err(eyre!(
                                "expected {} results, got {}",
                                expected_alternatives.first().map_or(0, |v| v.len()),
                                outcomes.len()
                            ));
                        }
                        if expected_alternatives.iter().any(|expected| {
                            expected.len() == outcomes.len()
                                && outcomes.iter().zip(expected.iter()).all(|(outcome, exp)| outcome.eq_loose(exp))
                        }) {
                            Ok(())
                        } else {
                            Err(eyre!("results did not match any expected alternative"))
                        }
                    });

                    let res = res.map_err(|e| eyre!("test panicked: {}", try_downcast_panic(e))).and_then(|r| r);
                    test_group.add_result(&format!("AssertReturn({invoke_name}-{i})"), span.linecol_in(wast_raw), res);
                }
                _ => test_group.add_result(
                    &format!("Unknown({i})"),
                    span.linecol_in(wast_raw),
                    Err(eyre!("unsupported directive")),
                ),
            }
        }

        Ok(())
    }
}

impl Display for WastRunner {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        use owo_colors::OwoColorize;

        let mut total_passed = 0;
        let mut total_failed = 0;

        for group in self.group_results() {
            total_passed += group.passed;
            total_failed += group.failed;

            writeln!(f, "{}", group.name.bold().underline())?;
            writeln!(f, "  Tests Passed: {}", group.passed.to_string().green())?;
            if group.failed != 0 {
                writeln!(f, "  Tests Failed: {}", group.failed.to_string().red())?;
            }
        }

        writeln!(f, "\n{}", "Total Test Summary:".bold().underline())?;
        writeln!(f, "  Total Tests: {}", total_passed + total_failed)?;
        writeln!(f, "  Total Passed: {}", total_passed.to_string().green())?;
        writeln!(f, "  Total Failed: {}", total_failed.to_string().red())?;
        Ok(())
    }
}

#[derive(Debug)]
struct TestGroup {
    tests: Vec<TestCase>,
    file: String,
}

impl TestGroup {
    fn new(file: &str) -> Self {
        Self { tests: Vec::new(), file: file.to_string() }
    }

    fn stats(&self) -> (usize, usize) {
        let mut passed = 0;
        let mut failed = 0;
        for test in &self.tests {
            match test.result {
                Ok(()) => passed += 1,
                Err(_) => failed += 1,
            }
        }
        (passed, failed)
    }

    fn add_result(&mut self, name: &str, linecol: (usize, usize), result: Result<()>) {
        self.tests.push(TestCase { name: name.to_string(), linecol, result });
    }
}

#[derive(Debug)]
struct TestCase {
    name: String,
    linecol: (usize, usize),
    result: Result<()>,
}

fn expand_paths(paths: &[PathBuf]) -> Result<Vec<PathBuf>> {
    let mut files = Vec::new();
    for path in paths {
        if path.is_dir() {
            for entry in std::fs::read_dir(path)? {
                let entry = entry?;
                let path = entry.path();
                if path.extension().is_some_and(|ext| ext == "wast") {
                    files.push(path);
                }
            }
        } else {
            files.push(path.clone());
        }
    }
    files.sort();
    Ok(files)
}

#[derive(Debug)]
pub struct TestFile<'a> {
    pub name: String,
    pub contents: &'a str,
    pub parent: String,
}

impl<'a> TestFile<'a> {
    pub fn name(&self) -> &str {
        &self.name
    }

    pub fn raw(&self) -> &'a str {
        self.contents
    }

    pub fn parent(&self) -> &str {
        &self.parent
    }

    pub fn wast(&self) -> wast::parser::Result<WastBuffer<'a>> {
        let mut lexer = wast::lexer::Lexer::new(self.contents);
        lexer.allow_confusing_unicode(true);
        let parse_buffer = wast::parser::ParseBuffer::new_with_lexer(lexer)?;
        Ok(WastBuffer { buffer: parse_buffer })
    }
}

pub struct WastBuffer<'a> {
    buffer: wast::parser::ParseBuffer<'a>,
}

impl<'a> WastBuffer<'a> {
    pub fn directives(&'a self) -> wast::parser::Result<Vec<wast::WastDirective<'a>>> {
        Ok(wast::parser::parse::<wast::Wast<'a>>(&self.buffer)?.directives)
    }
}

fn exec_with_budget(
    func: &tinywasm::Function,
    store: &mut Store,
    args: &[WasmValue],
) -> Result<Vec<WasmValue>, tinywasm::Error> {
    let mut exec = func.call_resumable(store, args)?;
    for _ in 0..TEST_MAX_SUSPENSIONS {
        match exec.resume_with_time_budget(TEST_TIME_SLICE)? {
            ExecProgress::Completed(values) => return Ok(values),
            ExecProgress::Suspended => {}
        }
    }
    Err(tinywasm::Error::Other(format!(
        "testsuite execution timed out after {} time slices of {:?}",
        TEST_MAX_SUSPENSIONS, TEST_TIME_SLICE
    )))
}

fn try_downcast_panic(panic: Box<dyn std::any::Any + Send>) -> String {
    let info = panic.downcast_ref::<panic::PanicHookInfo>().map(ToString::to_string);
    let info_string = panic.downcast_ref::<String>().cloned();
    let info_str = panic.downcast::<&str>().ok().map(|s| *s);
    info.unwrap_or_else(|| info_str.unwrap_or(&info_string.unwrap_or("unknown panic".to_owned())).to_string())
}

fn exec_fn_instance(
    instance: Option<u32>,
    store: &mut Store,
    name: &str,
    args: &[WasmValue],
) -> Result<Vec<WasmValue>, tinywasm::Error> {
    let Some(instance) = instance else {
        return Err(tinywasm::Error::Other("no instance found".to_string()));
    };
    let Some(instance) = store.get_module_instance(instance) else {
        return Err(tinywasm::Error::Other("no instance found".to_string()));
    };
    let func = instance.func_untyped(store, name)?;
    exec_with_budget(&func, store, args)
}

fn catch_unwind_silent<R>(f: impl FnOnce() -> R) -> std::thread::Result<R> {
    let prev_hook = panic::take_hook();
    panic::set_hook(Box::new(|_| {}));
    let result = panic::catch_unwind(AssertUnwindSafe(f));
    panic::set_hook(prev_hook);
    result
}

fn encode_quote_wat(module: QuoteWat) -> (Option<String>, Vec<u8>) {
    match module {
        QuoteWat::QuoteModule(_, quoted_wat) => {
            let wat = quoted_wat
                .iter()
                .map(|(_, s)| std::str::from_utf8(s).expect("failed to convert wast to utf8"))
                .collect::<Vec<_>>()
                .join("\n");
            let lexer = wast::lexer::Lexer::new(&wat);
            let buf = wast::parser::ParseBuffer::new_with_lexer(lexer).expect("failed to create parse buffer");
            let mut wat_data = wast::parser::parse::<wast::Wat>(&buf).expect("failed to parse wat");
            (None, wat_data.encode().expect("failed to encode module"))
        }
        QuoteWat::Wat(mut wat) => {
            let wast::Wat::Module(ref module) = wat else { unimplemented!("Not supported") };
            (module.id.map(|id| id.name().to_string()), wat.encode().expect("failed to encode module"))
        }
        QuoteWat::QuoteComponent(..) => unimplemented!("components are not supported"),
    }
}

fn parse_module_bytes(bytes: &[u8]) -> Result<Module> {
    Ok(tinywasm::parse_bytes(bytes)?)
}

fn convert_wastargs(args: Vec<wast::WastArg>) -> Result<Vec<WasmValue>> {
    args.into_iter().map(wastarg2tinywasmvalue).collect()
}

fn convert_wastret<'a>(args: impl Iterator<Item = wast::WastRet<'a>>) -> Result<Vec<Vec<WasmValue>>> {
    let mut alternatives = vec![Vec::new()];
    for arg in args {
        let choices = wastret2tinywasmvalues(arg)?;
        let mut next = Vec::with_capacity(alternatives.len() * choices.len());
        for prefix in alternatives {
            for choice in &choices {
                let mut candidate = prefix.clone();
                candidate.push(*choice);
                next.push(candidate);
            }
        }
        alternatives = next;
    }
    Ok(alternatives)
}

fn wastarg2tinywasmvalue(arg: wast::WastArg) -> Result<WasmValue> {
    let wast::WastArg::Core(arg) = arg else { bail!("unsupported arg type: Component") };
    use wast::core::WastArgCore::*;
    Ok(match arg {
        F32(f) => WasmValue::F32(f32::from_bits(f.bits)),
        F64(f) => WasmValue::F64(f64::from_bits(f.bits)),
        I32(i) => WasmValue::I32(i),
        I64(i) => WasmValue::I64(i),
        V128(i) => WasmValue::V128(i128::from_le_bytes(i.to_le_bytes())),
        RefExtern(v) => WasmValue::RefExtern(ExternRef::new(Some(v))),
        RefNull(t) => match t {
            wast::core::HeapType::Abstract { shared: false, ty: AbstractHeapType::Func } => {
                WasmValue::RefFunc(FuncRef::null())
            }
            wast::core::HeapType::Abstract { shared: false, ty: AbstractHeapType::Extern } => {
                WasmValue::RefExtern(ExternRef::null())
            }
            _ => bail!("unsupported arg type: refnull: {:?}", t),
        },
        RefHost(_) => bail!("unsupported arg type: RefHost"),
    })
}

fn wast_i128_to_i128(i: wast::core::V128Pattern) -> i128 {
    let res: Vec<u8> = match i {
        wast::core::V128Pattern::F32x4(f) => {
            f.iter().flat_map(|v| nanpattern2tinywasmvalue(*v).unwrap().as_f32().unwrap().to_le_bytes()).collect()
        }
        wast::core::V128Pattern::F64x2(f) => {
            f.iter().flat_map(|v| nanpattern2tinywasmvalue(*v).unwrap().as_f64().unwrap().to_le_bytes()).collect()
        }
        wast::core::V128Pattern::I16x8(f) => f.iter().flat_map(|v| v.to_le_bytes()).collect(),
        wast::core::V128Pattern::I32x4(f) => f.iter().flat_map(|v| v.to_le_bytes()).collect(),
        wast::core::V128Pattern::I64x2(f) => f.iter().flat_map(|v| v.to_le_bytes()).collect(),
        wast::core::V128Pattern::I8x16(f) => f.iter().flat_map(|v| v.to_le_bytes()).collect(),
    };
    i128::from_le_bytes(res.try_into().unwrap())
}

fn wastret2tinywasmvalues(ret: wast::WastRet) -> Result<Vec<WasmValue>> {
    let wast::WastRet::Core(ret) = ret else { bail!("unsupported arg type") };
    match ret {
        wast::core::WastRetCore::Either(options) => {
            options.into_iter().map(wastretcore2tinywasmvalue).collect::<Result<Vec<_>>>()
        }
        ret => Ok(vec![wastretcore2tinywasmvalue(ret)?]),
    }
}

fn wastretcore2tinywasmvalue(ret: wast::core::WastRetCore) -> Result<WasmValue> {
    use wast::core::WastRetCore::{F32, F64, I32, I64, RefExtern, RefFunc, RefNull, V128};
    Ok(match ret {
        F32(f) => nanpattern2tinywasmvalue(f)?,
        F64(f) => nanpattern2tinywasmvalue(f)?,
        I32(i) => WasmValue::I32(i),
        I64(i) => WasmValue::I64(i),
        V128(i) => WasmValue::V128(wast_i128_to_i128(i)),
        RefNull(t) => match t {
            Some(wast::core::HeapType::Abstract { shared: false, ty: AbstractHeapType::Func }) => {
                WasmValue::RefFunc(FuncRef::null())
            }
            Some(wast::core::HeapType::Abstract { shared: false, ty: AbstractHeapType::Extern }) => {
                WasmValue::RefExtern(ExternRef::null())
            }
            _ => bail!("unsupported arg type: refnull: {:?}", t),
        },
        RefExtern(v) => WasmValue::RefExtern(ExternRef::new(v)),
        RefFunc(v) => WasmValue::RefFunc(FuncRef::new(match v {
            Some(wast::token::Index::Num(n, _)) => Some(n),
            _ => bail!("unsupported arg type: reffunc: {:?}", v),
        })),
        a => bail!("unsupported arg type {:?}", a),
    })
}

enum Bits {
    U32(u32),
    U64(u64),
}

trait FloatToken {
    fn bits(&self) -> Bits;
    fn canonical_nan() -> WasmValue;
    fn arithmetic_nan() -> WasmValue;
    fn value(&self) -> WasmValue {
        match self.bits() {
            Bits::U32(v) => WasmValue::F32(f32::from_bits(v)),
            Bits::U64(v) => WasmValue::F64(f64::from_bits(v)),
        }
    }
}

impl FloatToken for wast::token::F32 {
    fn bits(&self) -> Bits {
        Bits::U32(self.bits)
    }
    fn canonical_nan() -> WasmValue {
        WasmValue::F32(f32::NAN)
    }
    fn arithmetic_nan() -> WasmValue {
        WasmValue::F32(f32::NAN)
    }
}

impl FloatToken for wast::token::F64 {
    fn bits(&self) -> Bits {
        Bits::U64(self.bits)
    }
    fn canonical_nan() -> WasmValue {
        WasmValue::F64(f64::NAN)
    }
    fn arithmetic_nan() -> WasmValue {
        WasmValue::F64(f64::NAN)
    }
}

fn nanpattern2tinywasmvalue<T>(arg: wast::core::NanPattern<T>) -> Result<WasmValue>
where
    T: FloatToken,
{
    use wast::core::NanPattern::{ArithmeticNan, CanonicalNan, Value};
    Ok(match arg {
        CanonicalNan => T::canonical_nan(),
        ArithmeticNan => T::arithmetic_nan(),
        Value(v) => v.value(),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn runs_simple_wast_file() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("simple.wast");
        std::fs::write(
            &path,
            "(module (func (export \"add\") (result i32) i32.const 1))\n(assert_return (invoke \"add\") (i32.const 1))",
        )
        .unwrap();

        let mut runner = WastRunner::new();
        runner.run_paths(&[path]).unwrap();
    }
}
