//! Cobra `__complete` protocol for `kubectl` plugin shell completion.
//! Entry point: `kubectl-vynil __complete <words...>` (see docs/conception/completion.md).

use crate::cli::Cli;
use clap::CommandFactory;

// ── Cobra directive bitfield ────────────────────────────────────────────────
const NO_FILE_COMP: u8 = 4;

pub struct Candidate {
    pub value: String,
    pub description: Option<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum KindId {
    Jukebox,
    Vti,
    Vsvc,
    Vsi,
}

impl KindId {
    fn parse(s: &str) -> Option<Self> {
        match s {
            "jukebox" | "box" => Some(Self::Jukebox),
            "vti" | "tenantinstance" | "tenantinstances" => Some(Self::Vti),
            "vsvc" | "serviceinstance" | "serviceinstances" => Some(Self::Vsvc),
            "vsi" | "systeminstance" | "systeminstances" => Some(Self::Vsi),
            _ => None,
        }
    }

    /// Name of the corresponding `clap` subcommand in `enum Commands`.
    fn subcommand_name(self) -> &'static str {
        match self {
            Self::Jukebox => "jukebox",
            Self::Vti => "vti",
            Self::Vsvc => "vsvc",
            Self::Vsi => "vsi",
        }
    }
}

#[derive(Debug, PartialEq, Eq)]
enum Slot {
    Kind,
    Name(KindId),
    Verb(KindId),
    VerbFlag(KindId, String),
    GlobalFlagValue,
    None,
}

/// Splits the already-typed words into (positionals, previous-word-was-a-bare-global-flag).
fn classify(words: &[String]) -> (Slot, String) {
    let Some((to_complete, typed)) = words.split_last() else {
        return (Slot::Kind, String::new());
    };
    let to_complete = to_complete.clone();

    // Walk typed words, absorbing global flags and their values.
    let mut positionals: Vec<&str> = Vec::new();
    let mut i = 0;
    let mut prev_bare_global = false;
    while i < typed.len() {
        let w = typed[i].as_str();
        prev_bare_global = false;
        if w == "-n" || w == "--namespace" || w == "--context" {
            // consumes the next word as its value
            i += 2;
            if i > typed.len() {
                prev_bare_global = true; // flag was last, value is what we're completing
            }
            continue;
        }
        if w.starts_with("--namespace=") || w.starts_with("--context=") {
            i += 1;
            continue;
        }
        if w.starts_with('-') {
            // unknown flag: skip it, don't treat as positional
            i += 1;
            continue;
        }
        positionals.push(w);
        i += 1;
    }

    if prev_bare_global {
        return (Slot::GlobalFlagValue, to_complete);
    }

    // Completing a flag?
    if to_complete.starts_with('-') {
        return match positionals.len() {
            0 | 1 => (Slot::Kind, to_complete), // global flags surfaced via reflection in candidates()
            2 => (Slot::Kind, to_complete),     // between name and verb: still global flags
            _ => {
                let k = KindId::parse(positionals[0]);
                match k {
                    Some(k) => (Slot::VerbFlag(k, positionals[2].to_string()), to_complete),
                    None => (Slot::None, to_complete),
                }
            }
        };
    }

    match positionals.len() {
        0 => (Slot::Kind, to_complete),
        1 => match KindId::parse(positionals[0]) {
            Some(k) => (Slot::Name(k), to_complete),
            None => (Slot::None, to_complete),
        },
        2 => match KindId::parse(positionals[0]) {
            Some(k) => (Slot::Verb(k), to_complete),
            None => (Slot::None, to_complete),
        },
        _ => (Slot::None, to_complete),
    }
}

/// First line of a clap help/about string, tabs stripped.
fn one_line(s: Option<&clap::builder::StyledStr>) -> Option<String> {
    let raw = s?.to_string();
    let first = raw
        .lines()
        .next()
        .unwrap_or("")
        .replace('\t', " ")
        .trim()
        .to_string();
    (!first.is_empty()).then_some(first)
}

fn kind_candidates() -> Vec<Candidate> {
    let cmd = Cli::command();
    let mut out = Vec::new();
    for sub in cmd.get_subcommands() {
        if sub.is_hide_set() {
            continue;
        }
        let about = one_line(sub.get_about());
        out.push(Candidate {
            value: sub.get_name().to_string(),
            description: about.clone(),
        });
        for alias in sub.get_visible_aliases() {
            out.push(Candidate {
                value: alias.to_string(),
                description: about.clone(),
            });
        }
    }
    out
}

fn verb_candidates(kind: KindId) -> Vec<Candidate> {
    let cmd = Cli::command();
    let Some(sub) = cmd
        .get_subcommands()
        .find(|s| s.get_name() == kind.subcommand_name())
    else {
        return vec![];
    };
    sub.get_subcommands()
        .filter(|v| !v.is_hide_set())
        .map(|v| Candidate {
            value: v.get_name().to_string(),
            description: one_line(v.get_about()),
        })
        .collect()
}

fn global_flag_candidates() -> Vec<Candidate> {
    Cli::command()
        .get_arguments()
        .filter(|a| a.is_global_set())
        .filter_map(|a| {
            a.get_long().map(|l| Candidate {
                value: format!("--{l}"),
                description: one_line(a.get_help()),
            })
        })
        .collect()
}

fn verb_flag_candidates(kind: KindId, verb: &str) -> Vec<Candidate> {
    let cmd = Cli::command();
    let Some(sub) = cmd
        .get_subcommands()
        .find(|s| s.get_name() == kind.subcommand_name())
    else {
        return vec![];
    };
    let Some(v) = sub.get_subcommands().find(|v| v.get_name() == verb) else {
        return vec![];
    };
    v.get_arguments()
        .filter_map(|a| {
            a.get_long().map(|l| Candidate {
                value: format!("--{l}"),
                description: one_line(a.get_help()),
            })
        })
        .collect()
}

/// Render candidates and directive to Cobra format (tab-separated, :<directive> on last line).
fn render(candidates: &[Candidate], to_complete: &str) -> String {
    let mut out = String::new();
    for c in candidates.iter().filter(|c| c.value.starts_with(to_complete)) {
        match &c.description {
            Some(d) if !d.is_empty() => out.push_str(&format!("{}\t{}\n", c.value, d)),
            _ => out.push_str(&format!("{}\n", c.value)),
        }
    }
    out.push_str(&format!(":{NO_FILE_COMP}\n"));
    out
}

fn emit(candidates: &[Candidate], to_complete: &str) {
    print!("{}", render(candidates, to_complete));
}

pub fn run(words: Vec<String>) {
    // tâche 02 : rendra ceci async pour la requête cluster de Slot::Name
    let (slot, to_complete) = classify(&words);
    let candidates = match slot {
        Slot::Kind => {
            let mut c = kind_candidates();
            if to_complete.starts_with('-') {
                c = global_flag_candidates();
            }
            c
        }
        Slot::Verb(k) => verb_candidates(k),
        Slot::VerbFlag(k, ref v) => verb_flag_candidates(k, v),
        Slot::Name(_) => vec![], // tâche 02
        Slot::GlobalFlagValue | Slot::None => vec![],
    };
    emit(&candidates, &to_complete);
}

#[cfg(test)]
mod tests {
    use super::*;

    fn args(strs: &[&str]) -> Vec<String> {
        strs.iter().map(|s| s.to_string()).collect()
    }

    // ── Automate tests (classify) ──────────────────────────────────────────

    #[test]
    fn classify_empty_completion() {
        let (slot, to_complete) = classify(&args(&[""]));
        assert_eq!(slot, Slot::Kind);
        assert_eq!(to_complete, "");
    }

    #[test]
    fn classify_partial_kind() {
        let (slot, to_complete) = classify(&args(&["v"]));
        assert_eq!(slot, Slot::Kind);
        assert_eq!(to_complete, "v");
    }

    #[test]
    fn classify_kind_then_name() {
        let (slot, to_complete) = classify(&args(&["vti", ""]));
        assert_eq!(slot, Slot::Name(KindId::Vti));
        assert_eq!(to_complete, "");
    }

    #[test]
    fn classify_kind_alias_then_name() {
        let (slot, to_complete) = classify(&args(&["tenantinstance", ""]));
        assert_eq!(slot, Slot::Name(KindId::Vti));
        assert_eq!(to_complete, "");
    }

    #[test]
    fn classify_kind_name_then_verb() {
        let (slot, to_complete) = classify(&args(&["vti", "x", ""]));
        assert_eq!(slot, Slot::Verb(KindId::Vti));
        assert_eq!(to_complete, "");
    }

    #[test]
    fn classify_kind_name_partial_verb() {
        let (slot, to_complete) = classify(&args(&["vti", "x", "up"]));
        assert_eq!(slot, Slot::Verb(KindId::Vti));
        assert_eq!(to_complete, "up");
    }

    #[test]
    fn classify_verb_with_flag() {
        let (slot, to_complete) = classify(&args(&["vti", "x", "upgrade", "--"]));
        assert_eq!(slot, Slot::VerbFlag(KindId::Vti, "upgrade".to_string()));
        assert_eq!(to_complete, "--");
    }

    #[test]
    fn classify_global_flag_before_kind() {
        let (slot, to_complete) = classify(&args(&["-n", "prod", "vti", ""]));
        assert_eq!(slot, Slot::Name(KindId::Vti));
        assert_eq!(to_complete, "");
    }

    #[test]
    fn classify_verb_with_global_flag_bare() {
        let (slot, to_complete) = classify(&args(&["vti", "x", "upgrade", "-n", ""]));
        assert_eq!(slot, Slot::GlobalFlagValue);
        assert_eq!(to_complete, "");
    }

    #[test]
    fn classify_jukebox_then_name() {
        let (slot, to_complete) = classify(&args(&["box", ""]));
        assert_eq!(slot, Slot::Name(KindId::Jukebox));
        assert_eq!(to_complete, "");
    }

    #[test]
    fn classify_unknown_kind() {
        let (slot, to_complete) = classify(&args(&["nope", ""]));
        assert_eq!(slot, Slot::None);
        assert_eq!(to_complete, "");
    }

    #[test]
    fn classify_global_flag_bare() {
        let (slot, to_complete) = classify(&args(&["-n", ""]));
        assert_eq!(slot, Slot::GlobalFlagValue);
        assert_eq!(to_complete, "");
    }

    // ── Static candidates tests ────────────────────────────────────────────

    #[test]
    fn kind_candidates_contains_jukebox() {
        let candidates = kind_candidates();
        assert!(candidates.iter().any(|c| c.value == "jukebox"));
    }

    #[test]
    fn kind_candidates_contains_vti() {
        let candidates = kind_candidates();
        assert!(candidates.iter().any(|c| c.value == "vti"));
    }

    #[test]
    fn kind_candidates_contains_vsvc() {
        let candidates = kind_candidates();
        assert!(candidates.iter().any(|c| c.value == "vsvc"));
    }

    #[test]
    fn kind_candidates_contains_vsi() {
        let candidates = kind_candidates();
        assert!(candidates.iter().any(|c| c.value == "vsi"));
    }

    #[test]
    fn kind_candidates_excludes_hidden_complete() {
        let candidates = kind_candidates();
        assert!(!candidates.iter().any(|c| c.value == "__complete"));
    }

    #[test]
    fn verb_candidates_for_vti() {
        let candidates = verb_candidates(KindId::Vti);
        let values: Vec<&str> = candidates.iter().map(|c| c.value.as_str()).collect();
        let expected = vec![
            "upgrade",
            "scan",
            "diagnostic",
            "children",
            "agentlog",
            "childlogs",
            "operatorlog",
        ];
        for verb in &expected {
            assert!(values.contains(verb), "verb {} should be in VTI candidates", verb);
        }
        assert_eq!(
            values.len(),
            expected.len(),
            "should have exactly {} verbs",
            expected.len()
        );
    }

    #[test]
    fn verb_candidates_for_jukebox() {
        let candidates = verb_candidates(KindId::Jukebox);
        let values: Vec<&str> = candidates.iter().map(|c| c.value.as_str()).collect();
        assert_eq!(values, vec!["scan"]);
    }

    #[test]
    fn render_with_two_candidates_one_described() {
        let candidates = vec![
            Candidate {
                value: "upgrade".to_string(),
                description: Some("Force-reinstall".to_string()),
            },
            Candidate {
                value: "scan".to_string(),
                description: None,
            },
        ];
        let output = render(&candidates, "");
        let lines: Vec<&str> = output.lines().collect();
        assert_eq!(lines.len(), 3); // upgrade + description, scan, :4
        assert_eq!(lines[0], "upgrade\tForce-reinstall");
        assert_eq!(lines[1], "scan");
        assert_eq!(lines[2], ":4");
    }

    #[test]
    fn render_empty_candidates() {
        let candidates: Vec<Candidate> = vec![];
        let output = render(&candidates, "");
        assert_eq!(output, ":4\n");
    }

    #[test]
    fn render_filters_by_to_complete() {
        let candidates = vec![
            Candidate {
                value: "upgrade".to_string(),
                description: None,
            },
            Candidate {
                value: "scan".to_string(),
                description: None,
            },
        ];
        let output = render(&candidates, "up");
        let lines: Vec<&str> = output.lines().collect();
        assert_eq!(lines.len(), 2); // upgrade, :4
        assert_eq!(lines[0], "upgrade");
        assert_eq!(lines[1], ":4");
    }
}
