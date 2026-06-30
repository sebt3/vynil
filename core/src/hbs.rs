use crate::{Error, Result, RhaiRes, hashes::Argon, rhai_err};
use base64::{Engine as _, engine::general_purpose::STANDARD};
use handlebars::{Handlebars, handlebars_helper};
use handlebars_misc_helpers::new_hbs;
use regex::Regex;
use serde_json::Value;
use std::{fs, path::PathBuf};
use tracing::*;
use url::form_urlencoded;

/// Generic helpers available in core's HandleBars (no vynil context dependency).
pub const CORE_HBS_HELPERS: &[&str] = &[
    // Handlebars built-ins
    "if",
    "unless",
    "each",
    "with",
    "lookup",
    "raw",
    "log",
    "inline",
    "eq",
    "ne",
    "gt",
    "gte",
    "lt",
    "lte",
    "and",
    "or",
    "not",
    "len",
    // handlebars string_helpers feature (case helpers)
    "lowerCamelCase",
    "upperCamelCase",
    "snakeCase",
    "kebabCase",
    "shoutySnakeCase",
    "shoutyKebabCase",
    "titleCase",
    "trainCase",
    // handlebars_misc_helpers — file (unconditional)
    "read_to_str",
    // handlebars_misc_helpers — path (unconditional)
    "parent",
    "file_name",
    "extension",
    "canonicalize",
    // handlebars_misc_helpers — env (unconditional)
    "env_var",
    // handlebars_misc_helpers — string feature
    "to_lower_case",
    "to_upper_case",
    "trim",
    "trim_start",
    "trim_end",
    "replace",
    "quote",
    "unquote",
    "first_non_empty",
    // handlebars_misc_helpers — json feature
    "json_to_str",
    "str_to_json",
    "from_json",
    "to_json",
    "json_query",
    "json_str_query",
    // handlebars_misc_helpers — jsonnet feature
    "jsonnet",
    // handlebars_misc_helpers — regex feature
    "regex_captures",
    "regex_is_match",
    // handlebars_misc_helpers — uuid feature
    "uuid_new_v4",
    "uuid_new_v7",
    // vynil core helpers
    "base64_encode",
    "base64_decode",
    "url_encode",
    "to_decimal",
    "header_basic",
    "argon_hash",
    "bcrypt_hash",
    "crc32_hash",
    "gen_password",
    "gen_password_alphanum",
    "gen_private_key",
    "concat",
];

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
handlebars_helper!(bcrypt_hash: |password:Value| crate::hashes::bcrypt_hash(password.as_str().unwrap_or_else(|| {
    warn!("handlebars::bcrypt_hash received a non-string password: {:?}",password);
    ""
}).to_string()).unwrap_or_else(|e| {
    warn!("handlebars::bcrypt_hash failed to convert to string with: {e:?}");
    String::new()
}));
handlebars_helper!(crc32_hash: |password:Value| crate::hashes::crc32_hash(password.as_str().unwrap_or_else(|| {
    warn!("handlebars::crc32_hash received a non-string password: {:?}",password);
    ""
}).to_string()));
handlebars_helper!(gen_password: |len:u32, {lower:u32=1, upper:u32=1, digits:u32=1, symbols:u32=1}| crate::password::generate(len as usize, lower as usize, upper as usize, digits as usize, symbols as usize).unwrap_or_else(|e| {
    warn!("handlebars::gen_password failed with: {e:?}");
    String::new()
}));
handlebars_helper!(gen_password_alphanum: |len:u32| crate::password::generate(len as usize, 1, 1, 1, 0).unwrap_or_else(|e| {
    warn!("handlebars::gen_password_alphanum failed with: {e:?}");
    String::new()
}));
handlebars_helper!(gen_private_key: |algo:str, {bits:u32=4096}| crate::key::gen_private_key(algo, bits).unwrap_or_else(|e| {
    warn!("handlebars::gen_private_key failed with: {e:?}");
    String::new()
}));
handlebars_helper!(concat: |a: Value, b: Value| format!("{}{}", a.as_str().unwrap_or_else(|| {
    warn!("handlebars::concat received a non-string parameter: {:?}", a);
    ""
}),b.as_str().unwrap_or_else(|| {
    warn!("handlebars::concat received a non-string parameter: {:?}", b);
    ""
})));

#[derive(Clone, Debug)]
pub struct HandleBars<'a> {
    engine: Handlebars<'a>,
}
impl<'a> HandleBars<'a> {
    #[must_use]
    pub fn new() -> HandleBars<'static> {
        let mut engine = new_hbs();
        engine.register_helper("concat", Box::new(concat));
        engine.register_helper("to_decimal", Box::new(to_decimal));
        engine.register_helper("base64_decode", Box::new(base64_decode));
        engine.register_helper("base64_encode", Box::new(base64_encode));
        engine.register_helper("header_basic", Box::new(header_basic));
        engine.register_helper("argon_hash", Box::new(argon_hash));
        engine.register_helper("bcrypt_hash", Box::new(bcrypt_hash));
        engine.register_helper("url_encode", Box::new(url_encode));
        engine.register_helper("gen_password", Box::new(gen_password));
        engine.register_helper("gen_password_alphanum", Box::new(gen_password_alphanum));
        engine.register_helper("gen_private_key", Box::new(gen_private_key));
        engine.register_helper("crc32_hash", Box::new(crc32_hash));
        HandleBars { engine }
    }

    #[must_use]
    pub fn engine_mut(&mut self) -> &mut Handlebars<'a> {
        &mut self.engine
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
        if std::path::Path::new(&directory).is_dir() {
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
        if std::path::Path::new(&directory).is_dir() {
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

    pub fn render(&mut self, template: &str, data: &serde_json::Value) -> Result<String> {
        self.engine
            .render_template(template, data)
            .map_err(Error::HbsRenderError)
    }

    pub fn rhai_render(&mut self, template: String, data: rhai::Map) -> RhaiRes<String> {
        let json_data: serde_json::Value =
            serde_json::from_str(&serde_json::to_string(&data).map_err(|e| format!("{e}"))?)
                .map_err(|e| format!("{e}"))?;
        self.engine
            .render_template(template.as_str(), &json_data)
            .map_err(|e| format!("{e}").into())
    }

    pub fn render_named(&mut self, name: &str, template: &str, data: &serde_json::Value) -> Result<String> {
        self.engine
            .register_template_string(name, template)
            .map_err(Error::HbsTemplateError)?;
        self.engine.render(name, data).map_err(Error::HbsRenderError)
    }

    pub fn rhai_render_named(&mut self, name: String, template: String, data: rhai::Map) -> RhaiRes<String> {
        let json_data: serde_json::Value =
            serde_json::from_str(&serde_json::to_string(&data).map_err(|e| format!("{e}"))?)
                .map_err(|e| format!("{e}"))?;
        self.engine
            .register_template_string(name.as_str(), template)
            .map_err(Error::HbsTemplateError)
            .map_err(rhai_err)?;
        self.engine
            .render(name.as_str(), &json_data)
            .map_err(|e| format!("{e}").into())
    }
}
