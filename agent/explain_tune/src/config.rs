use std::fmt;

#[derive(Clone, Debug, PartialEq)]
pub struct ExplainTuneConfig {
    pub improvement_threshold: f64,
    pub regression_threshold: f64,
    pub high_trigger_rate: f64,
    pub low_trigger_rate: f64,
    pub high_rollback_rate: f64,
    pub high_side_effect_rate: f64,
    pub threshold_step_ratio: f64,
    pub action_step_ratio: f64,
    pub max_trigger_explanations: usize,
}

impl Default for ExplainTuneConfig {
    fn default() -> Self {
        Self {
            improvement_threshold: 0.05,
            regression_threshold: 0.05,
            high_trigger_rate: 0.75,
            low_trigger_rate: 0.20,
            high_rollback_rate: 0.20,
            high_side_effect_rate: 0.25,
            threshold_step_ratio: 0.15,
            action_step_ratio: 0.20,
            max_trigger_explanations: 8,
        }
    }
}

impl ExplainTuneConfig {
    pub fn with_max_trigger_explanations(mut self, max_trigger_explanations: usize) -> Self {
        self.max_trigger_explanations = max_trigger_explanations;
        self
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ExplainTuneConfigError {
    InvalidImprovementThreshold,
    InvalidRegressionThreshold,
    InvalidHighTriggerRate,
    InvalidLowTriggerRate,
    InvalidTriggerRateOrdering,
    InvalidHighRollbackRate,
    InvalidHighSideEffectRate,
    InvalidThresholdStepRatio,
    InvalidActionStepRatio,
    InvalidMaxTriggerExplanations,
}

impl fmt::Display for ExplainTuneConfigError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::InvalidImprovementThreshold => {
                write!(f, "improvement_threshold must be greater than 0")
            }
            Self::InvalidRegressionThreshold => {
                write!(f, "regression_threshold must be greater than 0")
            }
            Self::InvalidHighTriggerRate => write!(f, "high_trigger_rate must be in [0, 1]"),
            Self::InvalidLowTriggerRate => write!(f, "low_trigger_rate must be in [0, 1]"),
            Self::InvalidTriggerRateOrdering => {
                write!(
                    f,
                    "low_trigger_rate must be less than or equal to high_trigger_rate"
                )
            }
            Self::InvalidHighRollbackRate => write!(f, "high_rollback_rate must be in [0, 1]"),
            Self::InvalidHighSideEffectRate => {
                write!(f, "high_side_effect_rate must be in [0, 1]")
            }
            Self::InvalidThresholdStepRatio => {
                write!(
                    f,
                    "threshold_step_ratio must be greater than 0 and at most 1"
                )
            }
            Self::InvalidActionStepRatio => {
                write!(f, "action_step_ratio must be greater than 0 and at most 1")
            }
            Self::InvalidMaxTriggerExplanations => {
                write!(f, "max_trigger_explanations must be greater than 0")
            }
        }
    }
}

impl std::error::Error for ExplainTuneConfigError {}

pub(crate) fn validate_config(
    config: &mut ExplainTuneConfig,
) -> Result<(), ExplainTuneConfigError> {
    if config.improvement_threshold <= 0.0 {
        return Err(ExplainTuneConfigError::InvalidImprovementThreshold);
    }
    if config.regression_threshold <= 0.0 {
        return Err(ExplainTuneConfigError::InvalidRegressionThreshold);
    }
    if !(0.0..=1.0).contains(&config.high_trigger_rate) {
        return Err(ExplainTuneConfigError::InvalidHighTriggerRate);
    }
    if !(0.0..=1.0).contains(&config.low_trigger_rate) {
        return Err(ExplainTuneConfigError::InvalidLowTriggerRate);
    }
    if config.low_trigger_rate > config.high_trigger_rate {
        return Err(ExplainTuneConfigError::InvalidTriggerRateOrdering);
    }
    if !(0.0..=1.0).contains(&config.high_rollback_rate) {
        return Err(ExplainTuneConfigError::InvalidHighRollbackRate);
    }
    if !(0.0..=1.0).contains(&config.high_side_effect_rate) {
        return Err(ExplainTuneConfigError::InvalidHighSideEffectRate);
    }
    if !(0.0..=1.0).contains(&config.threshold_step_ratio) || config.threshold_step_ratio == 0.0 {
        return Err(ExplainTuneConfigError::InvalidThresholdStepRatio);
    }
    if !(0.0..=1.0).contains(&config.action_step_ratio) || config.action_step_ratio == 0.0 {
        return Err(ExplainTuneConfigError::InvalidActionStepRatio);
    }
    if config.max_trigger_explanations == 0 {
        return Err(ExplainTuneConfigError::InvalidMaxTriggerExplanations);
    }

    Ok(())
}
