use std::{collections::HashMap, io::Cursor};

use chrono::Utc;
use flate2::{Compression, write::GzEncoder};
use serde::Serialize;
use tar::Builder;

use crate::{cli::InstanceTarget, transport::GetResult};

/// Metadata for a single collected item in the manifest.
#[derive(Debug, Serialize)]
pub struct ManifestItem {
    pub item: String,
    pub file: String,
    pub http_status: u16,
    pub content_type: String,
    pub bytes: usize,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub redactions: Option<RedactionCounts>,
    #[serde(skip_serializing_if = "is_false")]
    pub truncated: bool,
}

#[derive(Debug, Clone, Serialize)]
pub struct RedactionCounts {
    pub distinct: usize,
    pub occurrences: usize,
}

#[derive(Debug, Serialize)]
pub struct Manifest {
    pub tool_version: String,
    pub api: String,
    pub target: InstanceTarget,
    pub collected_at: String,
    pub transport: String,
    pub items: Vec<ManifestItem>,
}

/// Aggregated redaction report.
#[derive(Debug, Serialize)]
pub struct RedactionReport {
    pub per_item: HashMap<String, RedactionCounts>,
    pub total: RedactionCounts,
}

/// Builds the tar.gz bundle.
pub async fn build_bundle(
    target: &InstanceTarget,
    transport_label: &str,
    items: Vec<(&str, GetResult)>,
    output: &std::path::Path,
) -> anyhow::Result<BundleSummary> {
    let ts = Utc::now().format("%Y%m%dT%H%M%SZ").to_string();
    let root_dir = format!("{}_{}_{}-diag-{}", target.namespace, target.kind, target.name, ts);
    let collected_at = Utc::now().to_rfc3339_opts(chrono::SecondsFormat::Secs, false);
    let tool_version = env!("CARGO_PKG_VERSION").to_string();

    // Build manifest items
    let mut manifest_items = Vec::new();
    let mut per_item_redactions: HashMap<String, RedactionCounts> = HashMap::new();
    let mut total_distinct: usize = 0;
    let mut total_occurrences: usize = 0;

    for (item_name, result) in &items {
        let ext = crate::items::extension_for_content_type(&result.content_type);
        let base_path = crate::items::item_path(item_name);
        let file_path = format!("{}{}", base_path, ext);
        const TRUNC_MARKER: &[u8] = b"... [truncated]";
        let truncated = result.body.windows(TRUNC_MARKER.len()).any(|w| w == TRUNC_MARKER);

        let manifest_item = ManifestItem {
            item: item_name.to_string(),
            file: file_path.clone(),
            http_status: result.status,
            content_type: result.content_type.clone(),
            bytes: result.body.len(),
            redactions: result.redactions.map(|(d, o)| RedactionCounts {
                distinct: d,
                occurrences: o,
            }),
            truncated,
        };

        if let Some((d, o)) = result.redactions {
            per_item_redactions.insert(item_name.to_string(), RedactionCounts {
                distinct: d,
                occurrences: o,
            });
            total_distinct += d;
            total_occurrences += o;
        }

        manifest_items.push(manifest_item);
    }

    let manifest = Manifest {
        tool_version: tool_version.clone(),
        api: "diag.vynil.solidite.fr/v1".to_string(),
        target: target.clone(),
        collected_at,
        transport: transport_label.to_string(),
        items: manifest_items,
    };

    let redaction_report = RedactionReport {
        per_item: per_item_redactions.clone(),
        total: RedactionCounts {
            distinct: total_distinct,
            occurrences: total_occurrences,
        },
    };

    // Write tar.gz
    let file = std::fs::File::create(output)?;
    let encoder = GzEncoder::new(file, Compression::default());
    let mut tar = Builder::new(encoder);

    // Add artefact files
    for (_item_name, result) in &items {
        let ext = crate::items::extension_for_content_type(&result.content_type);
        let base_path = crate::items::item_path(_item_name);
        // ext is a suffix, not a path segment: "<root>/cluster/clusterinfo.json".
        let file_path = format!("{}/{}{}", root_dir, base_path, ext);

        let mut header = tar::Header::new_gnu();
        header.set_size(result.body.len() as u64);
        header.set_mode(0o644);
        header.set_mtime(0);
        header.set_username("vynil").ok();
        header.set_cksum();
        tar.append_data(&mut header, file_path.as_str(), Cursor::new(&result.body))?;
    }

    // Add manifest.yaml
    {
        let manifest_yaml = serde_yaml::to_string(&manifest)?;
        let file_path = format!("{}/manifest.yaml", root_dir);
        let mut header = tar::Header::new_gnu();
        header.set_size(manifest_yaml.len() as u64);
        header.set_mode(0o644);
        header.set_mtime(0);
        header.set_username("vynil").ok();
        header.set_cksum();
        tar.append_data(
            &mut header,
            file_path.as_str(),
            Cursor::new(manifest_yaml.as_bytes()),
        )?;
    }

    // Add redactions.json
    {
        let redactions_json = serde_json::to_string(&redaction_report)?;
        let file_path = format!("{}/redactions.json", root_dir);
        let mut header = tar::Header::new_gnu();
        header.set_size(redactions_json.len() as u64);
        header.set_mode(0o644);
        header.set_mtime(0);
        header.set_username("vynil").ok();
        header.set_cksum();
        tar.append_data(
            &mut header,
            file_path.as_str(),
            Cursor::new(redactions_json.as_bytes()),
        )?;
    }

    // Add SUMMARY.md
    {
        let summary = build_summary(target, &tool_version, &ts, &items, &redaction_report);
        let file_path = format!("{}/SUMMARY.md", root_dir);
        let mut header = tar::Header::new_gnu();
        header.set_size(summary.len() as u64);
        header.set_mode(0o644);
        header.set_mtime(0);
        header.set_username("vynil").ok();
        header.set_cksum();
        tar.append_data(&mut header, file_path.as_str(), Cursor::new(summary.as_bytes()))?;
    }

    tar.finish()?;

    let error_items: Vec<String> = items
        .iter()
        .filter(|(_, r)| r.status >= 400)
        .map(|(name, _)| name.to_string())
        .collect();

    Ok(BundleSummary {
        output_path: output.to_path_buf(),
        item_count: items.len(),
        total_redactions_distinct: total_distinct,
        total_redactions_occurrences: total_occurrences,
        error_items,
    })
}

/// Bundle build summary for stdout output.
pub struct BundleSummary {
    pub output_path: std::path::PathBuf,
    pub item_count: usize,
    pub total_redactions_distinct: usize,
    pub total_redactions_occurrences: usize,
    pub error_items: Vec<String>,
}

fn is_false(v: &bool) -> bool {
    !v
}

fn build_summary(
    target: &InstanceTarget,
    tool_version: &str,
    ts: &str,
    items: &[(&str, GetResult)],
    redaction_report: &RedactionReport,
) -> String {
    let mut md = String::new();
    md.push_str("# Vynil Diagnostic Bundle\n\n");
    md.push_str(&format!(
        "- **Instance**: `{}/{}/{}`\n",
        target.kind, target.namespace, target.name
    ));
    md.push_str(&format!("- **Collected**: {}\n", ts));
    md.push_str(&format!("- **Tool version**: {}\n\n", tool_version));

    md.push_str("## Collected Items\n\n");
    md.push_str("| Item | File | HTTP Status | Bytes | Redactions (distinct / occurrences) |\n");
    md.push_str("|------|------|-------------|-------|--------------------------------------|\n");

    for (item_name, result) in items {
        let ext = crate::items::extension_for_content_type(&result.content_type);
        let base_path = crate::items::item_path(item_name);
        let file_path = format!("{}{}", base_path, ext);
        let redactions_str = match result.redactions {
            Some((d, o)) => format!("{}/{}", d, o),
            None => "—".to_string(),
        };
        md.push_str(&format!(
            "| {} | {} | {} | {} | {} |\n",
            item_name,
            file_path,
            result.status,
            result.body.len(),
            redactions_str
        ));
    }

    md.push_str("\n## Redactions Summary\n\n");
    md.push_str(&format!(
        "- **Total distinct values redacted**: {}\n",
        redaction_report.total.distinct
    ));
    md.push_str(&format!(
        "- **Total occurrences replaced**: {}\n\n",
        redaction_report.total.occurrences
    ));

    md.push_str("## How to Read This Bundle\n\n");
    md.push_str("1. `instance/state` — instance conditions, tags, Terraform/Rhai state (anonymized).\n");
    md.push_str("2. `logs/` — agent, operator, and child logs for error investigation (anonymized).\n");
    md.push_str("3. `cluster/` and `config/` — cluster context and Vynil configuration.\n");
    md.push_str("4. `redactions.json` — audit trail of anonymized values.\n");
    md.push_str("\nAll artefacts are anonymized: secret values are replaced with `<anonymized>`.\n");

    // Best-effort: if state is present and parses as YAML, extract conditions
    if let Some((_, state_result)) = items.iter().find(|(name, _)| *name == "state")
        && state_result.status == 200
        && let Ok(state_yaml) = String::from_utf8(state_result.body.clone())
        && let Ok(doc) = serde_yaml::from_str::<serde_yaml::Value>(&state_yaml)
        && let Some(status) = doc.get("status")
        && let Some(conditions) = status.get("conditions")
        && let Some(conds) = conditions.as_sequence()
    {
        md.push_str("\n## Instance Conditions\n\n");
        md.push_str("| Type | Status | Reason | Message |\n");
        md.push_str("|------|--------|--------|---------|\n");
        for cond in conds {
            let ctype = cond.get("type").and_then(|v| v.as_str()).unwrap_or("—");
            let cstatus = cond.get("status").and_then(|v| v.as_str()).unwrap_or("—");
            let creason = cond.get("reason").and_then(|v| v.as_str()).unwrap_or("—");
            let cmessage = cond.get("message").and_then(|v| v.as_str()).unwrap_or("—");
            md.push_str(&format!(
                "| {} | {} | {} | {} |\n",
                ctype, cstatus, creason, cmessage
            ));
        }
    }

    md
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_manifest_serialization() {
        let manifest = Manifest {
            tool_version: "0.7.7".to_string(),
            api: "diag.vynil.solidite.fr/v1".to_string(),
            target: InstanceTarget {
                namespace: "test-ns".to_string(),
                kind: "systeminstances".to_string(),
                name: "test-inst".to_string(),
            },
            collected_at: "2026-06-28T14:05:12Z".to_string(),
            transport: "direct".to_string(),
            items: vec![ManifestItem {
                item: "clusterinfo".to_string(),
                file: "cluster/clusterinfo.json".to_string(),
                http_status: 200,
                content_type: "application/json".to_string(),
                bytes: 1234,
                redactions: Some(RedactionCounts {
                    distinct: 3,
                    occurrences: 7,
                }),
                truncated: false,
            }],
        };
        let yaml = serde_yaml::to_string(&manifest).expect("serialize manifest");
        assert!(yaml.contains("tool_version: 0.7.7"));
        assert!(yaml.contains("namespace: test-ns"));
        assert!(yaml.contains("http_status: 200"));
        assert!(yaml.contains("redactions:"));
    }

    #[test]
    fn test_redaction_report_serialization() {
        let mut per_item = HashMap::new();
        per_item.insert("state".to_string(), RedactionCounts {
            distinct: 2,
            occurrences: 5,
        });
        per_item.insert("agentlog".to_string(), RedactionCounts {
            distinct: 1,
            occurrences: 3,
        });
        let report = RedactionReport {
            per_item,
            total: RedactionCounts {
                distinct: 3,
                occurrences: 8,
            },
        };
        let json = serde_json::to_string(&report).expect("serialize report");
        assert!(json.contains("\"distinct\":3"));
        assert!(json.contains("\"occurrences\":8"));
    }

    #[test]
    fn test_manifest_skips_none_redactions() {
        let item = ManifestItem {
            item: "clusterinfo".to_string(),
            file: "cluster/clusterinfo.json".to_string(),
            http_status: 200,
            content_type: "application/json".to_string(),
            bytes: 100,
            redactions: None,
            truncated: false,
        };
        let yaml = serde_yaml::to_string(&item).expect("serialize item");
        assert!(!yaml.contains("redactions"));
    }

    #[test]
    fn test_manifest_skips_false_truncated() {
        let item = ManifestItem {
            item: "clusterinfo".to_string(),
            file: "cluster/clusterinfo.json".to_string(),
            http_status: 200,
            content_type: "application/json".to_string(),
            bytes: 100,
            redactions: None,
            truncated: false,
        };
        let yaml = serde_yaml::to_string(&item).expect("serialize item");
        assert!(!yaml.contains("truncated"));
    }

    #[test]
    fn test_manifest_includes_true_truncated() {
        let item = ManifestItem {
            item: "agentlog".to_string(),
            file: "logs/agentlog.log".to_string(),
            http_status: 200,
            content_type: "text/plain".to_string(),
            bytes: 2048,
            redactions: None,
            truncated: true,
        };
        let yaml = serde_yaml::to_string(&item).expect("serialize item");
        assert!(yaml.contains("truncated: true"));
    }
}
