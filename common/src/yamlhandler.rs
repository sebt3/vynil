use crate::{Error, RhaiRes, rhai_err};
use indexmap::IndexMap;
use rhai::{Dynamic, ImmutableString, Engine, Map};
use rust_yaml::{Value, Yaml, YamlConfig, yaml::IndentConfig};

fn new_yaml() -> Yaml {
    Yaml::with_config(YamlConfig {
        preserve_comments: true,
        loader_type: rust_yaml::LoaderType::RoundTrip,
        emit_anchors: false,
        indent: IndentConfig {
            indent: 2,
            sequence_indent: Some(0),
            ..Default::default()
        },
        ..Default::default()
    })
}

// ── Public order-preserving YAML document type ────────────────────────────────

/// A YAML document that preserves key insertion order (backed by IndexMap).
/// This type is registered in the Rhai engine so scripts can manipulate
/// YAML files and write them back without scrambling key order.
#[derive(Debug, Clone)]
pub struct YamlDoc(pub Value);

impl YamlDoc {
    pub fn from_str(s: &str) -> Result<Self, String> {
        new_yaml()
            .load_str(s)
            .map(YamlDoc)
            .map_err(|e| e.to_string())
    }

    pub fn from_str_multi(s: &str) -> Result<Vec<Self>, String> {
        new_yaml()
            .load_all_str(s)
            .map(|docs| docs.into_iter().map(YamlDoc).collect())
            .map_err(|e| e.to_string())
    }

    pub fn to_yaml_string(&self) -> Result<String, String> {
        new_yaml().dump_str(&self.0).map_err(|e| e.to_string())
    }

    // ── Rhai indexer get ──────────────────────────────────────────────────
    // Returns a clone of the sub-document as a Dynamic.
    // - Mapping  → Dynamic::from(YamlDoc)  (type_of == "map")
    // - Sequence → Dynamic::Array          (type_of == "array")
    // - String   → Dynamic::ImmutableString
    // - Int      → Dynamic::INT
    // - Float    → Dynamic::FLOAT
    // - Bool     → Dynamic::bool
    // - Null     → Dynamic::UNIT
    pub fn idx_get(&mut self, key: ImmutableString) -> Dynamic {
        match &self.0 {
            Value::Mapping(m) => {
                let k = Value::String(key.to_string());
                m.get(&k).map(|v| value_to_dynamic(v.clone())).unwrap_or(Dynamic::UNIT)
            }
            Value::Sequence(s) => key
                .parse::<usize>()
                .ok()
                .and_then(|i| s.get(i))
                .map(|v| value_to_dynamic(v.clone()))
                .unwrap_or(Dynamic::UNIT),
            _ => Dynamic::UNIT,
        }
    }

    // ── Rhai indexer set ──────────────────────────────────────────────────
    pub fn idx_set(&mut self, key: ImmutableString, val: Dynamic) {
        let yaml_val = dynamic_to_value(val);
        match &mut self.0 {
            Value::Mapping(m) => {
                m.insert(Value::String(key.to_string()), yaml_val);
            }
            _ => {}
        }
    }

    // ── Rhai methods ──────────────────────────────────────────────────────

    /// Returns the keys of a YAML mapping as a Rhai array of strings.
    pub fn keys(&mut self) -> rhai::Array {
        match &self.0 {
            Value::Mapping(m) => m
                .keys()
                .filter_map(|k| {
                    if let Value::String(s) = k {
                        Some(Dynamic::from(ImmutableString::from(s.as_str())))
                    } else {
                        None
                    }
                })
                .collect(),
            _ => rhai::Array::new(),
        }
    }

    /// Returns the values of a YAML mapping as a Rhai array.
    pub fn values(&mut self) -> rhai::Array {
        match &self.0 {
            Value::Mapping(m) => m.values().map(|v| value_to_dynamic(v.clone())).collect(),
            _ => rhai::Array::new(),
        }
    }

    /// Support `"key" in yaml_doc`.
    pub fn contains_key(&self, key: ImmutableString) -> bool {
        match &self.0 {
            Value::Mapping(m) => m.contains_key(&Value::String(key.to_string())),
            _ => false,
        }
    }

    pub fn len(&self) -> i64 {
        match &self.0 {
            Value::Mapping(m) => m.len() as i64,
            Value::Sequence(s) => s.len() as i64,
            _ => 0,
        }
    }

    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }
}

// ── Conversions: rust_yaml::Value ↔ Rhai Dynamic ─────────────────────────────

/// Converts a rust-yaml `Value` to a Rhai `Dynamic`.
/// Mappings become `YamlDoc` (preserves order).
/// Sequences become Rhai `Array`.
/// Primitives become their native Rhai types.
pub fn value_to_dynamic(v: Value) -> Dynamic {
    match v {
        Value::Null => Dynamic::UNIT,
        Value::Bool(b) => Dynamic::from(b),
        Value::Int(i) => Dynamic::from(i),
        Value::Float(f) => Dynamic::from(f),
        Value::String(s) => Dynamic::from(ImmutableString::from(s.as_str())),
        Value::Sequence(a) => {
            Dynamic::from_array(a.into_iter().map(value_to_dynamic).collect())
        }
        Value::Mapping(_) => Dynamic::from(YamlDoc(v)),
    }
}

/// Converts a rust-yaml `Value` to a plain Rhai `Dynamic` where mappings
/// become Rhai `Map` (BTreeMap, unsorted keys) instead of `YamlDoc`.
/// Used by `yaml_decode` / `yaml_decode_multi` for backwards compatibility
/// with scripts that pass objects to the k8s API.
pub fn value_to_rhai_dynamic(v: Value) -> Dynamic {
    match v {
        Value::Null => Dynamic::UNIT,
        Value::Bool(b) => Dynamic::from(b),
        Value::Int(i) => Dynamic::from(i),
        Value::Float(f) => Dynamic::from(f),
        Value::String(s) => Dynamic::from(ImmutableString::from(s.as_str())),
        Value::Sequence(a) => {
            Dynamic::from_array(a.into_iter().map(value_to_rhai_dynamic).collect())
        }
        Value::Mapping(m) => {
            let rhai_map: rhai::Map = m
                .into_iter()
                .filter_map(|(k, v)| {
                    if let Value::String(s) = k {
                        Some((s.into(), value_to_rhai_dynamic(v)))
                    } else {
                        None
                    }
                })
                .collect();
            Dynamic::from_map(rhai_map)
        }
    }
}

/// Converts a Rhai `Dynamic` to a rust-yaml `Value`.
/// Handles `YamlDoc`, `Map` (Rhai BTreeMap), `Array`, and primitives.
pub fn dynamic_to_value(d: Dynamic) -> Value {
    if d.is_unit() {
        Value::Null
    } else if d.is::<YamlDoc>() {
        d.cast::<YamlDoc>().0
    } else if d.is::<bool>() {
        Value::Bool(d.cast::<bool>())
    } else if d.is::<i64>() {
        Value::Int(d.cast::<i64>())
    } else if d.is::<f64>() {
        Value::Float(d.cast::<f64>())
    } else if d.is::<ImmutableString>() {
        Value::String(d.cast::<ImmutableString>().to_string())
    } else if d.is::<String>() {
        Value::String(d.cast::<String>())
    } else if d.is::<rhai::Array>() {
        let arr = d.cast::<rhai::Array>();
        Value::Sequence(arr.into_iter().map(dynamic_to_value).collect())
    } else if d.is::<rhai::Map>() {
        let map = d.cast::<rhai::Map>();
        let indexmap: IndexMap<Value, Value> = map
            .into_iter()
            .map(|(k, v)| (Value::String(k.to_string()), dynamic_to_value(v)))
            .collect();
        Value::Mapping(indexmap)
    } else {
        // Fallback: try JSON round-trip
        serde_json::to_string(&d)
            .ok()
            .and_then(|s| serde_json::from_str::<serde_json::Value>(&s).ok())
            .map(serde_json_to_yaml_value)
            .unwrap_or(Value::Null)
    }
}

// ── Conversions: rust_yaml::Value ↔ serde_json::Value ────────────────────────

/// Converts a rust-yaml `Value` to a `serde_json::Value`.
/// Used for deserializing YAML into Rust structs via serde_json.
pub fn yaml_value_to_serde_json(v: Value) -> serde_json::Value {
    match v {
        Value::Null => serde_json::Value::Null,
        Value::Bool(b) => serde_json::Value::Bool(b),
        Value::Int(i) => serde_json::Value::Number(i.into()),
        Value::Float(f) => serde_json::Number::from_f64(f)
            .map(serde_json::Value::Number)
            .unwrap_or(serde_json::Value::Null),
        Value::String(s) => serde_json::Value::String(s),
        Value::Sequence(a) => {
            serde_json::Value::Array(a.into_iter().map(yaml_value_to_serde_json).collect())
        }
        Value::Mapping(m) => {
            let map: serde_json::Map<String, serde_json::Value> = m
                .into_iter()
                .map(|(k, v)| {
                    let key = match k {
                        Value::String(s) => s,
                        other => format!("{other:?}"),
                    };
                    (key, yaml_value_to_serde_json(v))
                })
                .collect();
            serde_json::Value::Object(map)
        }
    }
}

/// Converts a `serde_json::Value` to a rust-yaml `Value`.
/// Used for serializing Rust structs (via serde_json) to YAML output.
pub fn serde_json_to_yaml_value(v: serde_json::Value) -> Value {
    match v {
        serde_json::Value::Null => Value::Null,
        serde_json::Value::Bool(b) => Value::Bool(b),
        serde_json::Value::Number(n) => {
            if let Some(i) = n.as_i64() {
                Value::Int(i)
            } else if let Some(f) = n.as_f64() {
                Value::Float(f)
            } else {
                Value::String(n.to_string())
            }
        }
        serde_json::Value::String(s) => Value::String(s),
        serde_json::Value::Array(a) => {
            Value::Sequence(a.into_iter().map(serde_json_to_yaml_value).collect())
        }
        serde_json::Value::Object(m) => {
            let indexmap: IndexMap<Value, Value> = m
                .into_iter()
                .map(|(k, v)| (Value::String(k), serde_json_to_yaml_value(v)))
                .collect();
            Value::Mapping(indexmap)
        }
    }
}

// ── Convenience helpers ────────────────────────────────────────────────────────

/// Parses a YAML string and returns the first document as a `serde_json::Value`.
/// Used to replace `serde_yaml::from_str::<serde_json::Value>`.
pub fn yaml_str_to_json(s: &str) -> crate::Result<serde_json::Value> {
    new_yaml()
        .load_str(s)
        .map(yaml_value_to_serde_json)
        .map_err(|e| Error::YamlError(e.to_string()))
}

/// Serialises any `serde::Serialize` value to a YAML string.
/// Used to replace `serde_yaml::to_string`.
pub fn yaml_serialize_to_string<T: serde::Serialize>(val: &T) -> crate::Result<String> {
    let json_val = serde_json::to_value(val).map_err(Error::SerializationError)?;
    let yaml_val = serde_json_to_yaml_value(json_val);
    new_yaml()
        .dump_str(&yaml_val)
        .map_err(|e| Error::YamlError(e.to_string()))
}

pub fn yaml_rhai_register(engine: &mut Engine) {
    engine
        .register_fn("yaml_encode", |val: Dynamic| -> RhaiRes<ImmutableString> {
            let yaml_val = dynamic_to_value(val);
            new_yaml()
                .dump_str(&yaml_val)
                .map_err(|e| rhai_err(Error::YamlError(e.to_string())))
                .map(|v| v.into())
        })
        .register_fn("yaml_encode", |val: Map| -> RhaiRes<ImmutableString> {
            let dyn_val = Dynamic::from_map(val);
            let yaml_val = dynamic_to_value(dyn_val);
            new_yaml()
                .dump_str(&yaml_val)
                .map_err(|e| rhai_err(Error::YamlError(e.to_string())))
                .map(|v| v.into())
        })
        // yaml_decode returns an order-preserving YamlDoc (backed by IndexMap).
        // yaml_encode accepts YamlDoc or any Dynamic (Dynamic overload above handles both).
        .register_fn("yaml_decode", |val: ImmutableString| -> RhaiRes<YamlDoc> {
            YamlDoc::from_str(val.as_ref()).map_err(|e| rhai_err(Error::YamlError(e)))
        })
        .register_fn(
            "yaml_decode_multi",
            // Returns plain Rhai Map objects so they can be passed to k8s API calls
            // (rhai::serde::from_dynamic) without extra conversion.
            |val: ImmutableString| -> RhaiRes<Vec<Dynamic>> {
                if val.len() <= 5 {
                    return Ok(vec![]);
                }
                new_yaml()
                    .load_all_str(val.as_ref())
                    .map(|docs| docs.into_iter().map(value_to_rhai_dynamic).collect())
                    .map_err(|e| rhai_err(Error::YamlError(e.to_string())))
            },
        );
    engine
        .register_type_with_name::<YamlDoc>("map")
        .register_indexer_get(YamlDoc::idx_get)
        .register_indexer_set(YamlDoc::idx_set)
        .register_fn("keys", YamlDoc::keys)
        .register_fn("values", YamlDoc::values)
        .register_fn("len", YamlDoc::len)
        .register_fn("is_empty", YamlDoc::is_empty)
        .register_fn("contains", YamlDoc::contains_key)
        .register_fn("to_string", |yd: &mut YamlDoc| -> String {
            yd.to_yaml_string().unwrap_or_default()
        });
}
