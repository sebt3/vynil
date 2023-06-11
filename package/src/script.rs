use rhai::{Engine, Scope, Module, RegisterNativeFunction};
use std::{process, path::{PathBuf, Path}};
use anyhow::{Result, bail};
use core::any::Any;
use crate::shell;
pub use rhai::ImmutableString;
use crate::terraform::gen_file;


pub fn gen_index(dest_dir: &PathBuf) -> Result<()> {
    let mut file  = PathBuf::new();
    file.push(dest_dir);
    file.push("index.rhai");
    gen_file(&file, &"
const VERSION=config.release;
const SRC=src;
const DEST=dest;
fn pre_pack() {
    shell(`kubectl kustomize https://github.com/rabbitmq/cluster-operator//config/manager/?ref=${global::VERSION} >${global::SRC}/manager.yaml`);
}
fn post_pack() {
    shell(`rm -f ${global::DEST}/v1_Secret_authentik.yaml`);
}
fn pre_install() {
    shell(`kubectl apply -k https://github.com/rabbitmq/cluster-operator//config/crd/?ref=v${global::VERSION}`);
}
".to_string(), false)
}


pub fn new_context(category:String, component:String, instance:String, src:String, dest:String, config:&serde_json::Map<String, serde_json::Value>) -> Scope<'static> {
    let json = serde_json::to_string(config).unwrap();
    let cfg: rhai::Dynamic = serde_json::from_str(&json).unwrap();
    let mut s = Scope::new();
    s.push_constant("instance", instance);
    s.push_constant("component", component);
    s.push_constant("category", category);
    s.push_constant("src", src);
    s.push_constant("dest", dest);
    s.push_constant("config", cfg);
    s
}

pub struct Script {
    pub engine: Engine,
    ctx: Scope<'static>
}
impl Script {
    pub fn new(file:&PathBuf, ctx: Scope<'static>) -> Script {
        let mut e = Engine::new();
        // Logging
        e.register_fn("log_debug", |s:ImmutableString| log::debug!("{s}"));
        e.register_fn("log_info", |s:ImmutableString| log::info!("{s}"));
        e.register_fn("log_warn", |s:ImmutableString| log::warn!("{s}"));
        e.register_fn("log_error", |s:ImmutableString| log::error!("{s}"));
        // lancement de commande shell
        e.register_fn("shell", |s:ImmutableString| {
            shell::run_log_check(&format!("{s}"));
        });
        e.register_fn("sh_value", |s:ImmutableString| {
            shell::get_output(&format!("{s}")).unwrap()
        });
        // TODO: Add an http client (download/get/post/put)
        // TODO: Add a kubectl wrapper
        if Path::new(&file).is_file() {
            let str = file.as_os_str().to_str().unwrap();
            let ast = match e.compile_file(str.into()) {Ok(d) => d, Err(e) => {log::error!("Loading {str} failed with: {e:}");process::exit(1)},};
            let module = match Module::eval_ast_as_new(ctx.clone(), &ast,&e) {
                Ok(d) => d, Err(e) => {log::error!("Parsing {str} failed with: {e:}");process::exit(1)},
            };
            e.register_global_module(module.into());
        }

        Script {engine: e, ctx}
    }

    pub fn set_context(&mut self, ctx: Scope<'static>) {
        self.ctx = ctx;
    }

    pub fn register<A: 'static, const N: usize, const C: bool, R: Any + Clone, const L: bool, F: RegisterNativeFunction<A, N, C, R, L>>(&mut self, name: &str, func: F) {
        self.engine.register_fn(name, func);
    }

    fn run_fn(&mut self, func: &str) -> Result<()> {
        let cmd = format!("let x = {func}();x!=false");
        match self.engine.eval_with_scope::<bool>(&mut self.ctx, cmd.as_str()) {Ok(b) => {if !b {bail!("function {func} failed.")}}, Err(e) => {bail!("{e}")}};
        Ok(())
    }
    fn have_fn(&mut self, stage: &str) -> bool {
        let cmd = format!("is_def_fn(\"{stage}\",0)");
        self.engine.eval::<bool>(cmd.as_str()).unwrap()
    }
    pub fn have_stage(&mut self, stage: &str) -> bool {
        self.have_fn(&format!("pre_{stage}")) || self.have_fn(&format!("post_{stage}"))
    }
    pub fn run_pre_stage(&mut self, stage: &str) -> Result<()> {
        if self.have_fn(&format!("pre_{stage}")) {
            match self.run_fn(&format!("pre_{stage}")) {Ok(_) => {}, Err(e) => {bail!("pre_{stage} failed with: {e}")}}
        }
        Ok(())
    }
    pub fn run_post_stage(&mut self, stage: &str) -> Result<()> {
        if self.have_fn(&format!("post_{stage}")) {
            match self.run_fn(&format!("post_{stage}")) {Ok(_) => {}, Err(e) => {bail!("post_{stage} failed with: {e}")}}
        }
        Ok(())
    }
}
