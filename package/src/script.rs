use rhai::{Engine, Scope, Module/*, RhaiNativeFunc */};
use std::{process, path::{PathBuf, Path}};
use anyhow::{Result, bail};
//use core::any::Any;
use crate::shell;
use k8s::{Client, get_client,handlers::{DistribHandler, Ingress, IngressHandler, InstallHandler, SecretHandler, Secret, CustomResourceDefinitionHandler, CustomResourceDefinition},install::Install, distrib::Distrib};
pub use rhai::ImmutableString;
use tokio::runtime::Handle;

pub fn new_base_context(category:String, component:String, instance:String, config:&serde_json::Map<String, serde_json::Value>) -> Scope<'static> {
    let json = serde_json::to_string(config).unwrap();
    let cfg: rhai::Dynamic = serde_json::from_str(&json).unwrap();
    let mut s = Scope::new();
    s.push_constant("instance", instance);
    s.push_constant("component", component);
    s.push_constant("category", category);
    s.push_constant("config", cfg);
    s
}
pub fn new_context(category:String, component:String, instance:String, src:String, dest:String, config:&serde_json::Map<String, serde_json::Value>) -> Scope<'static> {
    let mut s = new_base_context(category,component,instance,config);
    s.push_constant("src", src);
    s.push_constant("dest", dest);
    s
}

#[derive(Debug)]
pub struct Script {
    pub engine: Engine,
    ctx: Scope<'static>
}

fn add_to_engine(engine: &mut Engine, code: &str, ctx: Scope<'static>) {
    match engine.compile(code) {Ok(ast) => {
        match Module::eval_ast_as_new(ctx, &ast,&engine) {Ok(module) => {
            engine.register_global_module(module.into());
        }, Err(e) => {tracing::error!("Parsing {code} failed with: {e:}");},};
    }, Err(e) => {tracing::error!("Loading {code} failed with: {e:}")},};
}

fn create_engine(client: &Client) -> Engine {
    let mut e = Engine::new();
    // Logging
    e.register_fn("log_debug", |s:ImmutableString| tracing::debug!("{s}"));
    e.register_fn("log_info", |s:ImmutableString| tracing::info!("{s}"));
    e.register_fn("log_warn", |s:ImmutableString| tracing::warn!("{s}"));
    e.register_fn("log_error", |s:ImmutableString| tracing::error!("{s}"));
    // lancement de commande shell
    e.register_fn("shell", |s:ImmutableString| {
        shell::run_log_check(&format!("{s}"));
    });
    e.register_fn("sh_value", |s:ImmutableString| {
        shell::get_output(&format!("{s}")).unwrap()
    });
    let cli = client.clone();
    e.register_fn("have_crd", move |name:ImmutableString| -> bool {
        let cl = cli.clone();
        tokio::task::block_in_place(|| {Handle::current().block_on(async move {
            let mut handle = CustomResourceDefinitionHandler::new(&cl);
            handle.have(&name).await
        })})
    });
    let cli = client.clone();
    e.register_fn("get_crd", move |name:ImmutableString| -> CustomResourceDefinition {
        let cl = cli.clone();
        tokio::task::block_in_place(|| {Handle::current().block_on(async move {
            let mut handle = CustomResourceDefinitionHandler::new(&cl);
            handle.get(&name).await.unwrap()
        })})
    });
    let cli = client.clone();
    e.register_fn("have_distrib", move |name:ImmutableString| -> bool {
        let cl = cli.clone();
        tokio::task::block_in_place(|| {Handle::current().block_on(async move {
            let mut handle = DistribHandler::new(&cl);
            handle.have(&name).await
        })})
    });
    let cli = client.clone();
    e.register_fn("get_distrib", move |name:ImmutableString| -> Distrib {
        let cl = cli.clone();
        tokio::task::block_in_place(|| {Handle::current().block_on(async move {
            let mut handle = DistribHandler::new(&cl);
            handle.get(&name).await.unwrap()
        })})
    });
    let cli: Client = client.clone();
    e.register_fn("have_install", move |ns:ImmutableString, name:ImmutableString| -> bool {
        let cl = cli.clone();
        tokio::task::block_in_place(|| {Handle::current().block_on(async move {
            let mut handle = InstallHandler::new(&cl, &ns);
            let ret = handle.have(&name).await;
            ret
        })})
    });
    let cli: Client = client.clone();
    e.register_fn("get_install", move |ns:ImmutableString, name:ImmutableString| -> Install {
        let cl = cli.clone();
        tokio::task::block_in_place(|| {Handle::current().block_on(async move {
            let mut handle = InstallHandler::new(&cl, &ns);
            let ret = handle.get(&name).await.unwrap();
            ret
        })})
    });
    let cli: Client = client.clone();
    e.register_fn("have_ingress", move |ns:ImmutableString, name:ImmutableString| -> bool {
        let cl = cli.clone();
        tokio::task::block_in_place(|| {Handle::current().block_on(async move {
            let mut handle = IngressHandler::new(&cl, &ns);
            handle.have(&name).await
        })})
    });
    let cli: Client = client.clone();
    e.register_fn("get_ingress", move |ns:ImmutableString, name:ImmutableString| -> Ingress {
        let cl = cli.clone();
        tokio::task::block_in_place(|| {Handle::current().block_on(async move {
            let mut handle = IngressHandler::new(&cl, &ns);
            handle.get(&name).await.unwrap()
        })})
    });
    let cli = client.clone();
    e.register_fn("have_secret", move |ns:ImmutableString, name:ImmutableString| -> bool {
        let cl = cli.clone();
        tokio::task::block_in_place(|| {Handle::current().block_on(async move {
            let mut handle = SecretHandler::new(&cl, &ns);
            handle.have(&name).await
        })})
    });
    let cli = client.clone();
    e.register_fn("get_secret", move |ns:ImmutableString, name:ImmutableString| -> Secret {
        let cl = cli.clone();
        tokio::task::block_in_place(|| {Handle::current().block_on(async move {
            let mut handle = SecretHandler::new(&cl, &ns);
            handle.get(&name).await.unwrap()
        })})
    });
    add_to_engine(&mut e, "fn assert(cond, mess) {if (!cond){throw mess}}", Scope::new());
    // TODO: Add an http client (download/get/post/put)
    // TODO: Add a kubectl wrapper
    e
}

impl Script {
    pub fn new(ctx: Scope<'static>) -> Script {
        let cl = futures::executor::block_on(async move {
            get_client().await
        });
        Script {engine: create_engine(&cl), ctx}
    }

    pub fn from(file:&PathBuf, ctx: Scope<'static>) -> Script {
        let mut script = Self::new(ctx.clone());
        if Path::new(&file).is_file() {
            let str = file.as_os_str().to_str().unwrap();
            let ast = match script.engine.compile_file(str.into()) {Ok(d) => d, Err(e) => {tracing::error!("Loading {str} failed with: {e:}");process::exit(1)},};
            let module = match Module::eval_ast_as_new(ctx, &ast,&script.engine) {
                Ok(d) => d, Err(e) => {tracing::error!("Parsing {str} failed with: {e:}");process::exit(1)},
            };
            script.engine.register_global_module(module.into());
        }
        script
    }

    pub fn from_dir(dir:&PathBuf, stage: &str, ctx: Scope<'static>) -> Script {
        let mut stg = PathBuf::new();
        let mut index = PathBuf::new();
        stg.push(dir.clone());
        stg.push(format!("{}.rhai", stage));
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
        add_to_engine(&mut script.engine, code, ctx.clone());
        script
    }

    pub fn set_context(&mut self, ctx: Scope<'static>) {
        self.ctx = ctx;
    }

    /*pub fn register<A: 'static, const N: usize, const C: bool, R: Any + Clone, const L: bool, F: RhaiNativeFunc<A, N, C, R, L>+ SendSync + 'static>(&mut self, name: &str, func: F) {
        self.engine.register_fn(name, func);
    }*/

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
