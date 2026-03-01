use crate::{Error, RhaiRes, rhai_err};
use indexmap::IndexMap;
use rhai::{Dynamic, Engine, ImmutableString, Map};
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

// ── Order-preserving YAML document type (rust-yaml) ───────────────────────────
//
// Used exclusively by `yaml_decode_ordered` / `yaml_encode_ordered`, which are
// called from agent/scripts/packages/update.rhai so that package.yaml key order
// is preserved when image tags are bumped.

/// A YAML document that preserves key insertion order (backed by IndexMap).
#[derive(Debug, Clone)]
pub struct YamlDoc(pub Value);

impl YamlDoc {
    pub fn from_str(s: &str) -> Result<Self, String> {
        new_yaml().load_str(s).map(YamlDoc).map_err(|e| e.to_string())
    }

    pub fn to_yaml_string(&self) -> Result<String, String> {
        new_yaml().dump_str(&self.0).map_err(|e| e.to_string())
    }

    // ── Rhai indexer get ──────────────────────────────────────────────────
    pub fn idx_get(&mut self, key: ImmutableString) -> Dynamic {
        match &self.0 {
            Value::Mapping(m) => {
                let k = Value::String(key.to_string());
                m.get(&k)
                    .map(|v| value_to_dynamic(v.clone()))
                    .unwrap_or(Dynamic::UNIT)
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
        if let Value::Mapping(m) = &mut self.0 {
            m.insert(Value::String(key.to_string()), yaml_val);
        }
    }

    // ── Rhai methods ──────────────────────────────────────────────────────

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

    pub fn values(&mut self) -> rhai::Array {
        match &self.0 {
            Value::Mapping(m) => m.values().map(|v| value_to_dynamic(v.clone())).collect(),
            _ => rhai::Array::new(),
        }
    }

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
// Used internally by YamlDoc indexers.

pub fn value_to_dynamic(v: Value) -> Dynamic {
    match v {
        Value::Null => Dynamic::UNIT,
        Value::Bool(b) => Dynamic::from(b),
        Value::Int(i) => Dynamic::from(i),
        Value::Float(f) => Dynamic::from(f),
        Value::String(s) => Dynamic::from(ImmutableString::from(s.as_str())),
        Value::Sequence(a) => Dynamic::from_array(a.into_iter().map(value_to_dynamic).collect()),
        Value::Mapping(_) => Dynamic::from(YamlDoc(v)),
    }
}

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
        // Fallback: JSON round-trip
        serde_json::to_string(&d)
            .ok()
            .and_then(|s| serde_json::from_str::<serde_json::Value>(&s).ok())
            .map(serde_json_value_to_yaml_value)
            .unwrap_or(Value::Null)
    }
}

// Used only by the dynamic_to_value fallback path above.
fn serde_json_value_to_yaml_value(v: serde_json::Value) -> Value {
    match v {
        serde_json::Value::Null => Value::Null,
        serde_json::Value::Bool(b) => Value::Bool(b),
        serde_json::Value::Number(n) => {
            if let Some(i) = n.as_i64() { Value::Int(i) }
            else if let Some(f) = n.as_f64() { Value::Float(f) }
            else { Value::String(n.to_string()) }
        }
        serde_json::Value::String(s) => Value::String(s),
        serde_json::Value::Array(a) => {
            Value::Sequence(a.into_iter().map(serde_json_value_to_yaml_value).collect())
        }
        serde_json::Value::Object(m) => {
            let indexmap: IndexMap<Value, Value> = m
                .into_iter()
                .map(|(k, v)| (Value::String(k), serde_json_value_to_yaml_value(v)))
                .collect();
            Value::Mapping(indexmap)
        }
    }
}

// ── Public helpers (backed by serde_yaml) ────────────────────────────────────

/// Parses a YAML string to a `serde_json::Value`.
pub fn yaml_str_to_json(s: &str) -> crate::Result<serde_json::Value> {
    serde_yaml::from_str(s).map_err(|e| Error::YamlError(e.to_string()))
}

/// Serialises any `serde::Serialize` value to a YAML string.
pub fn yaml_serialize_to_string<T: serde::Serialize>(val: &T) -> crate::Result<String> {
    serde_yaml::to_string(val).map_err(|e| Error::YamlError(e.to_string()))
}

/// Serialises a slice of `serde::Serialize` values as a multi-document YAML string.
pub fn yaml_all_serialize_to_string<T: serde::Serialize>(vals: &[T]) -> crate::Result<String> {
    let mut out = String::new();
    for v in vals {
        out.push_str("---\n");
        out.push_str(&serde_yaml::to_string(v).map_err(|e| Error::YamlError(e.to_string()))?);
    }
    Ok(out)
}

// ── Rhai registration ─────────────────────────────────────────────────────────

pub fn yaml_rhai_register(engine: &mut Engine) {
    // ── Standard functions (serde_yaml, alphabetical key order) ──────────
    engine
        .register_fn("yaml_encode", |val: Dynamic| -> RhaiRes<ImmutableString> {
            serde_yaml::to_string(&val)
                .map_err(|e| rhai_err(Error::YamlError(e.to_string())))
                .map(|s| s.into())
        })
        .register_fn("yaml_encode", |val: Map| -> RhaiRes<ImmutableString> {
            serde_yaml::to_string(&val)
                .map_err(|e| rhai_err(Error::YamlError(e.to_string())))
                .map(|s| s.into())
        })
        .register_fn("yaml_decode", |val: ImmutableString| -> RhaiRes<Dynamic> {
            serde_yaml::from_str(val.as_ref()).map_err(|e| rhai_err(Error::YamlError(e.to_string())))
        })
        .register_fn(
            "yaml_decode_multi",
            |val: ImmutableString| -> RhaiRes<Vec<Dynamic>> {
                if val.len() <= 5 {
                    return Ok(vec![]);
                }
                let mut result = Vec::new();
                for doc in serde_yaml::Deserializer::from_str(val.as_ref()) {
                    let d: Dynamic = serde::Deserialize::deserialize(doc)
                        .map_err(|e| rhai_err(Error::YamlError(e.to_string())))?;
                    result.push(d);
                }
                Ok(result)
            },
        );

    // ── Order-preserving functions (rust-yaml / YamlDoc) ─────────────────
    // Used exclusively by agent/scripts/packages/update.rhai.
    engine
        .register_fn("yaml_decode_ordered", |val: ImmutableString| -> RhaiRes<YamlDoc> {
            YamlDoc::from_str(val.as_ref()).map_err(|e| rhai_err(Error::YamlError(e)))
        })
        .register_fn("yaml_encode_ordered", |val: Dynamic| -> RhaiRes<ImmutableString> {
            let yaml_val = dynamic_to_value(val);
            new_yaml()
                .dump_str(&yaml_val)
                .map_err(|e| rhai_err(Error::YamlError(e.to_string())))
                .map(|s| s.into())
        })
        .register_fn("yaml_encode_ordered", |yd: YamlDoc| -> RhaiRes<ImmutableString> {
            new_yaml()
                .dump_str(&yd.0)
                .map_err(|e| rhai_err(Error::YamlError(e.to_string())))
                .map(|s| s.into())
        });

    // ── YamlDoc type registration ─────────────────────────────────────────
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
