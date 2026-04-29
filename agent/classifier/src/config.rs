use std::collections::BTreeSet;
use std::fmt;
use std::fs;
use std::path::Path;

use crate::model::{ProcessRule, WorkloadTag};

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct ClassifierConfig {
    pub process_rules: Vec<ProcessRule>,
}

impl ClassifierConfig {
    pub fn from_path(path: impl AsRef<Path>) -> Result<Self, ConfigError> {
        let source = fs::read_to_string(path).map_err(ConfigError::Io)?;
        Self::from_toml_str(&source)
    }

    pub fn from_toml_str(source: &str) -> Result<Self, ConfigError> {
        let mut process_rules = Vec::new();
        let mut current_rule: Option<ProcessRule> = None;

        for (index, raw_line) in source.lines().enumerate() {
            let line_number = index + 1;
            let line = strip_comments(raw_line);
            let line = line.trim();

            if line.is_empty() {
                continue;
            }

            if line == "[[process_rules]]" {
                if let Some(rule) = current_rule.take() {
                    process_rules.push(rule);
                }

                current_rule = Some(ProcessRule::default());
                continue;
            }

            let Some(rule) = current_rule.as_mut() else {
                return Err(ConfigError::Syntax {
                    line: line_number,
                    message: "expected [[process_rules]] before key/value entries".to_string(),
                });
            };

            let Some((key, value)) = split_key_value(line) else {
                return Err(ConfigError::Syntax {
                    line: line_number,
                    message: format!("invalid key/value pair: {line}"),
                });
            };

            apply_rule_field(rule, key, value, line_number)?;
        }

        if let Some(rule) = current_rule.take() {
            process_rules.push(rule);
        }

        let mut config = Self { process_rules };
        config.validate()?;
        Ok(config)
    }

    fn validate(&mut self) -> Result<(), ConfigError> {
        for (index, rule) in self.process_rules.iter_mut().enumerate() {
            if rule.id.trim().is_empty() {
                rule.id = infer_rule_id(index + 1, rule);
            }

            if !rule.has_matchers() {
                return Err(ConfigError::Validation {
                    rule_id: rule.id.clone(),
                    message: "rule must declare at least one matcher".to_string(),
                });
            }

            if rule.tags.is_empty() {
                return Err(ConfigError::Validation {
                    rule_id: rule.id.clone(),
                    message: "rule must emit at least one workload tag".to_string(),
                });
            }
        }

        Ok(())
    }
}

#[derive(Debug)]
pub enum ConfigError {
    Io(std::io::Error),
    Syntax { line: usize, message: String },
    Validation { rule_id: String, message: String },
}

impl fmt::Display for ConfigError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Io(error) => write!(f, "failed to read classifier config: {error}"),
            Self::Syntax { line, message } => {
                write!(
                    f,
                    "classifier config syntax error on line {line}: {message}"
                )
            }
            Self::Validation { rule_id, message } => {
                write!(
                    f,
                    "classifier config validation error for rule {rule_id}: {message}"
                )
            }
        }
    }
}

impl std::error::Error for ConfigError {}

fn apply_rule_field(
    rule: &mut ProcessRule,
    key: &str,
    value: &str,
    line_number: usize,
) -> Result<(), ConfigError> {
    match key {
        "id" => rule.id = parse_string(value, line_number, key)?,
        "name" | "process_name" => rule.process_name = Some(parse_string(value, line_number, key)?),
        "cmdline_contains" => rule.cmdline_contains = Some(parse_string(value, line_number, key)?),
        "cgroup_contains" => rule.cgroup_contains = Some(parse_string(value, line_number, key)?),
        "pids" | "pid_allowlist" => {
            rule.pid_allowlist = parse_u32_array(value, line_number, key)?;
        }
        "tag_markers" => {
            rule.tag_markers = parse_string_array(value, line_number, key)?
                .into_iter()
                .collect();
        }
        "parent_name" | "parent_process_name" => {
            rule.parent_process_name = Some(parse_string(value, line_number, key)?)
        }
        "parent_cmdline_contains" => {
            rule.parent_cmdline_contains = Some(parse_string(value, line_number, key)?)
        }
        "parent_has_any_tags" => {
            rule.parent_has_any_tags = parse_tag_array(value, line_number, key)?;
        }
        "tags" => {
            rule.tags = parse_tag_array(value, line_number, key)?;
        }
        other => {
            return Err(ConfigError::Syntax {
                line: line_number,
                message: format!("unsupported field `{other}`"),
            })
        }
    }

    Ok(())
}

fn parse_string(value: &str, line: usize, field: &str) -> Result<String, ConfigError> {
    let value = value.trim();
    if !(value.starts_with('"') && value.ends_with('"')) {
        return Err(ConfigError::Syntax {
            line,
            message: format!("field `{field}` expects a quoted string"),
        });
    }

    Ok(unescape_string(&value[1..value.len() - 1]))
}

fn parse_string_array(value: &str, line: usize, field: &str) -> Result<Vec<String>, ConfigError> {
    let items = parse_array_items(value, line, field)?;
    items
        .into_iter()
        .map(|item| parse_string(item.as_str(), line, field))
        .collect()
}

fn parse_u32_array(value: &str, line: usize, field: &str) -> Result<BTreeSet<u32>, ConfigError> {
    let items = parse_array_items(value, line, field)?;
    let mut parsed = BTreeSet::new();

    for item in items {
        let number = item
            .trim()
            .parse::<u32>()
            .map_err(|error| ConfigError::Syntax {
                line,
                message: format!("field `{field}` expects integer values: {error}"),
            })?;
        parsed.insert(number);
    }

    Ok(parsed)
}

fn parse_tag_array(
    value: &str,
    line: usize,
    field: &str,
) -> Result<BTreeSet<WorkloadTag>, ConfigError> {
    Ok(parse_string_array(value, line, field)?
        .into_iter()
        .map(|entry| WorkloadTag::parse(&entry))
        .collect())
}

fn parse_array_items(value: &str, line: usize, field: &str) -> Result<Vec<String>, ConfigError> {
    let value = value.trim();
    if !(value.starts_with('[') && value.ends_with(']')) {
        return Err(ConfigError::Syntax {
            line,
            message: format!("field `{field}` expects an array"),
        });
    }

    let inner = &value[1..value.len() - 1];
    let mut items = Vec::new();
    let mut current = String::new();
    let mut in_quotes = false;
    let mut escaped = false;

    for ch in inner.chars() {
        match ch {
            '"' if !escaped => {
                in_quotes = !in_quotes;
                current.push(ch);
            }
            ',' if !in_quotes => {
                let item = current.trim();
                if !item.is_empty() {
                    items.push(item.to_string());
                }
                current.clear();
            }
            '\\' if in_quotes => {
                escaped = !escaped;
                current.push(ch);
                continue;
            }
            _ => {
                current.push(ch);
            }
        }

        if ch != '\\' {
            escaped = false;
        }
    }

    if in_quotes {
        return Err(ConfigError::Syntax {
            line,
            message: format!("field `{field}` contains an unterminated string"),
        });
    }

    let tail = current.trim();
    if !tail.is_empty() {
        items.push(tail.to_string());
    }

    Ok(items)
}

fn split_key_value(line: &str) -> Option<(&str, &str)> {
    let (key, value) = line.split_once('=')?;
    Some((key.trim(), value.trim()))
}

fn strip_comments(line: &str) -> String {
    let mut output = String::new();
    let mut in_quotes = false;
    let mut escaped = false;

    for ch in line.chars() {
        match ch {
            '"' if !escaped => {
                in_quotes = !in_quotes;
                output.push(ch);
            }
            '#' if !in_quotes => break,
            '\\' if in_quotes => {
                escaped = !escaped;
                output.push(ch);
                continue;
            }
            _ => output.push(ch),
        }

        if ch != '\\' {
            escaped = false;
        }
    }

    output
}

fn unescape_string(value: &str) -> String {
    let mut output = String::new();
    let mut escaped = false;

    for ch in value.chars() {
        if escaped {
            match ch {
                '\\' => output.push('\\'),
                '"' => output.push('"'),
                'n' => output.push('\n'),
                't' => output.push('\t'),
                other => output.push(other),
            }
            escaped = false;
            continue;
        }

        if ch == '\\' {
            escaped = true;
        } else {
            output.push(ch);
        }
    }

    if escaped {
        output.push('\\');
    }

    output
}

fn infer_rule_id(index: usize, rule: &ProcessRule) -> String {
    let hint = rule
        .process_name
        .as_ref()
        .or(rule.cmdline_contains.as_ref())
        .or(rule.parent_process_name.as_ref())
        .map(|value| sanitize_rule_name(value))
        .filter(|value| !value.is_empty())
        .unwrap_or_else(|| "rule".to_string());

    format!("process_rule_{index}_{hint}")
}

fn sanitize_rule_name(value: &str) -> String {
    value
        .chars()
        .map(|ch| {
            if ch.is_ascii_alphanumeric() {
                ch.to_ascii_lowercase()
            } else {
                '_'
            }
        })
        .collect::<String>()
        .trim_matches('_')
        .to_string()
}
