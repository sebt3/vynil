use crate::linting::LintLevel;
use common::Result;
use serde::{Deserialize, Serialize};
use std::{
    collections::{HashMap, HashSet},
    path::Path,
};

#[derive(Deserialize, Default, Clone, Debug, Serialize)]
pub struct LintConfig {
    #[serde(default)]
    pub disable: Vec<String>,
    #[serde(default)]
    pub override_: HashMap<String, LintLevel>,
    #[serde(default)]
    pub files: Vec<FileRule>,
}

#[derive(Deserialize, Serialize, Clone, Debug)]
pub struct FileRule {
    pub glob: String,
    #[serde(default)]
    pub disable: Vec<String>,
    #[serde(default)]
    pub override_: HashMap<String, LintLevel>,
}

impl LintConfig {
    /// Load .vynil-lint.yaml from package_dir, return Default if absent
    pub fn load(package_dir: &Path) -> Result<Self> {
        let config_path = package_dir.join(".vynil-lint.yaml");
        if !config_path.exists() {
            return Ok(Self::default());
        }

        match std::fs::read_to_string(&config_path) {
            Ok(content) => match serde_yaml::from_str::<LintConfig>(&content) {
                Ok(config) => Ok(config),
                Err(e) => {
                    tracing::warn!("Invalid .vynil-lint.yaml: {}, using defaults", e);
                    Ok(Self::default())
                }
            },
            Err(e) => {
                tracing::warn!("Failed to read .vynil-lint.yaml: {}, using defaults", e);
                Ok(Self::default())
            }
        }
    }

    /// Resolve effective lint level for a rule on a file.
    /// Priority order (lowest to highest):
    /// 1. Default level
    /// 2. Global override
    /// 3. File glob override (first matching glob)
    /// 4. Inline disable (returns None)
    pub fn resolve_level(
        &self,
        rule: &str,
        file: &Path,
        default_level: LintLevel,
        inline_disabled: &HashSet<String>,
    ) -> Option<LintLevel> {
        // If inline disable contains this rule, suppress it
        if inline_disabled.contains(rule) {
            return None;
        }

        // Check if rule is globally disabled
        if self.disable.contains(&rule.to_string()) {
            return None;
        }

        let mut level = default_level;

        // Apply global override
        if let Some(&override_level) = self.override_.get(rule) {
            level = override_level;
        }

        // Apply file glob override (first matching)
        for file_rule in &self.files {
            if wildmatch::WildMatch::new(&file_rule.glob).matches(&file.to_string_lossy()) {
                if file_rule.disable.contains(&rule.to_string()) {
                    return None;
                }
                if let Some(&override_level) = file_rule.override_.get(rule) {
                    level = override_level;
                }
                break;
            }
        }

        Some(level)
    }
}

/// Parse inline disable comments from source code.
/// Returns a map of rule IDs disabled on each line.
/// Patterns:
/// - Rhai: `// vynil-lint-disable rule-a, rule-b`
/// - HBS:  `{{!-- vynil-lint-disable rule-a, rule-b --}}`
pub fn parse_inline_disables(source: &str) -> HashMap<usize, HashSet<String>> {
    let mut result: HashMap<usize, HashSet<String>> = HashMap::new();

    for (line_num, line) in source.lines().enumerate() {
        let line_number = line_num + 1; // 1-based

        // Try Rhai pattern: // vynil-lint-disable
        if let Some(pos) = line.find("// vynil-lint-disable") {
            let rest = &line[pos + 21..];
            let rules = parse_rules(rest);
            if !rules.is_empty() {
                result.insert(line_number, rules);
                continue;
            }
        }

        // Try HBS pattern: {{!-- vynil-lint-disable
        if let Some(pos) = line.find("{{!-- vynil-lint-disable") {
            let rest = &line[pos + 24..];
            let rules = parse_rules(rest);
            if !rules.is_empty() {
                result.insert(line_number, rules);
                continue;
            }
        }

        // Try YAML pattern: # vynil-lint-disable
        if let Some(pos) = line.find("# vynil-lint-disable") {
            let rest = &line[pos + 20..];
            let rules = parse_rules(rest);
            if !rules.is_empty() {
                result.insert(line_number, rules);
            }
        }
    }

    result
}

/// Parse comma-separated rule IDs from a string.
fn parse_rules(s: &str) -> HashSet<String> {
    s.split(',')
        .map(|r| r.trim())
        .filter(|r| !r.is_empty())
        .map(|r| {
            // Remove closing patterns if present
            r.trim_end_matches("--}}")
                .trim_end_matches("*/")
                .trim()
                .to_string()
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn load_returns_default_when_file_absent() {
        let config = LintConfig::load(Path::new("/nonexistent/path")).unwrap();
        assert!(config.disable.is_empty());
        assert!(config.override_.is_empty());
        assert!(config.files.is_empty());
    }

    #[test]
    fn global_disable_suppresses_finding() {
        let mut config = LintConfig::default();
        config.disable.push("hbs/unknown-helper".to_string());

        let inline_disabled = HashSet::new();
        let result = config.resolve_level(
            "hbs/unknown-helper",
            Path::new("test.hbs"),
            LintLevel::Error,
            &inline_disabled,
        );

        assert_eq!(result, None);
    }

    #[test]
    fn global_override_changes_level() {
        let mut config = LintConfig::default();
        config
            .override_
            .insert("hbs/unknown-value".to_string(), LintLevel::Warn);

        let inline_disabled = HashSet::new();
        let result = config.resolve_level(
            "hbs/unknown-value",
            Path::new("test.hbs"),
            LintLevel::Error,
            &inline_disabled,
        );

        assert_eq!(result, Some(LintLevel::Warn));
    }

    #[test]
    fn file_glob_override_takes_priority_over_global() {
        let mut config = LintConfig::default();
        config
            .override_
            .insert("hbs/unknown-partial".to_string(), LintLevel::Error);

        let mut file_rule = FileRule {
            glob: "handlebars/partials/**".to_string(),
            disable: vec![],
            override_: HashMap::new(),
        };
        file_rule
            .override_
            .insert("hbs/unknown-partial".to_string(), LintLevel::Warn);
        config.files.push(file_rule);

        let inline_disabled = HashSet::new();

        // File matching glob → Warn
        let result = config.resolve_level(
            "hbs/unknown-partial",
            Path::new("handlebars/partials/foo.hbs"),
            LintLevel::Error,
            &inline_disabled,
        );
        assert_eq!(result, Some(LintLevel::Warn));

        // File not matching glob → Error (global override)
        let result = config.resolve_level(
            "hbs/unknown-partial",
            Path::new("systems/bar.hbs"),
            LintLevel::Error,
            &inline_disabled,
        );
        assert_eq!(result, Some(LintLevel::Error));
    }

    #[test]
    fn inline_disable_returns_none() {
        let config = LintConfig::default();
        let mut inline_disabled = HashSet::new();
        inline_disabled.insert("test/rule".to_string());

        let result = config.resolve_level(
            "test/rule",
            Path::new("test.txt"),
            LintLevel::Error,
            &inline_disabled,
        );

        assert_eq!(result, None);
    }

    #[test]
    fn parse_rhai_inline_disable() {
        let src = "let x = 1; // vynil-lint-disable rhai/unused-variable\n";
        let map = parse_inline_disables(src);
        assert!(map.contains_key(&1));
        assert!(map[&1].contains("rhai/unused-variable"));
    }

    #[test]
    fn parse_hbs_inline_disable() {
        let src = "{{foo}}{{!-- vynil-lint-disable hbs/unknown-helper --}}\n";
        let map = parse_inline_disables(src);
        assert!(map.contains_key(&1));
        assert!(map[&1].contains("hbs/unknown-helper"));
    }

    #[test]
    fn parse_yaml_inline_disable() {
        let src = "size: # vynil-lint-disable hbs/unused-option\n";
        let map = parse_inline_disables(src);
        assert!(map.contains_key(&1));
        assert!(map[&1].contains("hbs/unused-option"));
    }
}
