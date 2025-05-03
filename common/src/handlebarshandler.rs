use crate::{hasheshandlers::Argon, passwordhandler::Passwords, rhai_err, Error, Result, RhaiRes};
use base64::{engine::general_purpose::STANDARD, Engine as _};
use handlebars::{handlebars_helper, Handlebars};
use handlebars_misc_helpers::new_hbs;
use regex::Regex;
pub use serde_json::Value;
use std::{
    fs,
    path::{Path, PathBuf},
};
use tracing::*;
use url::form_urlencoded;
// TODO: improve error management
handlebars_helper!(base64_decode: |arg:Value| String::from_utf8(STANDARD.decode(arg.as_str().unwrap_or_else(|| {
    warn!("handlebars::base64_decode received a non-string parameter: {:?}",arg);
    ""
})).unwrap_or_else(|e| {
    warn!("handlebars::base64_decode failed to decode with: {e:?}");
    vec![]
})).unwrap_or_else(|e| {
    warn!("handlebars::base64_decode failed to convert to string with: {e:?}");
    String::new()
}));
handlebars_helper!(base64_encode: |arg:Value| STANDARD.encode(arg.as_str().unwrap_or_else(|| {
    warn!("handlebars::base64_encode received a non-string parameter: {:?}",arg);
    ""
})));
handlebars_helper!(url_encode: |arg:Value| form_urlencoded::byte_serialize(arg.as_str().unwrap_or_else(|| {
    warn!("handlebars::url_encode received a non-string parameter: {:?}",arg);
    ""
}).as_bytes()).collect::<String>());
handlebars_helper!(to_decimal: |arg:Value| format!("{}", u32::from_str_radix(arg.as_str().unwrap_or_else(|| {
    warn!("handlebars::to_decimal received a non-string parameter: {:?}",arg);
    ""
}), 8).unwrap_or_else(|_| {
    warn!("handlebars::to_decimal received a non-string parameter: {:?}",arg);
    0
})));
handlebars_helper!(header_basic: |username:Value, password:Value| format!("Basic {}",STANDARD.encode(format!("{}:{}",username.as_str().unwrap_or_else(|| {
    warn!("handlebars::header_basic received a non-string username: {:?}",username);
    ""
}),password.as_str().unwrap_or_else(|| {
    warn!("handlebars::header_basic received a non-string password: {:?}",password);
    ""
})))));
handlebars_helper!(argon_hash: |password:Value| Argon::new().hash(password.as_str().unwrap_or_else(|| {
    warn!("handlebars::argon_hash received a non-string password: {:?}",password);
    ""
}).to_string()).unwrap_or_else(|e| {
    warn!("handlebars::argon_hash failed to convert to string with: {e:?}");
    String::new()
}));
handlebars_helper!(bcrypt_hash: |password:Value| crate::hasheshandlers::bcrypt_hash(password.as_str().unwrap_or_else(|| {
    warn!("handlebars::bcrypt_hash received a non-string password: {:?}",password);
    ""
}).to_string()).unwrap_or_else(|e| {
    warn!("handlebars::bcrypt_hash failed to convert to string with: {e:?}");
    String::new()
}));
handlebars_helper!(crc32_hash: |password:Value| crate::hasheshandlers::crc32_hash(password.as_str().unwrap_or_else(|| {
    warn!("handlebars::crc32_hash received a non-string password: {:?}",password);
    ""
}).to_string()));
handlebars_helper!(gen_password: |len:u32| Passwords::new().generate(len.into(), 6, 2, 2));
handlebars_helper!(gen_password_alphanum:  |len:u32| Passwords::new().generate(len.into(), 8, 2, 0));
handlebars_helper!(selector: |ctx: Value, {comp:str=""}| {
    let mut sel = ctx.as_object().unwrap()["instance"].as_object().unwrap()["selector"].as_object().unwrap().clone();
    if !comp.is_empty() {
        sel.insert("app.kubernetes.io/component".into(), Value::from(comp));
    }
    sel
});
handlebars_helper!(labels: |ctx: Value, {comp:str=""}| {
    let mut sel = ctx.as_object().unwrap()["instance"].as_object().unwrap()["labels"].as_object().unwrap().clone();
    if !comp.is_empty() {
        sel.insert("app.kubernetes.io/component".into(), Value::from(comp));
    }
    sel
});
handlebars_helper!(have_crd: |ctx: Value, name: String| {
    ctx.as_object().unwrap()["cluster"].as_object().unwrap()["crds"].as_array().unwrap().iter().any(|crd| *crd==name)
});
handlebars_helper!(concat: |a: Value, b: Value| format!("{}{}", a.as_str().unwrap_or_else(|| {
    warn!("handlebars::concat received a non-string parameter: {:?}", a);
    ""
}),b.as_str().unwrap_or_else(|| {
    warn!("handlebars::concat received a non-string parameter: {:?}", b);
    ""
})));

handlebars_helper!(render_template: |template: String, data: Value| {
    let mut hbs = HandleBars::new();
    hbs.render(&template, &data).unwrap_or_else(|e| {
        warn!("handlebars::render_template failed with: {:?}",e);
        String::new()
    })
});


handlebars_helper!(render_file: |file: String, data: Value| {
    let mut hbs = HandleBars::new();
    match std::fs::read_to_string(file.clone()) {
        Ok(template) => {
            hbs.render_named(&file, &template, &data).unwrap_or_else(|e| {
                warn!("handlebars::render_file failed to render with: {:?}",e);
                String::new()
            })
        },
        Err(e) => {
            warn!("handlebars::render_file failed to read {file} with: {:?}", e);
            String::new()
        }
    }
});

#[derive(Clone, Debug)]
pub struct HandleBars<'a> {
    engine: Handlebars<'a>,
}
impl HandleBars<'_> {
    #[must_use]
    pub fn new() -> HandleBars<'static> {
        let mut engine = new_hbs();
        engine.register_helper("concat", Box::new(concat));
        engine.register_helper("to_decimal", Box::new(to_decimal));
        engine.register_helper("labels_from_ctx", Box::new(labels));
        engine.register_helper("ctx_have_crd", Box::new(have_crd));
        engine.register_helper("selector_from_ctx", Box::new(selector));
        engine.register_helper("base64_decode", Box::new(base64_decode));
        engine.register_helper("base64_encode", Box::new(base64_encode));
        engine.register_helper("header_basic", Box::new(header_basic));
        engine.register_helper("argon_hash", Box::new(argon_hash));
        engine.register_helper("bcrypt_hash", Box::new(bcrypt_hash));
        engine.register_helper("url_encode", Box::new(url_encode));
        engine.register_helper("gen_password", Box::new(gen_password));
        engine.register_helper("gen_password_alphanum", Box::new(gen_password_alphanum));
        engine.register_helper("render_template", Box::new(render_template));
        engine.register_helper("render_file", Box::new(render_file));
        let _ = engine.register_script_helper("image_from_ctx",
            "let root = params[0];let name = params[1];\n\
            let im = root[\"instance\"][\"images\"][name];\n\
            let tag = if (\"tag\" in im && im[\"tag\"] != ()) {im[\"tag\"]} else {root[\"instance\"][\"package\"][\"app_version\"]};
            `${im[\"registry\"]}/${im[\"repository\"]}:${tag}`");
        let _ = engine.register_script_helper(
            "resources_from_ctx",
            "let root = params[0];let name = params[1];\n\
            root[\"instance\"][\"resources\"][name]",
        );
        // TODO helper pour load de fichier
        // TODO: add more helpers
        HandleBars { engine }
    }

    pub fn register_template(&mut self, name: &str, template: &str) -> Result<()> {
        self.engine
            .register_template_string(name, template)
            .map_err(Error::HbsTemplateError)
    }

    pub fn rhai_register_template(&mut self, name: String, template: String) -> RhaiRes<()> {
        self.register_template(name.as_str(), template.as_str())
            .map_err(|e| format!("{e}").into())
    }

    pub fn register_helper_dir(&mut self, directory: PathBuf) -> Result<()> {
        if Path::new(&directory).is_dir() {
            let re_rhai = Regex::new(r"\.rhai$").unwrap();
            for file in fs::read_dir(directory).unwrap() {
                let path = file.unwrap().path();
                let filename = path.file_name().unwrap().to_str().unwrap();
                if re_rhai.is_match(filename) {
                    let name = filename[0..(filename.len() - 5)].to_string();
                    self.engine
                        .register_script_helper_file(&name, path)
                        .map_err(|e| Error::Other(format!("{:?}", e)))?;
                }
            }
            Ok(())
        } else {
            Ok(())
        }
    }

    pub fn rhai_register_helper_dir(&mut self, directory: String) -> RhaiRes<()> {
        self.register_helper_dir(PathBuf::from(directory))
            .map_err(rhai_err)
    }

    pub fn register_partial_dir(&mut self, directory: PathBuf) -> Result<()> {
        if Path::new(&directory).is_dir() {
            let re_rhai = Regex::new(r"\.hbs$").unwrap();
            for file in fs::read_dir(directory).unwrap() {
                let path = file.unwrap().path();
                let filename = path.file_name().unwrap().to_str().unwrap();
                if re_rhai.is_match(filename) {
                    let name = filename[0..(filename.len() - 4)].to_string();
                    let tmpl = std::fs::read_to_string(path).map_err(Error::Stdio)?;
                    tracing::debug!("registering {}", name);
                    self.register_template(&name, &tmpl)?;
                }
            }
            Ok(())
        } else {
            Ok(())
        }
    }

    pub fn rhai_register_partial_dir(&mut self, directory: String) -> RhaiRes<()> {
        self.register_partial_dir(PathBuf::from(directory))
            .map_err(rhai_err)
    }

    pub fn render(&mut self, template: &str, data: &Value) -> Result<String> {
        self.engine
            .render_template(template, data)
            .map_err(Error::HbsRenderError)
    }

    pub fn rhai_render(&mut self, template: String, data: rhai::Map) -> RhaiRes<String> {
        self.engine
            .render_template(template.as_str(), &data)
            .map_err(|e| format!("{e}").into())
    }

    pub fn render_named(&mut self, name: &str, template: &str, data: &Value) -> Result<String> {
        self.engine
            .register_template_string(name, template)
            .map_err(Error::HbsTemplateError)?;
        self.engine.render(name, data).map_err(Error::HbsRenderError)
    }

    pub fn rhai_render_named(&mut self, name: String, template: String, data: rhai::Map) -> RhaiRes<String> {
        self.engine
            .register_template_string(name.as_str(), template)
            .map_err(Error::HbsTemplateError)
            .map_err(rhai_err)?;
        self.engine
            .render(name.as_str(), &data)
            .map_err(|e| format!("{e}").into())
    }
}
