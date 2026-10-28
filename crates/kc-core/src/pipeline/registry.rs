//! Stage plugin registry — build pipelines from named, third-party stages.
//!
//! The same extensible pattern the codec registry uses, but for pipeline stages:
//! a third party registers a named factory, and a pipeline is assembled from
//! configuration (names + params) rather than hard-coded types. Core never knows
//! what any stage does — it only maps a name to a factory and hands over a
//! generic [`StageConfig`]. This is the Phase 6 extensibility surface.
//!
//! # Example
//!
//! ```
//! use std::sync::Arc;
//! use kc_core::pipeline::{StageRegistry, StageConfig, PassthroughStage};
//!
//! let mut reg = StageRegistry::new();
//! reg.register("passthrough", |_cfg| Ok(Arc::new(PassthroughStage) as _));
//!
//! // Assemble a pipeline from config — names a caller could read from a file.
//! let pipeline = reg
//!     .build_pipeline(&[("passthrough", StageConfig::new())])
//!     .unwrap();
//! assert_eq!(pipeline.stage_count(), 1);
//! ```

use std::collections::HashMap;
use std::sync::Arc;

use super::{Pipeline, Stage};
use crate::error::{PipelineError, Result};

/// A single configuration value for a stage. Generic on purpose: core carries
/// these without interpreting them; the stage factory reads what it needs.
#[derive(Debug, Clone, PartialEq)]
pub enum ConfigValue {
    Str(String),
    Int(i64),
    Float(f64),
    Bool(bool),
}

impl From<&str> for ConfigValue {
    fn from(s: &str) -> Self {
        ConfigValue::Str(s.to_string())
    }
}
impl From<String> for ConfigValue {
    fn from(s: String) -> Self {
        ConfigValue::Str(s)
    }
}
impl From<i64> for ConfigValue {
    fn from(n: i64) -> Self {
        ConfigValue::Int(n)
    }
}
impl From<f64> for ConfigValue {
    fn from(n: f64) -> Self {
        ConfigValue::Float(n)
    }
}
impl From<bool> for ConfigValue {
    fn from(b: bool) -> Self {
        ConfigValue::Bool(b)
    }
}

/// A bag of named parameters handed to a stage factory. Build it fluently with
/// [`set`](Self::set); read it with the typed accessors.
#[derive(Debug, Clone, Default)]
pub struct StageConfig {
    values: HashMap<String, ConfigValue>,
}

impl StageConfig {
    /// An empty config.
    pub fn new() -> Self {
        Self::default()
    }

    /// Set a parameter (chainable).
    #[must_use]
    pub fn set(mut self, key: impl Into<String>, value: impl Into<ConfigValue>) -> Self {
        self.values.insert(key.into(), value.into());
        self
    }

    /// Raw value for `key`.
    pub fn get(&self, key: &str) -> Option<&ConfigValue> {
        self.values.get(key)
    }

    /// `key` as a string, if it is one.
    pub fn str(&self, key: &str) -> Option<&str> {
        match self.values.get(key) {
            Some(ConfigValue::Str(s)) => Some(s),
            _ => None,
        }
    }

    /// `key` as an integer, if it is one.
    pub fn int(&self, key: &str) -> Option<i64> {
        match self.values.get(key) {
            Some(ConfigValue::Int(n)) => Some(*n),
            _ => None,
        }
    }

    /// `key` as a float (also accepts an integer), if present.
    pub fn float(&self, key: &str) -> Option<f64> {
        match self.values.get(key) {
            Some(ConfigValue::Float(n)) => Some(*n),
            Some(ConfigValue::Int(n)) => Some(*n as f64),
            _ => None,
        }
    }

    /// `key` as a bool, if it is one.
    pub fn bool(&self, key: &str) -> Option<bool> {
        match self.values.get(key) {
            Some(ConfigValue::Bool(b)) => Some(*b),
            _ => None,
        }
    }

    /// How many parameters are set.
    pub fn len(&self) -> usize {
        self.values.len()
    }
    /// Whether no parameters are set.
    pub fn is_empty(&self) -> bool {
        self.values.is_empty()
    }
}

/// A factory that builds a stage from a [`StageConfig`].
type StageFactory = Arc<dyn Fn(&StageConfig) -> Result<Arc<dyn Stage>> + Send + Sync>;

/// A registry mapping stage names to factories.
///
/// Register once, then build stages (or whole pipelines) by name. Cloning is
/// cheap — the factories are behind `Arc` — so a configured registry can be
/// shared across threads.
#[derive(Default, Clone)]
pub struct StageRegistry {
    factories: HashMap<String, StageFactory>,
}

impl std::fmt::Debug for StageRegistry {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("StageRegistry")
            .field("stages", &self.names())
            .finish()
    }
}

impl StageRegistry {
    /// An empty registry.
    pub fn new() -> Self {
        Self::default()
    }

    /// Register a stage factory under `name`. A later registration with the same
    /// name replaces the earlier one.
    pub fn register<F>(&mut self, name: impl Into<String>, factory: F)
    where
        F: Fn(&StageConfig) -> Result<Arc<dyn Stage>> + Send + Sync + 'static,
    {
        self.factories.insert(name.into(), Arc::new(factory));
    }

    /// Build one stage by name, passing `config` to its factory.
    pub fn build(&self, name: &str, config: &StageConfig) -> Result<Arc<dyn Stage>> {
        let factory = self
            .factories
            .get(name)
            .ok_or_else(|| PipelineError::UnknownStage {
                name: name.to_string(),
            })?;
        factory(config)
    }

    /// Assemble a whole [`Pipeline`] from an ordered list of `(name, config)`.
    ///
    /// Each entry is looked up and built in order; an unknown name fails the
    /// whole build. This is what lets a pipeline be described by configuration
    /// (a file, a request) instead of hard-coded types.
    pub fn build_pipeline(&self, specs: &[(&str, StageConfig)]) -> Result<Pipeline> {
        let mut builder = Pipeline::builder();
        for (name, config) in specs {
            builder = builder.stage_arc(self.build(name, config)?);
        }
        builder.build()
    }

    /// Whether a name is registered.
    pub fn contains(&self, name: &str) -> bool {
        self.factories.contains_key(name)
    }

    /// The registered names (unordered).
    pub fn names(&self) -> Vec<&str> {
        self.factories.keys().map(String::as_str).collect()
    }

    /// How many stages are registered.
    pub fn len(&self) -> usize {
        self.factories.len()
    }
    /// Whether the registry is empty.
    pub fn is_empty(&self) -> bool {
        self.factories.is_empty()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::pipeline::{DropStage, PassthroughStage};

    fn passthrough_registry() -> StageRegistry {
        let mut reg = StageRegistry::new();
        reg.register("passthrough", |_cfg| Ok(Arc::new(PassthroughStage) as _));
        reg.register("drop", |_cfg| Ok(Arc::new(DropStage) as _));
        reg
    }

    #[test]
    fn registers_and_builds_by_name() {
        let reg = passthrough_registry();
        assert!(reg.contains("passthrough"));
        assert_eq!(reg.len(), 2);
        let stage = reg.build("passthrough", &StageConfig::new()).unwrap();
        assert_eq!(stage.name(), "passthrough");
    }

    #[test]
    fn unknown_stage_is_an_error() {
        let reg = passthrough_registry();
        let err = reg.build("nope", &StageConfig::new()).unwrap_err();
        assert!(matches!(
            err,
            crate::error::MediaError::Pipeline(PipelineError::UnknownStage { .. })
        ));
    }

    #[test]
    fn builds_a_pipeline_from_config() {
        let reg = passthrough_registry();
        let p = reg
            .build_pipeline(&[
                ("passthrough", StageConfig::new()),
                ("drop", StageConfig::new()),
            ])
            .unwrap();
        assert_eq!(p.stage_count(), 2);
        assert_eq!(p.stage_names(), vec!["passthrough", "drop"]);
    }

    #[test]
    fn factory_reads_typed_config() {
        // A factory that reads params proves core carries config without
        // interpreting it — the stage decides what the values mean.
        let cfg = StageConfig::new()
            .set("label", "x")
            .set("count", 5i64)
            .set("ratio", 0.5f64)
            .set("on", true);
        assert_eq!(cfg.str("label"), Some("x"));
        assert_eq!(cfg.int("count"), Some(5));
        assert_eq!(cfg.float("ratio"), Some(0.5));
        assert_eq!(cfg.float("count"), Some(5.0)); // int promotes to float
        assert_eq!(cfg.bool("on"), Some(true));
        assert_eq!(cfg.str("missing"), None);
    }

    #[test]
    fn re_registration_replaces() {
        let mut reg = StageRegistry::new();
        reg.register("s", |_| Ok(Arc::new(PassthroughStage) as _));
        reg.register("s", |_| Ok(Arc::new(DropStage) as _));
        assert_eq!(reg.len(), 1);
        assert_eq!(reg.build("s", &StageConfig::new()).unwrap().name(), "drop");
    }
}
