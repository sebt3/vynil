use std::{fs, env, path::{PathBuf, Path}};
use serde::{Serialize, Deserialize};
use serde_yaml;
use serde_json;
use anyhow::{Result, ensure, bail, anyhow};
pub use openapiv3::{Schema, ReferenceOr};
use schemars::{JsonSchema,schema_for_value};
use std::collections::BTreeMap;


#[derive(Serialize, Deserialize, Eq, PartialEq, Clone, Debug, JsonSchema)]
pub struct ComponentMetadata {
    pub name: String,
    pub description: Option<String>,
}
#[derive(Serialize, Deserialize, Eq, PartialEq, Clone, Debug, JsonSchema)]
pub struct ComponentDependency {
    pub dist: Option<String>,
    pub category: String,
    pub component: String,
}
#[derive(Serialize, Deserialize, Eq, PartialEq, Clone, Debug, JsonSchema)]
pub struct Providers {
    pub kubernetes: Option<bool>,
    pub authentik: Option<bool>,
    pub kubectl: Option<bool>,
    pub postgresql: Option<bool>,
    pub mysql: Option<bool>,
    pub restapi: Option<bool>,
    pub http: Option<bool>,
    pub gitea: Option<bool>,
}
#[allow(non_snake_case)]
#[derive(Serialize, Deserialize, Eq, PartialEq, Clone, Debug, JsonSchema)]
pub struct Component {
    pub apiVersion: String,
    pub kind: String,
    pub category: String,
    pub metadata: ComponentMetadata,
    pub options: BTreeMap<String, serde_json::Value>,
    pub dependencies: Option<Vec<ComponentDependency>>,
    pub providers: Option<Providers>,
    pub tfaddtype: Option<bool>,
}

fn merge_json(a: &mut serde_json::Value, b: serde_json::Value) {
    if let serde_json::Value::Object(a) = a {
        if let serde_json::Value::Object(b) = b {
            for (k, v) in b {
                if v.is_null() {
                    a.remove(&k);
                }
                else if k!="items" || v != serde_json::Value::Bool(true) {
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
                    // Copy remaining values
                    if let Some(ref v) = vals {
                        if v.is_object() {
                            if let Some(x) = v.as_object() {
                                for (k, v) in x {
                                    if ! object.contains_key(k) {
                                        object.insert(k.clone(), v.clone());
                                    }
                                }
                            }
                        }
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
            let schema: Schema = serde_json::from_str(serde_json::to_string(val).unwrap().as_str()).map_err(|e| anyhow!("While evaluating options {key} : {e} (value was {:?})", val)).unwrap();
            object.insert(key.clone(), Component::get_values_inner(key, option, &schema));
        }
        object.insert("name".to_string(), serde_json::Value::String(env::var("NAME").unwrap_or_else(|_| self.metadata.name.clone())));
        // TODO: should detect current namespace instead of hard-coding default
        object.insert("namespace".to_string(), serde_json::Value::String(env::var("NAMESPACE").unwrap_or_else(|_| "default".to_string())));
        object
    }

    fn get_tf_type_inner(schema: Schema, use_optional: bool) -> String {
        let kind = &schema.schema_kind;
        if let openapiv3::SchemaKind::Type(t) = kind {
            match t {
                openapiv3::Type::String(_) => {
                    return if use_optional {
                        "optional(string)"
                    } else {
                        "string"
                    }.to_string();
                },
                openapiv3::Type::Number(_) | openapiv3::Type::Integer(_) => {
                    return if use_optional {
                        "optional(number)"
                    } else {
                        "number"
                    }.to_string();
                },
                openapiv3::Type::Boolean{ .. } => { // Boolean
                    return if use_optional {
                        "optional(bool)"
                    } else {
                        "bool"
                    }.to_string();
                }
                openapiv3::Type::Object(objt) => {
                    let mut ret = String::new();
                    for (key, val) in &objt.properties {
                        if let Some(item) = val.clone().into_item() {
                            if ! ret.is_empty() {
                                ret += ", ";
                            }
                            ret += format!("{} = {}", key, Self::get_tf_type_inner(*item, true)).as_str();
                        }
                    }
                    if ret.is_empty() {
                        return if use_optional {
                            "optional(map(any))"
                        } else {
                            "map(any)"
                        }.to_string();
                    }
                    return if use_optional {
                        format!("optional(object({{{}}}))", ret)
                    } else {
                        format!("object({{{}}})", ret)
                    }.to_string();
                },
                openapiv3::Type::Array(arrt) => {
                    if let Some(boxed) = arrt.items.clone() {
                        if let Some(item) = boxed.into_item() {
                            return if use_optional {
                                format!("optional(list({}))", Self::get_tf_type_inner(*item, false))
                            } else {
                                format!("list({})", Self::get_tf_type_inner(*item, false))
                            }.to_string();
                        }
                    }
                    return if use_optional {
                        "optional(list(any))"
                    } else {
                        "list(any)"
                    }.to_string();
                },
            }
        }
        "any".to_string()
    }
    pub fn get_tf_type(&self, key: &str) -> String {
        for (k, val) in &self.options {
            if k == key {
                let schema: Schema = serde_json::from_str(serde_json::to_string(val).unwrap().as_str()).map_err(|e| anyhow!("While evaluating options {key} : {e} (value was {:?})", val)).unwrap();
                return Self::get_tf_type_inner(schema, false)
            }
        }
        "any".to_string()
    }

    pub fn validate(&self) -> Result<()> {
        for (key, val) in &self.options.clone() {
            let _schema: &Schema = &serde_json::from_str(serde_json::to_string(val).unwrap().as_str()).map_err(|e| anyhow!("while checking {key} : {e}"))?;
        }
        Ok(())
    }

    pub fn write_self_to(self, dest:PathBuf) -> Result<()> {
        let mut data = "---
".to_string();
        data.push_str(serde_yaml::to_string(&self).unwrap().as_str());
        fs::write(dest, data).expect("Unable to write file");
        Ok(())
    }
    pub fn update_options_from_defaults(&mut self) -> Result<()> {
        for (key, mut val) in self.options.clone() {
            let schema: &Schema = &serde_json::from_str(serde_json::to_string(&val).unwrap().as_str()).unwrap();
            let mut skip = false; // empty array as default produce failed items, skipping
            if let openapiv3::SchemaKind::Type(openapiv3::Type::Array(_)) = &schema.schema_kind {
                if let Some(opts) = schema.schema_data.default.as_ref() {
                    if let Some(serde_json::Value::Array(data)) = opts.into() {
                        skip = data.is_empty();
                    }
                }
            }
            if skip {
                log::warn!("Skipping option \"{}\" while updating type structure from default values", key);
                log::info!("you should set \"type: array\" and a correct \"items\" definition for option \"{}\" so later validation will work", key);
            } else if let Some(opts) = schema.schema_data.default.as_ref() {
                // That option have a default value, update its properties
                let final_schema = &schema_for_value!(opts).schema;
                let objdef = serde_json::from_str(serde_json::to_string(final_schema)?.as_str())?;
                merge_json( &mut val, objdef);
                add_defaults(&mut val);
                log::debug!("{key} after default : {:}", serde_yaml::to_string(&val).unwrap());
                *self.options.get_mut(key.as_str()).unwrap() = val;
            }
        }
        if self.dependencies.is_none() {
            self.dependencies = Some(Vec::new());
        }
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
    ensure!(["apps", "core", "share", "tech", "meta", "crd", "dbo"].contains(&yaml["category"].as_str().unwrap()), "category is not supported");
    Ok(())
}

// Read the file as type enforced
pub fn read_index(file:&PathBuf) -> Result<Component> {
    let f = match fs::File::open(Path::new(&file)) {Ok(f) => f, Err(e) => bail!("Error {} while opening {}", e, file.display()),};
    match serde_yaml::from_reader(f) {Ok(d) => Ok(d), Err(e) => bail!("Error {} while parsing yaml from: {}", e, file.display()),}
}
