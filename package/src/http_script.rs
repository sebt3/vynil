use rhai::{Engine,Map,Dynamic,ImmutableString};
use base64::{engine::general_purpose::STANDARD, Engine as _};
use reqwest::{Client,Response};

use serde::{Deserialize, Serialize};
use tokio::runtime::Handle;

#[derive(Deserialize, Serialize, Clone, Debug)]
pub struct Head {
    pub get: Map
}
impl Head {
    pub fn new() -> Head {
        Head {
            get: Map::new()
        }
    }
    pub fn from(src: Map) -> Head {
        Head {
            get: src
        }
    }
    pub fn bearer(token: &str) -> Head {
        let mut this = Head::new();
        this.add_bearer(token);
        this
    }
    pub fn basic(username: &str, password: &str) -> Head {
        let mut this = Head::new();
        this.add_basic(username, password);
        this
    }
    pub fn add_bearer(&mut self, token: &str) -> &mut Head {
        self.get.insert("Authorization".to_string().into(), format!("Bearer {token}").into());
        self
    }
    pub fn add_basic(&mut self, username: &str, password: &str) -> &mut Head {
        let hash = STANDARD.encode(format!("{username}:{password}"));
        self.get.insert("Authorization".to_string().into(), format!("Basic {hash}").into());
        self
    }
    pub fn add_json_content(&mut self) -> &mut Head {
        self.get.insert("Content-Type".to_string().into(), "application/json; charset=utf-8".to_string().into());
        self
    }
    pub fn add_json_accept(&mut self) -> &mut Head {
        self.get.insert("Accept".to_string().into(), "application/json".to_string().into());
        self
    }
    pub fn add_json(&mut self) -> &mut Head {
        self.add_json_content().add_json_accept()
    }
}

fn http_get(url: &str, headers: Map) -> Response {
    let mut client = Client::new().get(url.to_string());
    for (key,val) in headers {
        client = client.header(key.to_string(), val.to_string());
    }
    tokio::task::block_in_place(|| {Handle::current().block_on(async move {
        client.send().await.unwrap()
    })})
}
fn http_patch(url: &str, headers: Map, body: &str) -> Response {
    let mut client = Client::new().patch(url.to_string()).body(body.to_string());
    for (key,val) in headers {
        client = client.header(key.to_string(), val.to_string());
    }
    tokio::task::block_in_place(|| {Handle::current().block_on(async move {
        client.send().await.unwrap()
    })})
}
fn http_post(url: &str, headers: Map, body: &str) -> Response {
    let mut client = Client::new().post(url.to_string()).body(body.to_string());
    for (key,val) in headers {
        client = client.header(key.to_string(), val.to_string());
    }
    tokio::task::block_in_place(|| {Handle::current().block_on(async move {
        client.send().await.unwrap()
    })})
}
fn http_put(url: &str, headers: Map, body: &str) -> Response {
    let mut client = Client::new().put(url.to_string()).body(body.to_string());
    for (key,val) in headers {
        client = client.header(key.to_string(), val.to_string());
    }
    tokio::task::block_in_place(|| {Handle::current().block_on(async move {
        client.send().await.unwrap()
    })})
}
fn http_check(url: &str, headers: Map, code: i64) -> bool {
    let res = http_get(url, headers);
    i64::from(res.status().as_u16())==code
}
pub fn add_http_to_engine(e: &mut Engine) {
    // TODO: http_get[,_json](uri,headers)
    // TODO: http_[patch|post|put][,_json](uri,headers,payload)
    e.register_fn("http_check", move |url:ImmutableString,headers:Map,code:i64| -> bool {
        http_check(&url.to_string(),headers,i64::from(code))
    });
    e.register_fn("http_get", move |url:ImmutableString,headers:Map| -> Dynamic {
        let res = http_get(&url.to_string(),headers);
        let mut ret = Map::new();
        ret.insert("code".to_string().into(), Dynamic::from(res.status().as_u16()));
        tokio::task::block_in_place(|| {Handle::current().block_on(async move {
            ret.insert("body".to_string().into(), Dynamic::from(res.text().await.unwrap()));
            ret.into()
        })})
    });
    e.register_fn("http_post", move |url:ImmutableString,headers:Map,data:ImmutableString| -> Dynamic {
        let res = http_post(&url.to_string(),headers,&data.to_string());
        let mut ret = Map::new();
        ret.insert("code".to_string().into(), Dynamic::from(res.status().as_u16()));
        tokio::task::block_in_place(|| {Handle::current().block_on(async move {
            ret.insert("body".to_string().into(), Dynamic::from(res.text().await.unwrap()));
            ret.into()
        })})
    });
    e.register_fn("http_patch", move |url:ImmutableString,headers:Map,data:ImmutableString| -> Dynamic {
        let res = http_patch(&url.to_string(),headers,&data.to_string());
        let mut ret = Map::new();
        ret.insert("code".to_string().into(), Dynamic::from(res.status().as_u16()));
        tokio::task::block_in_place(|| {Handle::current().block_on(async move {
            ret.insert("body".to_string().into(), Dynamic::from(res.text().await.unwrap()));
            ret.into()
        })})
    });
    e.register_fn("http_put", move |url:ImmutableString,headers:Map,data:ImmutableString| -> Dynamic {
        let res = http_put(&url.to_string(),headers,&data.to_string());
        let mut ret = Map::new();
        ret.insert("code".to_string().into(), Dynamic::from(res.status().as_u16()));
        tokio::task::block_in_place(|| {Handle::current().block_on(async move {
            ret.insert("body".to_string().into(), Dynamic::from(res.text().await.unwrap()));
            ret.into()
        })})
    });
    e.register_fn("http_get_json", move |url:ImmutableString,headers:Map| -> Dynamic {
        let mut h = Head::from(headers);h.add_json_accept();
        let res = http_get(&url.to_string(),h.get);
        let mut ret = Map::new();
        ret.insert("code".to_string().into(), Dynamic::from(res.status().as_u16()));
        tokio::task::block_in_place(|| {Handle::current().block_on(async move {
            let text = res.text().await.unwrap();
            ret.insert("json".to_string().into(), serde_json::from_str(&text).unwrap());
            ret.insert("body".to_string().into(), Dynamic::from(text));
            ret.into()
        })})
    });
    e.register_fn("http_post_json", move |url:ImmutableString,headers:Map,data:Dynamic| -> Dynamic {
        let mut h = Head::from(headers);h.add_json();
        let res = http_post(&url.to_string(),h.get,&serde_json::to_string(&data).unwrap());
        let mut ret = Map::new();
        ret.insert("code".to_string().into(), Dynamic::from(res.status().as_u16()));
        tokio::task::block_in_place(|| {Handle::current().block_on(async move {
            let text = res.text().await.unwrap();
            ret.insert("json".to_string().into(), serde_json::from_str(&text).unwrap());
            ret.insert("body".to_string().into(), Dynamic::from(text));
            ret.into()
        })})
    });
    e.register_fn("http_patch_json", move |url:ImmutableString,headers:Map,data:Dynamic| -> Dynamic {
        let mut h = Head::from(headers);h.add_json();
        let res = http_patch(&url.to_string(),h.get,&serde_json::to_string(&data).unwrap());
        let mut ret = Map::new();
        ret.insert("code".to_string().into(), Dynamic::from(res.status().as_u16()));
        tokio::task::block_in_place(|| {Handle::current().block_on(async move {
            let text = res.text().await.unwrap();
            ret.insert("json".to_string().into(), serde_json::from_str(&text).unwrap());
            ret.insert("body".to_string().into(), Dynamic::from(text));
            ret.into()
        })})
    });
    e.register_fn("http_put_json", move |url:ImmutableString,headers:Map,data:Dynamic| -> Dynamic {
        let mut h = Head::from(headers);h.add_json();
        let res = http_put(&url.to_string(),h.get,&serde_json::to_string(&data).unwrap());
        let mut ret = Map::new();
        ret.insert("code".to_string().into(), Dynamic::from(res.status().as_u16()));
        tokio::task::block_in_place(|| {Handle::current().block_on(async move {
            let text = res.text().await.unwrap();
            ret.insert("json".to_string().into(), serde_json::from_str(&text).unwrap());
            ret.insert("body".to_string().into(), Dynamic::from(text));
            ret.into()
        })})
    });
    e.register_fn("http_header", || -> Map {
        Head::new().get
    });
    e.register_fn("http_header_basic", |user:ImmutableString,pass:ImmutableString| -> Map {
        Head::basic(&user.to_string(),&pass.to_string()).get
    });
    e.register_fn("http_header_bearer", |token:ImmutableString| -> Map {
        Head::bearer(&token.to_string()).get
    });
    /*e.register_fn("http_header_json", || -> Map {
        let mut r = Head::new();r.add_json();r.get
    });
    e.register_fn("http_header_json_basic", |user:ImmutableString,pass:ImmutableString| -> Map {
        let mut r = Head::basic(&user.to_string(),&pass.to_string());r.add_json();r.get
    });
    e.register_fn("http_header_json_bearer", |token:ImmutableString| -> Map {
        let mut r = Head::bearer(&token.to_string());r.add_json();r.get
    });*/
}