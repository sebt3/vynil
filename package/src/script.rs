use rhai::{Engine, Scope, Module, RhaiNativeFunc};
use std::{process, path::{PathBuf, Path}};
use anyhow::{Result, bail};
use core::any::Any;
use crate::shell;
pub use rhai::ImmutableString;

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
    pub fn new(ctx: Scope<'static>) -> Script {
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

        Script {engine: e, ctx}
    }

    pub fn from(file:&PathBuf, ctx: Scope<'static>) -> Script {
        let mut script = Self::new(ctx.clone());
        if Path::new(&file).is_file() {
            let str = file.as_os_str().to_str().unwrap();
            let ast = match script.engine.compile_file(str.into()) {Ok(d) => d, Err(e) => {log::error!("Loading {str} failed with: {e:}");process::exit(1)},};
            let module = match Module::eval_ast_as_new(ctx, &ast,&script.engine) {
                Ok(d) => d, Err(e) => {log::error!("Parsing {str} failed with: {e:}");process::exit(1)},
            };
            script.engine.register_global_module(module.into());
        }
        script
    }

    pub fn from_dir(dir:&PathBuf, stage: &str, ctx: Scope<'static>) -> Script {
        let mut stg = PathBuf::new();
        let mut index = PathBuf::new();
        stg.push(dir.clone());
        stg.push(format!("{}.yaml", stage));
        index.push(dir.clone());
        index.push("index.rhai");
        if Path::new(&stg.clone()).is_file() {
            Self::from(&stg, ctx)
        } else  {
            Self::from(&index, ctx)
        }
    }

    pub fn from_str(code: &str, ctx: Scope<'static>) -> Script {
        let mut script = Self::new(ctx.clone());
        let ast = match script.engine.compile(code) {Ok(d) => d, Err(e) => {log::error!("Loading {code} failed with: {e:}");process::exit(1)},};
        let module = match Module::eval_ast_as_new(ctx, &ast,&script.engine) {
            Ok(d) => d, Err(e) => {log::error!("Parsing {code} failed with: {e:}");process::exit(1)},
        };
        script.engine.register_global_module(module.into());
        script
    }

    pub fn set_context(&mut self, ctx: Scope<'static>) {
        self.ctx = ctx;
    }

    pub fn register<A: 'static, const N: usize, const C: bool, R: Any + Clone, const L: bool, F: RhaiNativeFunc<A, N, C, R, L>+ 'static>(&mut self, name: &str, func: F) {
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
