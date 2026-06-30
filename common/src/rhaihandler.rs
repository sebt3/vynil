use crate::{
    context,
    handlebarshandler::handlebars_rhai_register,
    httphandler::http_rhai_register,
    httpmock::{HttpMockItem, httpmock_rhai_register},
    instanceservice::service_rhai_register,
    instancesystem::system_rhai_register,
    instancetenant::tenant_rhai_register,
    jukebox::jukebox_rhai_register,
    jukebox_file::file_jukebox_rhai_register,
    k8sgeneric::k8sgeneric_rhai_register,
    k8smock::{k8smock_rhai_register, oci_mock_rhai_register},
    k8sraw::k8sraw_rhai_register,
    k8sworkload::k8sworkload_rhai_register,
    s3handler::s3_rhai_register,
    vynilpackage::package_rhai_register,
    yamlhandler::yaml_ordered_rhai_register,
};

// ── Re-exports from core ──────────────────────────────────────────────────────

pub use vynil_core::engine::{
    AST, ASTNode, Array, Dynamic, Engine, Expr, FileModuleResolver, ImmutableString, Map, Module,
    ModuleResolversCollection, ParseError, Scope, Stmt, base64_decode, to_dynamic,
};

// ── Newtype wrapper around vynil_core::Script ─────────────────────────────────

pub struct Script(pub vynil_core::engine::Script);

impl std::ops::Deref for Script {
    type Target = vynil_core::engine::Script;

    fn deref(&self) -> &Self::Target {
        &self.0
    }
}
impl std::ops::DerefMut for Script {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.0
    }
}

fn vynil_owner_register(engine: &mut Engine) {
    engine.register_fn("vynil_owner", || -> Dynamic {
        match context::get_owner() {
            Some(o) => serde_json::from_str(&serde_json::to_string(&o).unwrap()).unwrap(),
            None => serde_json::from_str("{}").unwrap(),
        }
    });
}

impl Script {
    pub fn new_core(resolver_path: Vec<String>) -> Script {
        let mut script = Script(vynil_core::engine::Script::new_bare(resolver_path));
        vynil_owner_register(&mut script.engine);
        yaml_ordered_rhai_register(&mut script.engine);
        package_rhai_register(&mut script.engine);
        handlebars_rhai_register(&mut script.engine);
        script
    }

    pub fn new_file_scan(resolver_path: Vec<String>) -> Script {
        let mut script = Self::new_core(resolver_path);
        http_rhai_register(&mut script.engine);
        s3_rhai_register(&mut script.engine);
        file_jukebox_rhai_register(&mut script.engine);
        script
    }

    pub fn new(resolver_path: Vec<String>) -> Script {
        let mut script = Self::new_core(resolver_path);
        http_rhai_register(&mut script.engine);
        s3_rhai_register(&mut script.engine);
        service_rhai_register(&mut script.engine);
        system_rhai_register(&mut script.engine);
        tenant_rhai_register(&mut script.engine);
        jukebox_rhai_register(&mut script.engine);
        k8sgeneric_rhai_register(&mut script.engine);
        k8sraw_rhai_register(&mut script.engine);
        k8sworkload_rhai_register(&mut script.engine);
        script
    }

    pub fn new_mock(
        resolver_path: Vec<String>,
        http_mocks: Vec<HttpMockItem>,
        k8s_mocks: Vec<Dynamic>,
        created_objects: std::sync::Arc<std::sync::Mutex<Vec<Dynamic>>>,
    ) -> Script {
        let mut script = Self::new_core(resolver_path);
        oci_mock_rhai_register(&mut script.engine);
        httpmock_rhai_register(&mut script.engine, http_mocks);
        k8smock_rhai_register(&mut script.engine, k8s_mocks, created_objects);
        script
    }

    // ── Error-converting wrappers (vynil_core::Result → common::Result) ─────

    pub fn run_file(&mut self, file: &std::path::PathBuf) -> crate::Result<Dynamic> {
        self.0.run_file(file).map_err(Into::into)
    }

    pub fn eval(&mut self, script: &str) -> crate::Result<Dynamic> {
        self.0.eval(script).map_err(Into::into)
    }

    pub fn eval_truth(&mut self, script: &str) -> crate::Result<bool> {
        self.0.eval_truth(script).map_err(Into::into)
    }

    pub fn eval_map_string(&mut self, script: &str) -> crate::Result<String> {
        self.0.eval_map_string(script).map_err(Into::into)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn make_script() -> Script {
        Script::new(vec![])
    }

    // ── Régression : préservation de l'ordre des clés YAML ───────────────────
    #[test]
    fn test_yaml_decode_modify_single_key_preserves_order() {
        let mut s = make_script();

        let original = "z: first\na: second\nm: third\n";
        s.ctx
            .set_or_push("input_yaml", Dynamic::from(original.to_string()));

        let result = s
            .eval(
                r#"
            let doc = yaml_decode_ordered(input_yaml);
            doc["a"] = "MODIFIED";
            yaml_encode_ordered(doc)
        "#,
            )
            .unwrap();

        let encoded = result.to_string();

        let orig_lines: Vec<&str> = original.lines().filter(|l| !l.is_empty()).collect();
        let enc_lines: Vec<&str> = encoded.lines().filter(|l| !l.is_empty()).collect();

        assert_eq!(
            orig_lines.len(),
            enc_lines.len(),
            "Le nombre de lignes a changé — des clés ont été perdues ou ajoutées.\n\
             Original:\n{}\nRésultat:\n{}",
            original,
            encoded
        );

        let changed: Vec<usize> = orig_lines
            .iter()
            .zip(enc_lines.iter())
            .enumerate()
            .filter(|(_, (a, b))| a != b)
            .map(|(i, _)| i)
            .collect();

        assert_eq!(
            changed.len(),
            1,
            "Attendu exactement 1 ligne modifiée (la clé 'a'), mais {} lignes diffèrent.\n\
             Cela indique que l'ordre des clés n'a PAS été préservé (bug serde_yaml/BTreeMap).\n\
             Original:\n{}\nRésultat:\n{}",
            changed.len(),
            original,
            encoded
        );

        assert!(
            enc_lines[changed[0]].contains("MODIFIED"),
            "La ligne modifiée ne contient pas 'MODIFIED' : {}",
            enc_lines[changed[0]]
        );
    }
}
