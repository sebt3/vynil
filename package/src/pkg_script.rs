use std::{fs, path::{Path,PathBuf}};
use anyhow::{Result, bail};
use base64::{engine::general_purpose::STANDARD, Engine as _};
use crate::shell;
use rhai::{Engine, Dynamic, ImmutableString,Map};
use crate::terraform::save_to_tf;
use std::{collections::HashMap,env};
use serde_json;
use serde_yaml;

fn explode_to_tf(src: &str, dest: &str, base: &str) -> Result<()> {
    let content = fs::read_to_string(src)
        .expect("Should have been able to read the file");
    let parts = content.split("
---
");
    let mut groups: HashMap<String, HashMap<String,serde_yaml::Value>> = HashMap::new();
    for i in parts {
        let str = i.to_string();
        let str = str.trim();
        if str.split('\n').count() > 4 {
            let yaml: serde_yaml::Value = match serde_yaml::from_str(str) {Ok(d) => d, Err(e) => {tracing::error!("'{e:}' while parsing yaml chuck from {}: '{str}'", src);std::process::exit(1)},};
            let kind = yaml["kind"].as_str().map(std::string::ToString::to_string).unwrap();
            let g = if ["ClusterRole","ClusterRoleBinding","Role","RoleBinding","ServiceAccount"].contains(&kind.as_str()) {"rbac"} else if ["HorizontalPodAutoscaler", "Deployment", "DaemonSet", "StatefulSet", "PodDisruptionBudget", "ResourceQuota"].contains(&kind.as_str()){"workload"}  else if ["PodMonitor","PrometheusRule","ServiceMonitor","Prometheus","PrometheusAgent","Probe","Alertmanager","AlertmanagerConfig","ThanosRuler"].contains(&kind.as_str()){"monitoring"} else {&kind.as_str()};
            let name = yaml["metadata"]["name"].as_str().map(std::string::ToString::to_string).unwrap();
            if !groups.contains_key(g) {
                groups.insert(g.to_string(), HashMap::new());
            }
            if let Some(grp) = groups.get_mut(&g.to_string()) {
                grp.insert(format!("{kind}_{name}"), yaml);
            }
        }
    }
    for (kind, yamls) in &groups {
        let mut content = "".to_string();
        let mut file = PathBuf::new();
        file.push(dest);
        let mut exist = PathBuf::new();
        exist.push(dest);
        exist.push(format!("{}_{}.tf", base, kind));
        if Path::new(&exist).is_file() {
            file.push(format!("gen_{}_{}.tf", base, kind));
        } else {
            file.push(format!("{}_{}.tf", base, kind));
        }
        for (name, yaml) in yamls {
            let mut values = yaml.clone();
            if !["ClusterRole","ClusterRoleBinding","MutatingWebhookConfiguration","ValidatingWebhookConfiguration","Namespace","APIService","Distrib","PriorityClass"].contains(&yaml["kind"].as_str().unwrap()) {
                values["metadata"]["namespace"] = serde_yaml::Value::from("${var.namespace}");
            }
            values["metadata"]["ownerReferences"] = serde_yaml::Value::from("${jsonencode(var.install_owner)}");
            values["metadata"]["labels"] = serde_yaml::Value::from("${jsonencode(local.common_labels)}");
            let str = serde_yaml::to_string(&values).unwrap();
            content.push_str(&format!("resource \"kubectl_manifest\" \"{}\" {{
  yaml_body  = <<-EOF
{}EOF
}}

",name,indent::indent_all_by(4,str)));
        }
        match std::fs::write(file.clone(), &content) {Ok(_) => {}, Err(e) => bail!("Error {} while generating: {}", e, file.display()),};
    }
    Ok(())
}


pub fn add_pkg_to_engine(e: &mut Engine) {
    // lancement de commande shell
    e.register_fn("shell", |s:ImmutableString| {shell::run_log_check(&format!("{s}"))});
    e.register_fn("sh_value", |s:ImmutableString| {shell::get_output(&format!("{s}")).unwrap()});
    // File management
    e.register_fn("save_to_tf", move |filename: ImmutableString, name: ImmutableString, data: Dynamic| {
        match save_to_tf(&filename, &name, &serde_json::to_string(&data).unwrap()) {Ok(d) => d, Err(e) => {
            tracing::error!("Failed to save {filename}: {e:}");
        }};
    });
    e.register_fn("yaml_explode_to_tf", move |source: ImmutableString, dest: ImmutableString, base: ImmutableString| {
        match explode_to_tf(&source, &dest, &base) {Ok(d) => d, Err(e) => {
            tracing::error!("Failed to explode {source} to {dest}/{base}: {e:}");
        }};
    });
    e.register_fn("get_env", |var: ImmutableString| -> ImmutableString {
        match env::var(var.to_string()) {
            Ok(s) => s.into(),
            Err(_e) => "".into()
        }
    });
    e.register_fn("base64_decode", |val: ImmutableString| -> ImmutableString {
        String::from_utf8(STANDARD.decode(val.to_string()).unwrap()).unwrap().into()
    });
    e.register_fn("base64_encode", |val: ImmutableString| -> ImmutableString {
        STANDARD.encode(val.to_string()).into()
    });
    e.register_fn("json_encode", |val: Dynamic| -> ImmutableString {
        serde_json::to_string(&val).unwrap().into()
    });
    e.register_fn("json_encode", |val: Map| -> ImmutableString {
        serde_json::to_string(&val).unwrap().into()
    });
    e.register_fn("json_decode", |val: ImmutableString| -> Dynamic {
        serde_json::from_str(&val.to_string()).unwrap()
    });
    e.register_fn("yaml_encode", |val: &Dynamic| -> ImmutableString {
        serde_yaml::to_string(val).unwrap().into()
    });
    e.register_fn("yaml_decode", |val: ImmutableString| -> Dynamic {
        serde_yaml::from_str(&val.to_string()).unwrap()
    });
}
