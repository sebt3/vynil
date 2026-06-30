use clap::{Args, Parser, Subcommand};
use serde::Serialize;

/// Vynil CLI — operate on Vynil instances and JukeBoxes from your kubectl context.
///
/// Noun-first grammar: `kubectl-vynil <kind> [-n <ns>] <name> <verb> [args]`.
#[derive(Parser, Debug)]
#[command(name = "kubectl-vynil", version, about, long_about = None)]
pub struct Cli {
    #[command(subcommand)]
    pub command: Commands,
}

/// Select the resource kind to act upon.
#[derive(Subcommand, Debug)]
pub enum Commands {
    /// Operate on a JukeBox (cluster-scoped package source).
    #[command(visible_alias = "box")]
    Jukebox(JukeboxArgs),
    /// Operate on a TenantInstance.
    #[command(name = "vti", visible_alias = "tenantinstance", alias = "tenantinstances")]
    Vti(InstanceArgs),
    /// Operate on a ServiceInstance.
    #[command(name = "vsvc", visible_alias = "serviceinstance", alias = "serviceinstances")]
    Vsvc(InstanceArgs),
    /// Operate on a SystemInstance.
    #[command(name = "vsi", visible_alias = "systeminstance", alias = "systeminstances")]
    Vsi(InstanceArgs),
}

// ── Kind table ────────────────────────────────────────────────────────────────

/// Static description of a Vynil instance kind.
#[derive(Debug, Clone, Copy)]
pub struct InstanceKindInfo {
    /// Plural resource name, e.g. `tenantinstances` (used in the diag API path).
    pub plural: &'static str,
    /// CamelCase kind, e.g. `TenantInstance` (used to build an `ApiResource`).
    pub kind: &'static str,
    /// Operator `type` label / job-name prefix, e.g. `tenant`.
    pub type_label: &'static str,
}

pub const TENANT_INSTANCE: InstanceKindInfo = InstanceKindInfo {
    plural: "tenantinstances",
    kind: "TenantInstance",
    type_label: "tenant",
};
pub const SERVICE_INSTANCE: InstanceKindInfo = InstanceKindInfo {
    plural: "serviceinstances",
    kind: "ServiceInstance",
    type_label: "service",
};
pub const SYSTEM_INSTANCE: InstanceKindInfo = InstanceKindInfo {
    plural: "systeminstances",
    kind: "SystemInstance",
    type_label: "system",
};

// ── JukeBox ─────────────────────────────────────────────────────────────────

#[derive(Args, Debug)]
pub struct JukeboxArgs {
    /// JukeBox name.
    pub name: String,
    #[command(subcommand)]
    pub verb: JukeboxVerb,
}

#[derive(Subcommand, Debug)]
pub enum JukeboxVerb {
    /// Trigger a (re)scan and wait for the scan job to complete.
    Scan(JukeboxScanArgs),
}

#[derive(Args, Debug)]
pub struct JukeboxScanArgs {
    /// Optional `<category>` or `<category>/<package>` to scan only that subset.
    pub package: Option<String>,
    /// Namespace where the Vynil operator and its scan jobs live.
    #[arg(long, default_value = "vynil-system")]
    pub vynil_namespace: String,
    /// Maximum seconds to wait for the scan job to complete.
    #[arg(long, default_value_t = 90)]
    pub timeout: u64,
}

// ── Instances ───────────────────────────────────────────────────────────────

#[derive(Args, Debug)]
pub struct InstanceArgs {
    /// Instance name.
    pub name: String,
    /// Namespace of the instance. Defaults to the current kubectl context namespace.
    #[arg(short, long)]
    pub namespace: Option<String>,
    #[command(subcommand)]
    pub verb: InstanceVerb,
}

#[derive(Subcommand, Debug)]
pub enum InstanceVerb {
    /// Force-reinstall the instance and follow the resulting install job.
    Upgrade(UpgradeArgs),
    /// Scan only the package referenced by this instance.
    Scan(InstanceScanArgs),
    /// Collect every diagnostic item into a tar.gz bundle.
    Diagnostic(DiagnosticArgs),
    /// Print the cluster info diagnostic item to stdout.
    Clusterinfo(ItemArgs),
    /// Print the Vynil config diagnostic item to stdout.
    Vynilconfig(ItemArgs),
    /// Print the packages diagnostic item to stdout.
    Packages(ItemArgs),
    /// Print the instance state diagnostic item to stdout.
    State(ItemArgs),
    /// Print the children diagnostic item to stdout.
    Children(ItemArgs),
    /// Print the agent log diagnostic item to stdout.
    Agentlog(ItemArgs),
    /// Print the child logs diagnostic item to stdout.
    Childlogs(ItemArgs),
    /// Print the operator log diagnostic item to stdout.
    Operatorlog(ItemArgs),
}

impl InstanceVerb {
    /// For single-item verbs, returns the diagnostic item name and its transport args.
    pub fn as_item(&self) -> Option<(&'static str, &TransportArgs)> {
        match self {
            InstanceVerb::Clusterinfo(a) => Some(("clusterinfo", &a.transport)),
            InstanceVerb::Vynilconfig(a) => Some(("vynilconfig", &a.transport)),
            InstanceVerb::Packages(a) => Some(("packages", &a.transport)),
            InstanceVerb::State(a) => Some(("state", &a.transport)),
            InstanceVerb::Children(a) => Some(("children", &a.transport)),
            InstanceVerb::Agentlog(a) => Some(("agentlog", &a.transport)),
            InstanceVerb::Childlogs(a) => Some(("childlogs", &a.transport)),
            InstanceVerb::Operatorlog(a) => Some(("operatorlog", &a.transport)),
            _ => None,
        }
    }
}

#[derive(Args, Debug)]
pub struct UpgradeArgs {
    /// Stream pod status live (like `kubectl get pod -w`) instead of waiting for a verdict.
    #[arg(short, long)]
    pub watch: bool,
    /// Namespace where the Vynil operator and its jobs live.
    #[arg(long, default_value = "vynil-system")]
    pub vynil_namespace: String,
    /// Maximum seconds to wait for the install job to finish.
    #[arg(long, default_value_t = 300)]
    pub timeout: u64,
}

#[derive(Args, Debug)]
pub struct InstanceScanArgs {
    /// Namespace where the Vynil operator and its scan jobs live.
    #[arg(long, default_value = "vynil-system")]
    pub vynil_namespace: String,
    /// Maximum seconds to wait for the scan job to complete.
    #[arg(long, default_value_t = 90)]
    pub timeout: u64,
}

/// Transport flags shared by the bundle and single-item diagnostic verbs.
#[derive(Args, Debug, Clone)]
pub struct TransportArgs {
    /// Direct mode: server URL to call directly (bypasses apiserver aggregation; testing).
    #[arg(long)]
    pub server_url: Option<String>,
    /// Bearer token for direct mode. Falls back to in-cluster SA token if not provided.
    #[arg(long)]
    pub token: Option<String>,
    /// Direct mode: skip TLS certificate verification (dev only).
    #[arg(long, default_value_t = false)]
    pub insecure: bool,
}

#[derive(Args, Debug)]
pub struct ItemArgs {
    #[command(flatten)]
    pub transport: TransportArgs,
}

#[derive(Args, Debug)]
pub struct DiagnosticArgs {
    /// Comma-separated subset of items to collect. Defaults to all.
    #[arg(long, value_delimiter = ',')]
    pub items: Option<Vec<String>>,
    /// Output file path. Defaults to `<name>-diag-<timestamp>.tar.gz`.
    #[arg(short = 'o', long)]
    pub output: Option<String>,
    #[command(flatten)]
    pub transport: TransportArgs,
}

// ── Diagnostic target (consumed by the transport / bundle layers) ─────────────

/// Resolved instance target for the diagnostic transport.
#[derive(Debug, Clone, Serialize)]
pub struct InstanceTarget {
    pub namespace: String,
    pub kind: String,
    pub name: String,
}

impl InstanceTarget {
    pub fn new(namespace: &str, kind_plural: &str, name: &str) -> Self {
        InstanceTarget {
            namespace: namespace.to_string(),
            kind: kind_plural.to_string(),
            name: name.to_string(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use clap::CommandFactory;

    #[test]
    fn cli_definition_is_valid() {
        Cli::command().debug_assert();
    }

    #[test]
    fn parses_jukebox_scan_with_package() {
        let cli =
            Cli::try_parse_from(["kubectl-vynil", "box", "kydah-alpha", "scan", "dev/code-server"]).unwrap();
        match cli.command {
            Commands::Jukebox(a) => {
                assert_eq!(a.name, "kydah-alpha");
                match a.verb {
                    JukeboxVerb::Scan(s) => assert_eq!(s.package.as_deref(), Some("dev/code-server")),
                }
            }
            _ => panic!("expected jukebox"),
        }
    }

    #[test]
    fn parses_instance_upgrade_verb_last_with_floating_flag() {
        // name before verb, -n placed between kind and name
        let cli = Cli::try_parse_from(["kubectl-vynil", "vsi", "-n", "toto", "titi", "upgrade"]).unwrap();
        match cli.command {
            Commands::Vsi(a) => {
                assert_eq!(a.namespace.as_deref(), Some("toto"));
                assert_eq!(a.name, "titi");
                assert!(matches!(a.verb, InstanceVerb::Upgrade(_)));
            }
            _ => panic!("expected vsi"),
        }
    }

    #[test]
    fn instance_kind_aliases_resolve() {
        for (argv, expect) in [
            ("tenantinstance", "vti"),
            ("serviceinstances", "vsvc"),
            ("systeminstance", "vsi"),
        ] {
            let cli = Cli::try_parse_from(["kubectl-vynil", argv, "-n", "ns", "x", "state"]).unwrap();
            let got = match cli.command {
                Commands::Vti(_) => "vti",
                Commands::Vsvc(_) => "vsvc",
                Commands::Vsi(_) => "vsi",
                Commands::Jukebox(_) => "box",
            };
            assert_eq!(got, expect, "alias {} should map to {}", argv, expect);
        }
    }

    #[test]
    fn item_verb_maps_to_item_name() {
        let cli = Cli::try_parse_from(["kubectl-vynil", "vti", "-n", "ns", "x", "agentlog"]).unwrap();
        match cli.command {
            Commands::Vti(a) => assert_eq!(a.verb.as_item().unwrap().0, "agentlog"),
            _ => panic!("expected vti"),
        }
    }

    #[test]
    fn old_invented_aliases_are_rejected() {
        // The pre-existing ti/si/sysi aliases must no longer be accepted.
        for bad in ["ti", "si", "sysi"] {
            assert!(
                Cli::try_parse_from(["kubectl-vynil", bad, "-n", "ns", "x", "state"]).is_err(),
                "{} should be rejected",
                bad
            );
        }
    }
}
