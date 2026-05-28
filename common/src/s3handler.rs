use crate::{Error, RhaiRes, rhai_err};
use futures::StreamExt;
use object_store::{ObjectStore, path::Path};
use rhai::{Dynamic, Engine};
use tokio::{runtime::Handle, task::block_in_place};

fn build_store(
    bucket: &str,
    region: &str,
    endpoint: &str,
    access_key: &str,
    secret_key: &str,
) -> Result<Box<dyn ObjectStore>, Error> {
    use object_store::aws::AmazonS3Builder;
    let mut builder = AmazonS3Builder::new()
        .with_bucket_name(bucket)
        .with_region(region);
    if !access_key.is_empty() {
        builder = builder
            .with_access_key_id(access_key)
            .with_secret_access_key(secret_key);
    }
    if !endpoint.is_empty() {
        builder = builder.with_endpoint(endpoint).with_allow_http(true);
    }
    let store = builder.build().map_err(|e| Error::Other(e.to_string()))?;
    Ok(Box::new(store))
}

pub fn s3_get_yaml(
    bucket: String,
    region: String,
    prefix: String,
    endpoint: String,
    access_key: String,
    secret_key: String,
    key: String,
) -> RhaiRes<Dynamic> {
    block_in_place(|| {
        Handle::current().block_on(async move {
            let store = build_store(&bucket, &region, &endpoint, &access_key, &secret_key)?;
            let full_key = format!("{}{}", prefix, key);
            let path = Path::from(full_key.as_str());
            let result = store.get(&path).await.map_err(|e| Error::Other(e.to_string()))?;
            let bytes = result.bytes().await.map_err(|e| Error::Other(e.to_string()))?;
            let body = String::from_utf8(bytes.to_vec()).map_err(Error::UTF8)?;
            let value: serde_yaml::Value =
                serde_yaml::from_str(&body).map_err(|e| Error::YamlError(e.to_string()))?;
            let json = serde_json::to_string(&value).map_err(Error::SerializationError)?;
            serde_json::from_str::<Dynamic>(&json).map_err(Error::SerializationError)
        })
    })
    .map_err(rhai_err)
}

pub fn s3_list_keys(
    bucket: String,
    region: String,
    prefix: String,
    endpoint: String,
    access_key: String,
    secret_key: String,
) -> RhaiRes<Vec<String>> {
    block_in_place(|| {
        Handle::current().block_on(async move {
            let store = build_store(&bucket, &region, &endpoint, &access_key, &secret_key)?;
            let prefix_path = Path::from(prefix.as_str());
            let mut list = store.list(Some(&prefix_path));
            let mut keys = vec![];
            while let Some(meta) = list.next().await {
                let meta = meta.map_err(|e: object_store::Error| Error::Other(e.to_string()))?;
                keys.push(meta.location.to_string());
            }
            Ok(keys)
        })
    })
    .map_err(rhai_err)
}

pub fn s3_rhai_register(engine: &mut Engine) {
    engine
        .register_fn("s3_get_yaml", s3_get_yaml)
        .register_fn("s3_list_keys", s3_list_keys);
}

#[cfg(test)]
mod tests {
    use super::*;
    use object_store::{PutPayload, memory::InMemory, path::Path};

    async fn setup_store_with_yaml(key: &str, yaml: &str) -> InMemory {
        let store = InMemory::new();
        store
            .put(&Path::from(key), PutPayload::from(yaml.as_bytes().to_vec()))
            .await
            .unwrap();
        store
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn test_s3_get_yaml_with_inmemory() {
        let yaml = "packages:\n  - name: mypkg\n";
        let store = setup_store_with_yaml("mybucket/index.yaml", yaml).await;

        let result = {
            let store: Box<dyn ObjectStore> = Box::new(store);
            let path = Path::from("mybucket/index.yaml");
            let r = store.get(&path).await.unwrap();
            let bytes = r.bytes().await.unwrap();
            let body = String::from_utf8(bytes.to_vec()).unwrap();
            let value: serde_yaml::Value = serde_yaml::from_str(&body).unwrap();
            let json = serde_json::to_string(&value).unwrap();
            serde_json::from_str::<Dynamic>(&json).unwrap()
        };
        assert!(result.is_map());
        let map = result.cast::<rhai::Map>();
        assert!(map.contains_key("packages"));
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn test_s3_list_keys_with_inmemory() {
        let store = InMemory::new();
        for key in &["prefix/a.yaml", "prefix/b.yaml", "prefix/c.yaml"] {
            store
                .put(&Path::from(*key), PutPayload::from(b"key: val".to_vec()))
                .await
                .unwrap();
        }

        let store: Box<dyn ObjectStore> = Box::new(store);
        let prefix_path = Path::from("prefix/");
        let mut list = store.list(Some(&prefix_path));
        let mut keys = vec![];
        while let Some(meta) = list.next().await {
            keys.push(meta.unwrap().location.to_string());
        }
        keys.sort();
        assert_eq!(keys, vec!["prefix/a.yaml", "prefix/b.yaml", "prefix/c.yaml"]);
    }
}
