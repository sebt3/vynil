use rhai::{Engine, Dynamic, ImmutableString};
use k8s::{Client, handlers::{DistribHandler, IngressHandler, InstallHandler, SecretHandler, CustomResourceDefinitionHandler, ServiceHandler, NamespaceHandler, StorageClassHandler, CSIDriverHandler, NodeHandler, IngressClassHandler, TenantHandler, ClusterIssuerHandler}};
use tokio::runtime::Handle;

fn add_crd_to_engine(e: &mut Engine, client: &Client) {
    let cli = client.clone();
    e.register_fn("have_crd", move |name:ImmutableString| -> bool {
        let cl = cli.clone();
        tokio::task::block_in_place(|| {Handle::current().block_on(async move {
            let mut handle = CustomResourceDefinitionHandler::new(&cl);
            handle.have(&name).await
        })})
    });
    let cli = client.clone();
    e.register_fn("get_crd", move |name:ImmutableString| -> Dynamic {
        let cl = cli.clone();
        tokio::task::block_in_place(|| {Handle::current().block_on(async move {
            let mut handle = CustomResourceDefinitionHandler::new(&cl);
            serde_json::from_str(&serde_json::to_string(&handle.get(&name).await.unwrap()).unwrap()).unwrap()
        })})
    });
    let cli = client.clone();
    e.register_fn("list_crd", move || -> Dynamic {
        let cl = cli.clone();
        tokio::task::block_in_place(|| {Handle::current().block_on(async move {
            let mut handle = CustomResourceDefinitionHandler::new(&cl);
            serde_json::from_str(&serde_json::to_string(&handle.list().await.unwrap()).unwrap()).unwrap()
        })})
    });
}

fn add_distrib_to_engine(e: &mut Engine, client: &Client) {
    let cli = client.clone();
    e.register_fn("have_distrib", move |name:ImmutableString| -> bool {
        let cl = cli.clone();
        tokio::task::block_in_place(|| {Handle::current().block_on(async move {
            let mut handle = DistribHandler::new(&cl);
            handle.have(&name).await
        })})
    });
    let cli = client.clone();
    e.register_fn("get_distrib", move |name:ImmutableString| -> Dynamic {
        let cl = cli.clone();
        tokio::task::block_in_place(|| {Handle::current().block_on(async move {
            let mut handle = DistribHandler::new(&cl);
            serde_json::from_str(&serde_json::to_string(&handle.get(&name).await.unwrap()).unwrap()).unwrap()
        })})
    });
    let cli = client.clone();
    e.register_fn("list_distrib", move || -> Dynamic {
        let cl = cli.clone();
        tokio::task::block_in_place(|| {Handle::current().block_on(async move {
            let mut handle = DistribHandler::new(&cl);
            serde_json::from_str(&serde_json::to_string(&handle.list().await.unwrap()).unwrap()).unwrap()
        })})
    });
}

fn add_install_to_engine(e: &mut Engine, client: &Client) {
    let cli: Client = client.clone();
    e.register_fn("have_install", move |ns:ImmutableString, name:ImmutableString| -> bool {
        let cl = cli.clone();
        tokio::task::block_in_place(|| {Handle::current().block_on(async move {
            let mut handle = InstallHandler::new(&cl, &ns);
            let ret = handle.have(&name).await;
            ret
        })})
    });
    let cli: Client = client.clone();
    e.register_fn("get_install", move |ns:ImmutableString, name:ImmutableString| -> Dynamic {
        let cl = cli.clone();
        tokio::task::block_in_place(|| {Handle::current().block_on(async move {
            let mut handle = InstallHandler::new(&cl, &ns);
            serde_json::from_str(&serde_json::to_string(&handle.get(&name).await.unwrap()).unwrap()).unwrap()
        })})
    });
    let cli: Client = client.clone();
    e.register_fn("list_install", move |ns:ImmutableString| -> Dynamic {
        let cl = cli.clone();
        tokio::task::block_in_place(|| {Handle::current().block_on(async move {
            let mut handle = InstallHandler::new(&cl, &ns);
            serde_json::from_str(&serde_json::to_string(&handle.list().await.unwrap()).unwrap()).unwrap()
        })})
    });
}

fn add_ingress_to_engine(e: &mut Engine, client: &Client) {
    let cli: Client = client.clone();
    e.register_fn("have_ingress", move |ns:ImmutableString, name:ImmutableString| -> bool {
        let cl = cli.clone();
        tokio::task::block_in_place(|| {Handle::current().block_on(async move {
            let mut handle = IngressHandler::new(&cl, &ns);
            handle.have(&name).await
        })})
    });
    let cli: Client = client.clone();
    e.register_fn("get_ingress", move |ns:ImmutableString, name:ImmutableString| -> Dynamic {
        let cl = cli.clone();
        tokio::task::block_in_place(|| {Handle::current().block_on(async move {
            let mut handle = IngressHandler::new(&cl, &ns);
            serde_json::from_str(&serde_json::to_string(&handle.get(&name).await.unwrap()).unwrap()).unwrap()
        })})
    });
    let cli: Client = client.clone();
    e.register_fn("list_ingress", move |ns:ImmutableString| -> Dynamic {
        let cl = cli.clone();
        tokio::task::block_in_place(|| {Handle::current().block_on(async move {
            let mut handle = IngressHandler::new(&cl, &ns);
            serde_json::from_str(&serde_json::to_string(&handle.list().await.unwrap()).unwrap()).unwrap()
        })})
    });
}

fn add_ingressclass_to_engine(e: &mut Engine, client: &Client) {
    let cli: Client = client.clone();
    e.register_fn("have_ingressclass", move |name:ImmutableString| -> bool {
        let cl = cli.clone();
        tokio::task::block_in_place(|| {Handle::current().block_on(async move {
            let mut handle = IngressClassHandler::new(&cl);
            handle.have(&name).await
        })})
    });
    let cli: Client = client.clone();
    e.register_fn("get_ingressclass", move |name:ImmutableString| -> Dynamic {
        let cl = cli.clone();
        tokio::task::block_in_place(|| {Handle::current().block_on(async move {
            let mut handle = IngressClassHandler::new(&cl);
            serde_json::from_str(&serde_json::to_string(&handle.get(&name).await.unwrap()).unwrap()).unwrap()
        })})
    });
    let cli: Client = client.clone();
    e.register_fn("list_ingressclass", move || -> Dynamic {
        let cl = cli.clone();
        tokio::task::block_in_place(|| {Handle::current().block_on(async move {
            let mut handle = IngressClassHandler::new(&cl);
            serde_json::from_str(&serde_json::to_string(&handle.list().await.unwrap()).unwrap()).unwrap()
        })})
    });
}

fn add_secret_to_engine(e: &mut Engine, client: &Client) {
    let cli = client.clone();
    e.register_fn("have_secret", move |ns:ImmutableString, name:ImmutableString| -> bool {
        let cl = cli.clone();
        tokio::task::block_in_place(|| {Handle::current().block_on(async move {
            let mut handle = SecretHandler::new(&cl, &ns);
            handle.have(&name).await
        })})
    });
    let cli = client.clone();
    e.register_fn("get_secret", move |ns:ImmutableString, name:ImmutableString| -> Dynamic {
        let cl = cli.clone();
        tokio::task::block_in_place(|| {Handle::current().block_on(async move {
            let mut handle = SecretHandler::new(&cl, &ns);
            serde_json::from_str(&serde_json::to_string(&handle.get(&name).await.unwrap()).unwrap()).unwrap()
        })})
    });
    let cli = client.clone();
    e.register_fn("list_secret", move |ns:ImmutableString| -> Dynamic {
        let cl = cli.clone();
        tokio::task::block_in_place(|| {Handle::current().block_on(async move {
            let mut handle = SecretHandler::new(&cl, &ns);
            serde_json::from_str(&serde_json::to_string(&handle.list().await.unwrap()).unwrap()).unwrap()
        })})
    });
}

fn add_service_to_engine(e: &mut Engine, client: &Client) {
    let cli = client.clone();
    e.register_fn("have_service", move |ns:ImmutableString, name:ImmutableString| -> bool {
        let cl = cli.clone();
        tokio::task::block_in_place(|| {Handle::current().block_on(async move {
            let mut handle = ServiceHandler::new(&cl, &ns);
            handle.have(&name).await
        })})
    });
    let cli = client.clone();
    e.register_fn("get_service", move |ns:ImmutableString, name:ImmutableString| -> Dynamic {
        let cl = cli.clone();
        tokio::task::block_in_place(|| {Handle::current().block_on(async move {
            let mut handle = ServiceHandler::new(&cl, &ns);
            serde_json::from_str(&serde_json::to_string(&handle.get(&name).await.unwrap()).unwrap()).unwrap()
        })})
    });
    let cli = client.clone();
    e.register_fn("list_service", move |ns:ImmutableString| -> Dynamic {
        let cl = cli.clone();
        tokio::task::block_in_place(|| {Handle::current().block_on(async move {
            let mut handle = ServiceHandler::new(&cl, &ns);
            serde_json::from_str(&serde_json::to_string(&handle.list().await.unwrap()).unwrap()).unwrap()
        })})
    });
}

fn add_ns_to_engine(e: &mut Engine, client: &Client) {
    let cli = client.clone();
    e.register_fn("have_namespace", move |name:ImmutableString| -> bool {
        let cl = cli.clone();
        tokio::task::block_in_place(|| {Handle::current().block_on(async move {
            let mut handle = NamespaceHandler::new(&cl);
            handle.have(&name).await
        })})
    });
    let cli = client.clone();
    e.register_fn("get_namespace", move |name:ImmutableString| -> Dynamic {
        let cl = cli.clone();
        tokio::task::block_in_place(|| {Handle::current().block_on(async move {
            let mut handle = NamespaceHandler::new(&cl);
            serde_json::from_str(&serde_json::to_string(&handle.get(&name).await.unwrap()).unwrap()).unwrap()
        })})
    });
    let cli = client.clone();
    e.register_fn("list_namespace", move || -> Dynamic {
        let cl = cli.clone();
        tokio::task::block_in_place(|| {Handle::current().block_on(async move {
            let mut handle = NamespaceHandler::new(&cl);
            serde_json::from_str(&serde_json::to_string(&handle.list().await.unwrap()).unwrap()).unwrap()
        })})
    });
}

fn add_node_to_engine(e: &mut Engine, client: &Client) {
    let cli = client.clone();
    e.register_fn("have_node", move |name:ImmutableString| -> bool {
        let cl = cli.clone();
        tokio::task::block_in_place(|| {Handle::current().block_on(async move {
            let mut handle = NodeHandler::new(&cl);
            handle.have(&name).await
        })})
    });
    let cli = client.clone();
    e.register_fn("get_node", move |name:ImmutableString| -> Dynamic {
        let cl = cli.clone();
        tokio::task::block_in_place(|| {Handle::current().block_on(async move {
            let mut handle = NodeHandler::new(&cl);
            serde_json::from_str(&serde_json::to_string(&handle.get(&name).await.unwrap()).unwrap()).unwrap()
        })})
    });
    let cli = client.clone();
    e.register_fn("list_node", move || -> Dynamic {
        let cl = cli.clone();
        tokio::task::block_in_place(|| {Handle::current().block_on(async move {
            let mut handle = NodeHandler::new(&cl);
            serde_json::from_str(&serde_json::to_string(&handle.list().await.unwrap()).unwrap()).unwrap()
        })})
    });
}

fn add_tenant_to_engine(e: &mut Engine, client: &Client) {
    let cli = client.clone();
    e.register_fn("have_tenant", move |name:ImmutableString| -> bool {
        let cl = cli.clone();
        tokio::task::block_in_place(|| {Handle::current().block_on(async move {
            let mut handle = TenantHandler::new(&cl);
            handle.have(&name).await
        })})
    });
    let cli = client.clone();
    e.register_fn("get_tenant", move |name:ImmutableString| -> Dynamic {
        let cl = cli.clone();
        tokio::task::block_in_place(|| {Handle::current().block_on(async move {
            let mut handle = TenantHandler::new(&cl);
            serde_json::from_str(&serde_json::to_string(&handle.get(&name).await.unwrap()).unwrap()).unwrap()
        })})
    });
    let cli = client.clone();
    e.register_fn("list_tenant", move || -> Dynamic {
        let cl = cli.clone();
        tokio::task::block_in_place(|| {Handle::current().block_on(async move {
            let mut handle = TenantHandler::new(&cl);
            serde_json::from_str(&serde_json::to_string(&handle.list().await.unwrap()).unwrap()).unwrap()
        })})
    });
}

fn add_clusterissuer_to_engine(e: &mut Engine, client: &Client) {
    let cli = client.clone();
    e.register_fn("have_clusterissuer", move |name:ImmutableString| -> bool {
        let cl = cli.clone();
        tokio::task::block_in_place(|| {Handle::current().block_on(async move {
            let mut handle = ClusterIssuerHandler::new(&cl);
            handle.have(&name).await
        })})
    });
    let cli = client.clone();
    e.register_fn("get_clusterissuer", move |name:ImmutableString| -> Dynamic {
        let cl = cli.clone();
        tokio::task::block_in_place(|| {Handle::current().block_on(async move {
            let mut handle = ClusterIssuerHandler::new(&cl);
            serde_json::from_str(&serde_json::to_string(&handle.get(&name).await.unwrap()).unwrap()).unwrap()
        })})
    });
    let cli = client.clone();
    e.register_fn("list_clusterissuer", move || -> Dynamic {
        let cl = cli.clone();
        tokio::task::block_in_place(|| {Handle::current().block_on(async move {
            let mut handle = ClusterIssuerHandler::new(&cl);
            serde_json::from_str(&serde_json::to_string(&handle.list().await.unwrap()).unwrap()).unwrap()
        })})
    });
}

fn add_sc_to_engine(e: &mut Engine, client: &Client) {
    let cli = client.clone();
    e.register_fn("have_storage_class", move |name:ImmutableString| -> bool {
        let cl = cli.clone();
        tokio::task::block_in_place(|| {Handle::current().block_on(async move {
            let mut handle = StorageClassHandler::new(&cl);
            handle.have(&name).await
        })})
    });
    let cli = client.clone();
    e.register_fn("get_storage_class", move |name:ImmutableString| -> Dynamic {
        let cl = cli.clone();
        tokio::task::block_in_place(|| {Handle::current().block_on(async move {
            let mut handle = StorageClassHandler::new(&cl);
            serde_json::from_str(&serde_json::to_string(&handle.get(&name).await.unwrap()).unwrap()).unwrap()
        })})
    });
    let cli = client.clone();
    e.register_fn("list_storage_class", move || -> Dynamic {
        let cl = cli.clone();
        tokio::task::block_in_place(|| {Handle::current().block_on(async move {
            let mut handle = StorageClassHandler::new(&cl);
            serde_json::from_str(&serde_json::to_string(&handle.list().await.unwrap()).unwrap()).unwrap()
        })})
    });
}

fn add_csi_to_engine(e: &mut Engine, client: &Client) {
    let cli = client.clone();
    e.register_fn("have_csi_driver", move |name:ImmutableString| -> bool {
        let cl = cli.clone();
        tokio::task::block_in_place(|| {Handle::current().block_on(async move {
            let mut handle = CSIDriverHandler::new(&cl);
            handle.have(&name).await
        })})
    });
    let cli = client.clone();
    e.register_fn("get_csi_driver", move |name:ImmutableString| -> Dynamic {
        let cl = cli.clone();
        tokio::task::block_in_place(|| {Handle::current().block_on(async move {
            let mut handle = CSIDriverHandler::new(&cl);
            serde_json::from_str(&serde_json::to_string(&handle.get(&name).await.unwrap()).unwrap()).unwrap()
        })})
    });
    let cli = client.clone();
    e.register_fn("list_csi_driver", move || -> Dynamic {
        let cl = cli.clone();
        tokio::task::block_in_place(|| {Handle::current().block_on(async move {
            let mut handle = CSIDriverHandler::new(&cl);
            serde_json::from_str(&serde_json::to_string(&handle.list().await.unwrap()).unwrap()).unwrap()
        })})
    });
}

pub fn add_k8s_to_engine(e: &mut Engine, client: &Client) {
    add_crd_to_engine(e,client);
    add_distrib_to_engine(e,client);
    add_install_to_engine(e,client);
    add_ingress_to_engine(e,client);
    add_ingressclass_to_engine(e,client);
    add_secret_to_engine(e,client);
    add_service_to_engine(e,client);
    add_ns_to_engine(e,client);
    add_node_to_engine(e,client);
    add_clusterissuer_to_engine(e,client);
    add_tenant_to_engine(e,client);
    add_sc_to_engine(e,client);
    add_csi_to_engine(e,client);
}