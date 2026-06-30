use crate::{Error, Result, RhaiRes, rhai_err};
use rhai::{Dynamic, Engine, ImmutableString, Map};

/// Parses a YAML string to a `serde_json::Value`.
pub fn yaml_str_to_json(s: &str) -> Result<serde_json::Value> {
    serde_yaml::from_str(s).map_err(|e| Error::YamlError(e.to_string()))
}

/// Serialises any `serde::Serialize` value to a YAML string.
pub fn yaml_serialize_to_string<T: serde::Serialize>(val: &T) -> Result<String> {
    serde_yaml::to_string(val).map_err(|e| Error::YamlError(e.to_string()))
}

/// Serialises a slice of `serde::Serialize` values as a multi-document YAML string.
pub fn yaml_all_serialize_to_string<T: serde::Serialize>(vals: &[T]) -> Result<String> {
    let mut out = String::new();
    for v in vals {
        out.push_str("---\n");
        out.push_str(&serde_yaml::to_string(v).map_err(|e| Error::YamlError(e.to_string()))?);
    }
    Ok(out)
}

pub fn yaml_rhai_register(engine: &mut Engine) {
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
}
