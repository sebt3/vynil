use std::{fs, env, path::{PathBuf, Path}};
use serde::{Serialize, Deserialize};
use serde_yaml;
use serde_json;
use anyhow::{Result, ensure, bail};
use indexmap::IndexMap;
pub use openapiv3::{Schema, ReferenceOr};
use schemars::schema_for_value;

#[derive(Serialize, Deserialize, Eq, PartialEq, Debug)]
pub struct ComponentMetadata {
    pub name: String,
    pub description: Option<String>,
}
#[allow(non_snake_case)]
#[derive(Serialize, Deserialize, PartialEq, Debug)]
pub struct Component {
    pub apiVersion: String,
    pub kind: String,
    pub category: String,
    pub metadata: ComponentMetadata,
    pub options: IndexMap<String, ReferenceOr<Box<Schema>>>
}

fn merge_json(a: &mut serde_json::Value, b: serde_json::Value) {
    if let serde_json::Value::Object(a) = a {
        if let serde_json::Value::Object(b) = b {
            for (k, v) in b {
                if v.is_null() {
                    a.remove(&k);
                }
                else {
                    merge_json(a.entry(k).or_insert(serde_json::Value::Null), v);
                }
            }
            return;
        }
    }
    *a = b;
}
fn add_defaults(json: &mut serde_json::Value) {
    if json["type"] == "object" {
        for (key, _val) in json.clone()["properties"].as_object().unwrap() {
            json["properties"][key]["default"] = json["default"][key].clone();
            if json["properties"][key]["type"] == "object" {
                add_defaults(&mut json["properties"][key]);
            }
        }
    }
}

fn merge_properties(dest: &mut openapiv3::Schema, from: serde_json::Value) {
    let mut json : serde_json::Value = serde_json::from_str(serde_json::to_string(dest).unwrap().as_str()).unwrap();
    merge_json(&mut json, from);
    add_defaults(&mut json);
    /*if json["type"] == "object" { if let Some(defaults) = dest.schema_data.default.as_ref() {
        log::info!("{:?}",json["default"]);
        for (key, _val) in json.clone()["properties"].as_object().unwrap() {
            json["properties"][key]["default"] = defaults[key].clone();
        }
    }}*/
    *dest = serde_json::from_str(serde_json::to_string(&json).unwrap().as_str()).unwrap();
}

impl Component {
    fn get_values_inner(id: &String, vals: Option<serde_json::Value>, schem: &Schema) -> serde_json::Value {
        let kind = &schem.schema_kind;
        let env_name = format!("OPTION_{}", id);
        let have_value = env::var(&env_name).is_ok();
        let value = if have_value {env::var(&env_name).unwrap()} else {String::new()};
        if let openapiv3::SchemaKind::Type(t) = kind {
            match t {
                openapiv3::Type::String(_) => {
                    if let Some(serde_json::Value::String(str)) = vals {
                        return serde_json::Value::String(str);
                    }
                    if have_value {
                        return serde_json::Value::String(value);
                    } else if let Some(ref data) = schem.schema_data.default {
                        return data.clone();
                    }
                    return serde_json::Value::String(String::new());
                },
                openapiv3::Type::Number(_) | openapiv3::Type::Integer(_) => {
                    if let Some(serde_json::Value::Number(data)) = vals {
                        return serde_json::Value::Number(data);
                    }
                    if have_value {
                        return serde_json::Value::Number(value.parse::<i32>().unwrap().into());
                    } else if let Some(ref data) = schem.schema_data.default {
                        return data.clone();
                    }
                    return serde_json::Value::Number(0.into());
                },
                openapiv3::Type::Object(objt) => {
                    let mut object = serde_json::Map::new();
                    for (key, val) in &objt.properties {
                        let opt = if let Some(ref v) = vals {
                            if v.is_object() {
                                if let Some(x) = v.as_object() {
                                    if x.contains_key(key) {
                                        Some(x[key].clone())
                                    } else {None}
                                } else {None}
                            } else {None}
                        } else {None};
                        //let option = if val.contains_key(&key) {Some(vals[&key])} else {None};
                        let schema = val.as_item().unwrap();
                        let tid = format!("{}_{}",id, key);
                        object.insert(key.clone(), Component::get_values_inner(&tid, opt, schema));
                    }
                    return serde_json::Value::Object(object);
                },
                openapiv3::Type::Array(_) => {
                    if let Some(serde_json::Value::Array(data)) = vals {
                        return serde_json::Value::Array(data);
                    }
                    if let Some(ref data) = schem.schema_data.default {
                        return data.clone();
                    }
                    return serde_json::Value::Array([].into());
                },
                openapiv3::Type::Boolean{ .. } => { // Boolean
                    if let Some(serde_json::Value::Bool(data)) = vals {
                        return serde_json::Value::Bool(data);
                    }
                    if have_value && ["True", "true", "1", "yes", "YES", "OK"].contains(&value.as_str())  {
                        return serde_json::Value::Bool(true);
                    } else if let Some(ref data) = schem.schema_data.default {
                        return data.clone();
                    }
                    return serde_json::Value::Bool(false);
                }
            }
        }
        serde_json::Value::String(String::new())
    }

    pub fn get_values(&mut self, options: &serde_json::Map<String, serde_json::Value>) -> serde_json::Map<String, serde_json::Value> {
        let mut object = serde_json::Map::new();
        for (key, val) in &self.options {
            let option = if options.contains_key(key) {Some(options[key].clone())} else {None};
            let schema = val.as_item().unwrap();
            object.insert(key.clone(), Component::get_values_inner(key, option, schema));
        }
        object.insert("name".to_string(), serde_json::Value::String(env::var("NAME").unwrap_or_else(|_| self.metadata.name.clone())));
        // TODO: should detect current namespace instead of hard-coding default
        object.insert("namespace".to_string(), serde_json::Value::String(env::var("NAMESPACE").unwrap_or_else(|_| "default".to_string())));
        object
    }

    pub fn update_options_from_defaults(mut self, dest:PathBuf) -> Result<()> {
        for (key, mut val) in self.options.clone() {
            if let openapiv3::ReferenceOr::Item(ref mut boxed) = val {
                let schema = boxed.as_mut();
                if let Some(opts) = schema.schema_data.default.as_ref() {
                    // That option have a default value, update its properties
                    let objdef = serde_json::from_str(serde_json::to_string(&schema_for_value!(opts).schema)?.as_str())?;
                    merge_properties( schema, objdef);
                    log::debug!("{key} after merge : {:}", serde_yaml::to_string(&schema).unwrap());
                    // TODO: propagate the default values
                    self.options[&key] = openapiv3::ReferenceOr::Item(Box::new(schema.clone()));
                }
            }
        }
        let mut data = "---
".to_string();
        data.push_str(serde_yaml::to_string(&self).unwrap().as_str());
        fs::write(dest, data).expect("Unable to write file");
        Ok(())
    }
}

pub fn read_yaml(file:&PathBuf) -> Result<serde_yaml::Value> {
    let f = match fs::File::open(Path::new(&file)) {Ok(f) => f, Err(e) => bail!("Error {} while opening {}", e, file.display()),};
    match serde_yaml::from_reader(f) {Ok(d) => Ok(d), Err(e) => bail!("Error {} while parsing yaml from: {}", e, file.display()),}
}

pub fn validate_index(yaml: &serde_yaml::Value) -> Result<()> {
    let kind_opt = yaml["kind"].as_str().map(std::string::ToString::to_string);
    ensure!(kind_opt.is_some(), "This file have no kind");
    let kind = kind_opt.unwrap();
    ensure!(kind == "Component", "{} is an unsupported kind (expected: Component)", kind);
    let version_opt = yaml["apiVersion"].as_str().map(std::string::ToString::to_string);
    ensure!(version_opt.is_some(), "This file have no apiVersion");
    let version = version_opt.unwrap();
    ensure!(version == "vinyl.solidite.fr/v1beta1", "{version} is an unsupported apiVersion (expected: vinyl.solidite.fr/v1beta1)");
    ensure!(yaml["metadata"]["name"].as_str().map(std::string::ToString::to_string).is_some(), "metadata.name is not set");
    ensure!(yaml["category"].as_str().map(std::string::ToString::to_string).is_some(), "category is not set");
    ensure!(["apps", "core", "share", "tech"].contains(&yaml["category"].as_str().unwrap()), "category is not supported");
    Ok(())
}

// Read the file as type enforced
pub fn read_index(file:&PathBuf) -> Result<Component> {
    let f = match fs::File::open(Path::new(&file)) {Ok(f) => f, Err(e) => bail!("Error {} while opening {}", e, file.display()),};
    match serde_yaml::from_reader(f) {Ok(d) => Ok(d), Err(e) => bail!("Error {} while parsing yaml from: {}", e, file.display()),}
}
