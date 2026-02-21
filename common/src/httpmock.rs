use rhai::{Engine, Map, Dynamic};
use crate::RhaiRes;

#[derive(Clone, Debug, PartialEq)]
pub enum HttpMethod {
    Head,
    Get,
    Delete,
    Patch,
    Post,
    Put,
}

#[derive(Clone, Debug)]
pub struct HttpMockItem {
    path: String,
    method: HttpMethod,
    return_obj: Map,
}

#[derive(Clone, Debug)]
pub struct RestClientMock {
    baseurl: String,
    mocks: Vec<HttpMockItem>,
}
impl RestClientMock {
    #[must_use]
    pub fn new(base: &str, mocks: Vec<HttpMockItem>) -> Self {
        Self {
            baseurl: base.to_string(),
            mocks: mocks,
        }
    }

    pub fn set_server_ca(&mut self, _ca: &str) {}
    pub fn set_mtls(&mut self, _cert: &str, _key: &str) {}
    pub fn headers_reset(&mut self) {}
    pub fn add_header(&mut self, _key: String, _value: String) {}
    pub fn add_header_json(&mut self) {}
    pub fn add_header_bearer(&mut self, _token: &str) {}
    pub fn add_header_basic(&mut self, _username: &str, _password: &str) {}
    pub fn baseurl(&mut self, base: String) {
        self.baseurl = base.to_string();
    }

    pub fn get(&mut self, path: String) -> RhaiRes<Map> {
        let found: Vec<HttpMockItem> = self.mocks.clone().into_iter().filter(|m| m.method==HttpMethod::Get && m.path==path).collect();
        if found.len() > 0 {
            Ok(found[0].clone().return_obj)
        } else {
            Err(format!("Failed to find GET {path} in the Mock database").into())
        }
    }

    pub fn head(&mut self, path: String) -> RhaiRes<Map> {
        let found: Vec<HttpMockItem> = self.mocks.clone().into_iter().filter(|m| m.method==HttpMethod::Head && m.path==path).collect();
        if found.len() > 0 {
            Ok(found[0].clone().return_obj)
        } else {
            Err(format!("Failed to find HEAD {path} in the Mock database").into())
        }
    }

    pub fn patch(&mut self, path: String, _val: Dynamic) -> RhaiRes<Map> {
        let found: Vec<HttpMockItem> = self.mocks.clone().into_iter().filter(|m| m.method==HttpMethod::Patch && m.path==path).collect();
        if found.len() > 0 {
            Ok(found[0].clone().return_obj)
        } else {
            Err(format!("Failed to find PATCH {path} in the Mock database").into())
        }
    }

    pub fn put(&mut self, path: String, _val: Dynamic) -> RhaiRes<Map> {
        let found: Vec<HttpMockItem> = self.mocks.clone().into_iter().filter(|m| m.method==HttpMethod::Put && m.path==path).collect();
        if found.len() > 0 {
            Ok(found[0].clone().return_obj)
        } else {
            Err(format!("Failed to find PUT {path} in the Mock database").into())
        }
    }

    pub fn post(&mut self, path: String, _val: Dynamic) -> RhaiRes<Map> {
        let found: Vec<HttpMockItem> = self.mocks.clone().into_iter().filter(|m| m.method==HttpMethod::Post && m.path==path).collect();
        if found.len() > 0 {
            Ok(found[0].clone().return_obj)
        } else {
            Err(format!("Failed to find POST {path} in the Mock database").into())
        }
    }

    pub fn delete(&mut self, path: String) -> RhaiRes<Map> {
                let found: Vec<HttpMockItem> = self.mocks.clone().into_iter().filter(|m| m.method==HttpMethod::Delete && m.path==path).collect();
        if found.len() > 0 {
            Ok(found[0].clone().return_obj)
        } else {
            Err(format!("Failed to find DELETE {path} in the Mock database").into())
        }
    }
}


pub fn httpmock_rhai_register(engine: &mut Engine, mocks: Vec<HttpMockItem>) {
    let new = move |base: &str| -> RestClientMock {
        RestClientMock::new(base, mocks.clone())
    };
    engine
        .register_type_with_name::<RestClientMock>("RestClient")
        .register_fn("new_http_client", new)
        .register_fn("headers_reset", RestClientMock::headers_reset)
        .register_fn("set_baseurl", RestClientMock::baseurl)
        .register_fn("set_server_ca", RestClientMock::set_server_ca)
        .register_fn("set_mtls_cert_key", RestClientMock::set_mtls)
        .register_fn("add_header", RestClientMock::add_header)
        .register_fn("add_header_json", RestClientMock::add_header_json)
        .register_fn("add_header_bearer", RestClientMock::add_header_bearer)
        .register_fn("add_header_basic", RestClientMock::add_header_basic)
        .register_fn("head", RestClientMock::head)
        .register_fn("get", RestClientMock::get)
        .register_fn("delete", RestClientMock::delete)
        .register_fn("patch", RestClientMock::patch)
        .register_fn("post", RestClientMock::post)
        .register_fn("put", RestClientMock::put);
}
