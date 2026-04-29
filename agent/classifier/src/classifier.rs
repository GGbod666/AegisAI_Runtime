use std::collections::BTreeSet;

use crate::config::ClassifierConfig;
use crate::config::ConfigError;
use crate::model::{ClassifierOptions, ProcessRule, ProcessSnapshot, WorkloadProfile, WorkloadTag};

#[derive(Clone, Debug, Default)]
pub struct Classifier {
    rules: Vec<ProcessRule>,
    options: ClassifierOptions,
}

impl Classifier {
    pub fn new(rules: Vec<ProcessRule>) -> Self {
        Self::with_options(rules, ClassifierOptions::default())
    }

    pub fn with_options(rules: Vec<ProcessRule>, options: ClassifierOptions) -> Self {
        Self { rules, options }
    }

    pub fn from_config(config: ClassifierConfig) -> Self {
        Self::new(config.process_rules)
    }

    pub fn from_config_path(path: impl AsRef<std::path::Path>) -> Result<Self, ConfigError> {
        let config = ClassifierConfig::from_path(path)?;
        Ok(Self::from_config(config))
    }

    pub fn rules(&self) -> &[ProcessRule] {
        &self.rules
    }

    pub fn options(&self) -> &ClassifierOptions {
        &self.options
    }

    pub fn classify_process(&self, snapshot: &ProcessSnapshot) -> WorkloadProfile {
        let mut tags = BTreeSet::<WorkloadTag>::new();
        let mut matched_rules = Vec::new();

        for rule in &self.rules {
            if rule.matches_with_options(snapshot, &self.options) {
                tags.extend(rule.tags.iter().cloned());
                matched_rules.push(rule.id.clone());
            }
        }

        WorkloadProfile::from_process(snapshot, tags, matched_rules)
    }
}
