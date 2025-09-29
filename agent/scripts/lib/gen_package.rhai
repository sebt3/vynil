fn gen_yaml(path, data) {
    file_write(path, `---
${yaml_encode(data)}`);
}
fn gen_yaml_if(path, type, data) {
    file_write(path, `{{#if (ctx_have_crd this "${type}")}}
---
${yaml_encode(data)}
{{/if}}
`);
}

fn gen_system(path, docs) {
    let yaml = yaml_decode(file_read(path+"/package.yaml"));
    let name = if type_of(yaml["metadata"]) == "map" {yaml["metadata"]["name"]} else {()};
    if name != () {
        for doc in docs.filter(|obj| type_of(obj) == "map" && obj.keys().contains("kind") && obj.keys().contains("metadata") && type_of(obj.metadata) == "map") {
            if doc.kind == "CustomResourceDefinition" {
                create_dir(`${path}/get_crds`);
                if "conversion" in doc.spec && doc.spec.conversion != () {
                    if "webhook" in doc.spec.conversion && doc.spec.conversion.webhook != () {
                        doc.spec.conversion.webhook.clientConfig.service.name.replace(name, "{{instance.appslug}}");
                        doc.spec.conversion.webhook.clientConfig.service.namespace = "{{instance.namespace}}";
                    }
                    gen_yaml(`${path}/get_crds/${doc.metadata.name}.yaml.hbs`, doc);
                } else {
                    gen_yaml(`${path}/get_crds/${doc.metadata.name}.yaml`, doc);
                }
            } else if doc.kind != () && doc.kind != "Namespace" {
                create_dir(`${path}/get_systems`);
                let doc_name = doc.metadata.name;
                if doc.kind == "ClusterRoleBinding" || doc.kind == "ClusterRole" || doc.kind == "ValidatingWebhookConfiguration" || doc.kind == "MutatingWebhookConfiguration" || doc.kind == "APIService" {
                    doc.metadata.name.replace(name, "{{instance.namespace}}-{{instance.appslug}}");
                } else {
                    doc.metadata.name.replace(name, "{{instance.appslug}}");
                }
                if "namespace" in doc.metadata {
                    doc.metadata.remove("namespace");
                }
                if doc.kind == "ClusterRoleBinding" || doc.kind == "RoleBinding" {
                    let subj = [];
                    for s in doc.subjects {
                        s.name.replace(name, "{{instance.appslug}}");
                        s.namespace = "{{instance.namespace}}";
                        subj.push(s);
                    }
                    doc.subjects = subj;
                    doc.roleRef.name.replace(name, "{{instance.appslug}}");
                    if "namespace" in doc.roleRef && doc.roleRef.namespace != () {
                        doc.roleRef.namespace = "{{instance.namespace}}";
                    }
                }
                if doc.kind == "ValidatingWebhookConfiguration" || doc.kind == "MutatingWebhookConfiguration" {
                    let webhooks = [];
                    for w in doc.webhooks {
                        w.clientConfig.service.name.replace(name, "{{instance.appslug}}");
                        w.clientConfig.service.namespace = "{{instance.namespace}}";
                        webhooks.push(w);
                    }
                    doc.webhooks = webhooks;
                }
                if doc.kind == "APIService" {
                    doc.spec.service.name.replace(name, "{{instance.appslug}}");
                    doc.spec.service.namespace = "{{instance.namespace}}";
                }
                if doc.kind == "ServiceMonitor" {
                    gen_yaml_if(`${path}/get_systems/${doc.kind}_${doc_name}.yaml.hbs`, "servicemonitors.monitoring.coreos.com", doc);
                } else if doc.kind == "PrometheusRule" {
                    gen_yaml_if(`${path}/get_systems/${doc.kind}_${doc_name}.yaml.hbs`, "prometheusrules.monitoring.coreos.com", doc);
                } else {
                    gen_yaml(`${path}/get_systems/${doc.kind}_${doc_name}.yaml.hbs`, doc);
                }
            }
        }
    }
}

fn gen_tenant(path, docs) {
    let yaml = yaml_decode(file_read(path+"/package.yaml"));
    let name = if type_of(yaml["metadata"]) == "map" {yaml["metadata"]["name"]} else {()};
    if name != () {
        for doc in docs.filter(|obj| type_of(obj) == "map" && obj.keys().contains("kind") && obj.keys().contains("metadata") && type_of(obj.metadata) == "map" && obj.kind != ()) {
            let doc_name = doc.metadata.name;
            doc.metadata.name.replace(name, "{{instance.appslug}}");
            if "namespace" in doc.metadata {
                doc.metadata.remove("namespace");
            }
            if ["PersistentVolumeClaim"].contains(doc.kind) {
                create_dir(`${path}/get_vitals`);
                gen_yaml(`${path}/get_vitals/${doc.kind}_${doc_name}.yaml.hbs`, doc);
            } else if ["Deployment", "ReplicaSet", "StatefulSet"].contains(doc.kind) {
                create_dir(`${path}/get_scalables`);
                gen_yaml(`${path}/get_scalables/${doc.kind}_${doc_name}.yaml.hbs`, doc);
            } else if ["Namespace", "CustomResourceDefinition", "ClusterRole", "ClusterRoleBinding", "ValidatingWebhookConfiguration", "MutatingWebhookConfiguration", "APIService"].contains(doc.kind) {
                create_dir(`${path}/get_systems`);
                if doc.kind == "ClusterRoleBinding" {
                    let subj = [];
                    for s in doc.subjects {
                        s.name.replace(name, "{{instance.appslug}}");
                        s.namespace = "{{instance.namespace}}";
                        subj.push(s);
                    }
                    doc.subjects = subj;
                    doc.roleRef.name.replace(name, "{{instance.appslug}}");
                    if "namespace" in doc.roleRef && doc.roleRef.namespace != () {
                        doc.roleRef.namespace = "{{instance.namespace}}";
                    }
                }
                if doc.kind == "ValidatingWebhookConfiguration" || doc.kind == "MutatingWebhookConfiguration" {
                    let webhooks = [];
                    for w in doc.webhooks {
                        w.clientConfig.service.name.replace(name, "{{instance.appslug}}");
                        w.clientConfig.service.namespace = "{{instance.namespace}}";
                        webhooks.push(w);
                    }
                    doc.webhooks = webhooks;
                }
                if doc.kind == "APIService" {
                    doc.spec.service.name.replace(name, "{{instance.appslug}}");
                    doc.spec.service.namespace = "{{instance.namespace}}";
                }
                gen_yaml(`${path}/get_systems/${doc.kind}_${doc_name}.yaml.hbs`, doc);
            } else {
                create_dir(`${path}/get_others`);
                if doc.kind == "RoleBinding" {
                    let subj = [];
                    for s in doc.subjects {
                        s.name.replace(name, "{{instance.appslug}}");
                        s.namespace = "{{instance.namespace}}";
                        subj.push(s);
                    }
                    doc.subjects = subj;
                    doc.roleRef.name.replace(name, "{{instance.appslug}}");
                    if "namespace" in doc.roleRef && doc.roleRef.namespace != () {
                        doc.roleRef.namespace = "{{instance.namespace}}";
                    }
                }
                if doc.kind == "ServiceMonitor" {
                    gen_yaml_if(`${path}/get_others/${doc.kind}_${doc_name}.yaml.hbs`, "servicemonitors.monitoring.coreos.com", doc);
                } else if doc.kind == "PrometheusRule" {
                    gen_yaml_if(`${path}/get_others/${doc.kind}_${doc_name}.yaml.hbs`, "prometheusrules.monitoring.coreos.com", doc);
                } else {
                    gen_yaml(`${path}/get_others/${doc.kind}_${doc_name}.yaml.hbs`, doc);
                }
           }
        }
    }
}

fn gen_service(path, docs) {
    let yaml = yaml_decode(file_read(path+"/package.yaml"));
    let name = if type_of(yaml["metadata"]) == "map" {yaml["metadata"]["name"]} else {()};
    if name != () {
        for doc in docs.filter(|obj| type_of(obj) == "map" && obj.keys().contains("kind") && obj.keys().contains("metadata") && type_of(obj.metadata) == "map" && obj.kind != ()) {
            let doc_name = doc.metadata.name;
            doc.metadata.name.replace(name, "{{instance.appslug}}");
            if "namespace" in doc.metadata {
                doc.metadata.remove("namespace");
            }
            if ["PersistentVolumeClaim"].contains(doc.kind) {
                create_dir(`${path}/get_vitals`);
                gen_yaml(`${path}/get_vitals/${doc.kind}_${doc_name}.yaml.hbs`, doc);
            } else if ["Deployment", "ReplicaSet", "StatefulSet", "DaemonSet"].contains(doc.kind) {
                create_dir(`${path}/get_scalables`);
                gen_yaml(`${path}/get_scalables/${doc.kind}_${doc_name}.yaml.hbs`, doc);
            } else if doc.kind == "CustomResourceDefinition" {
                create_dir(`${path}/get_crds`);
                if "conversion" in doc.spec && doc.spec.conversion != () {
                    if "webhook" in doc.spec.conversion && doc.spec.conversion.webhook != () {
                        doc.spec.conversion.webhook.clientConfig.service.name.replace(name, "{{instance.appslug}}");
                        doc.spec.conversion.webhook.clientConfig.service.namespace = "{{instance.namespace}}";
                    }
                    gen_yaml(`${path}/get_crds/${doc.metadata.name}.yaml.hbs`, doc);
                } else {
                    gen_yaml(`${path}/get_crds/${doc.metadata.name}.yaml`, doc);
                }
            } else if ["Namespace"].contains(doc.kind) {
                create_dir(`${path}/get_systems`);
                gen_yaml(`${path}/get_systems/${doc.kind}_${doc_name}.yaml.hbs`, doc);
            } else {
                create_dir(`${path}/get_others`);
                if doc.kind == "RoleBinding" {
                    let subj = [];
                    for s in doc.subjects {
                        s.name.replace(name, "{{instance.appslug}}");
                        s.namespace = "{{instance.namespace}}";
                        subj.push(s);
                    }
                    doc.subjects = subj;
                    doc.roleRef.name.replace(name, "{{instance.appslug}}");
                    if "namespace" in doc.roleRef && doc.roleRef.namespace != () {
                        doc.roleRef.namespace = "{{instance.namespace}}";
                    }
                }
                if doc.kind == "ClusterRoleBinding" {
                    let subj = [];
                    for s in doc.subjects {
                        s.name.replace(name, "{{instance.appslug}}");
                        s.namespace = "{{instance.namespace}}";
                        subj.push(s);
                    }
                    doc.subjects = subj;
                    doc.roleRef.name.replace(name, "{{instance.appslug}}");
                    if "namespace" in doc.roleRef && doc.roleRef.namespace != () {
                        doc.roleRef.namespace = "{{instance.namespace}}";
                    }
                }
                if doc.kind == "ValidatingWebhookConfiguration" || doc.kind == "MutatingWebhookConfiguration" {
                    let webhooks = [];
                    for w in doc.webhooks {
                        w.clientConfig.service.name.replace(name, "{{instance.appslug}}");
                        w.clientConfig.service.namespace = "{{instance.namespace}}";
                        webhooks.push(w);
                    }
                    doc.webhooks = webhooks;
                }
                if doc.kind == "APIService" {
                    doc.spec.service.name.replace(name, "{{instance.appslug}}");
                    doc.spec.service.namespace = "{{instance.namespace}}";
                }
                if doc.kind == "ServiceMonitor" {
                    gen_yaml_if(`${path}/get_others/${doc.kind}_${doc_name}.yaml.hbs`, "servicemonitors.monitoring.coreos.com", doc);
                } else if doc.kind == "PrometheusRule" {
                    gen_yaml_if(`${path}/get_others/${doc.kind}_${doc_name}.yaml.hbs`, "prometheusrules.monitoring.coreos.com", doc);
                } else {
                    gen_yaml(`${path}/get_others/${doc.kind}_${doc_name}.yaml.hbs`, doc);
                }
           }
        }
    }
}