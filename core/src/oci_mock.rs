use crate::RhaiRes;
use rhai::{Dynamic, Engine};

#[derive(Clone, Debug)]
pub struct OciRegistryMock;

impl OciRegistryMock {
    pub fn list_tags(&mut self, _repository: String) -> RhaiRes<Dynamic> {
        Ok(Dynamic::from_array(vec![]))
    }

    pub fn get_manifest(&mut self, _repository: String, _tag: String) -> RhaiRes<Dynamic> {
        let mut map = rhai::Map::new();
        map.insert("annotations".into(), Dynamic::from_map(rhai::Map::new()));
        Ok(Dynamic::from_map(map))
    }

    pub fn push_image(
        &mut self,
        _dir: String,
        _repo: String,
        _tag: String,
        _ann: Dynamic,
    ) -> RhaiRes<rhai::ImmutableString> {
        Ok("sha256:mock-digest-for-testing".into())
    }

    pub fn sign_image(&mut self, _repo: String, _tag: String, _digest: String, _key: String) -> RhaiRes<()> {
        Ok(())
    }
}

pub fn oci_mock_rhai_register(engine: &mut Engine) {
    engine
        .register_type_with_name::<OciRegistryMock>("OciRegistryMock")
        .register_fn("new_registry", |_: String, _: String, _: String| OciRegistryMock)
        .register_fn("list_tags", OciRegistryMock::list_tags)
        .register_fn("get_manifest", OciRegistryMock::get_manifest)
        .register_fn("push_image", OciRegistryMock::push_image)
        .register_fn("sign_image", OciRegistryMock::sign_image);
}
