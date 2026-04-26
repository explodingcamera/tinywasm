use clap::{Args, ValueEnum};
use eyre::{Result, bail};
use tinywasm::{Engine, StackConfig, engine::FuelPolicy};

#[derive(Args, Clone, Default)]
pub struct EngineFlags {
    /// Fuel accounting policy for budgeted execution APIs
    #[arg(long, value_enum)]
    pub fuel_policy: Option<FuelPolicyArg>,

    /// Trap immediately on memory or stack allocation failure
    #[arg(long)]
    pub trap_on_oom: bool,

    /// Memory backend to use for instantiated memories
    #[arg(long, value_enum)]
    pub memory_backend: Option<MemoryBackendArg>,

    /// Chunk size in bytes for the paged memory backend
    #[arg(long, default_value_t = 64 * 1024)]
    pub memory_page_chunk_size: usize,

    /// Fixed value stack size for all value lanes
    #[arg(long, conflicts_with = "value_stack_dynamic")]
    pub value_stack_size: Option<usize>,

    /// Dynamic value stack config in initial:max form for all value lanes
    #[arg(long, value_name = "INITIAL:MAX", conflicts_with = "value_stack_size")]
    pub value_stack_dynamic: Option<StackSpec>,

    /// Fixed call stack size in frames
    #[arg(long, conflicts_with = "call_stack_dynamic")]
    pub call_stack_size: Option<usize>,

    /// Dynamic call stack config in initial:max form
    #[arg(long, value_name = "INITIAL:MAX", conflicts_with = "call_stack_size")]
    pub call_stack_dynamic: Option<StackSpec>,
}

#[derive(Clone, Copy, ValueEnum)]
pub enum FuelPolicyArg {
    PerInstruction,
    Weighted,
}

#[derive(Clone, Copy, ValueEnum)]
pub enum MemoryBackendArg {
    Vec,
    Paged,
}

#[derive(Clone)]
pub struct StackSpec {
    initial: usize,
    max: usize,
}

impl core::str::FromStr for StackSpec {
    type Err = String;

    fn from_str(s: &str) -> std::result::Result<Self, Self::Err> {
        let (initial, max) = s.split_once(':').ok_or_else(|| "expected INITIAL:MAX".to_string())?;
        let initial = initial.parse::<usize>().map_err(|e| format!("invalid initial stack size: {e}"))?;
        let max = max.parse::<usize>().map_err(|e| format!("invalid max stack size: {e}"))?;
        if initial > max {
            return Err("initial stack size must be less than or equal to max stack size".to_string());
        }
        Ok(Self { initial, max })
    }
}

impl StackSpec {
    fn into_stack_config(self) -> StackConfig {
        StackConfig::dynamic(self.initial, self.max)
    }
}

impl EngineFlags {
    pub fn build_engine(&self) -> Result<Engine> {
        let mut config = tinywasm::engine::Config::new();

        if let Some(fuel_policy) = self.fuel_policy {
            config = config.with_fuel_policy(match fuel_policy {
                FuelPolicyArg::PerInstruction => FuelPolicy::PerInstruction,
                FuelPolicyArg::Weighted => FuelPolicy::Weighted,
            });
        }

        if let Some(memory_backend) = self.memory_backend {
            config = config.with_memory_backend(match memory_backend {
                MemoryBackendArg::Vec => tinywasm::MemoryBackend::vec(),
                MemoryBackendArg::Paged => {
                    if self.memory_page_chunk_size == 0 {
                        bail!("--memory-page-chunk-size must be greater than zero")
                    }
                    tinywasm::MemoryBackend::paged(self.memory_page_chunk_size)
                }
            });
        }

        if let Some(value_stack_size) = self.value_stack_size {
            config = config.with_value_stack(StackConfig::fixed(value_stack_size));
        }

        if let Some(value_stack_dynamic) = self.value_stack_dynamic.clone() {
            config = config.with_value_stack(value_stack_dynamic.into_stack_config());
        }

        if let Some(call_stack_size) = self.call_stack_size {
            config = config.with_call_stack(StackConfig::fixed(call_stack_size));
        }

        if let Some(call_stack_dynamic) = self.call_stack_dynamic.clone() {
            config = config.with_call_stack(call_stack_dynamic.into_stack_config());
        }

        if self.trap_on_oom {
            config = config.with_trap_on_oom(true);
        }

        Ok(Engine::new(config))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_stack_spec() {
        let spec: StackSpec = "16:64".parse().unwrap();
        let cfg = spec.into_stack_config();
        assert_eq!(cfg.initial_size, 16);
        assert_eq!(cfg.max_size, 64);
        assert!(cfg.dynamic);
    }

    #[test]
    fn builds_dynamic_stack_engine() {
        let flags = EngineFlags {
            value_stack_dynamic: Some("8:32".parse().unwrap()),
            call_stack_dynamic: Some("4:12".parse().unwrap()),
            ..Default::default()
        };

        let engine = flags.build_engine().unwrap();
        assert!(engine.config().value_stack_32.dynamic);
        assert_eq!(engine.config().value_stack_32.initial_size, 8);
        assert_eq!(engine.config().value_stack_32.max_size, 32);
        assert!(engine.config().call_stack.dynamic);
        assert_eq!(engine.config().call_stack.initial_size, 4);
        assert_eq!(engine.config().call_stack.max_size, 12);
    }
}
