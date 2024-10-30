fn gen_yaml(path, data) {
    file_write(path, `---
${yaml_encode(data)}`);
}

fn gen_system(path, docs) {
    let yaml = yaml_decode(file_read(path+"/package.yaml"));
    let name = if type_of(yaml["metadata"]) == "map" {yaml["metadata"]["name"]} else {()};
    if name != () {
        for doc in docs {
            if doc.kind == "CustomResourceDefinition" {
                create_dir(`${path}/get_crds`);
                if "conversion" in doc.spec && doc.spec.conversion != () {
                    if "webhook" in doc.spec.conversion && doc.spec.conversion.webhook != () {
                        doc.spec.conversion.webhook.clientConfig.service.name.replace(name, "{{ instance.appslug }}");
                        doc.spec.conversion.webhook.clientConfig.service.namespace = "{{ instance.namespace }}";
                    }
                    gen_yaml(`${path}/get_crds/${doc.metadata.name}.yaml.hbs`, doc);
                } else {
                    gen_yaml(`${path}/get_crds/${doc.metadata.name}.yaml`, doc);
                }
            } else if doc.kind != () && doc.kind != "Namespace" {
                create_dir(`${path}/get_systems`);
                let doc_name = doc.metadata.name;
                doc.metadata.name.replace(name, "{{ instance.appslug }}");
                if "namespace" in doc.metadata && doc.metadata.namespace != () {
                    doc.metadata.namespace = "{{ instance.namespace }}";
                }
                if doc.kind == "ClusterRoleBinding" || doc.kind == "RoleBinding" {
                    let subj = [];
                    for s in doc.subjects {
                        s.name.replace(name, "{{ instance.appslug }}");
                        s.namespace = "{{ instance.namespace }}";
                        subj.push(s);
                    }
                    doc.subjects = subj;
                    doc.roleRef.name.replace(name, "{{ instance.appslug }}");
                    if "namespace" in doc.roleRef && doc.roleRef.namespace != () {
                        doc.roleRef.namespace = "{{ instance.namespace }}";
                    }
                }
                if doc.kind == "ValidatingWebhookConfiguration" || doc.kind == "MutatingWebhookConfiguration" {
                    let webhooks = [];
                    for w in doc.webhooks {
                        w.clientConfig.service.name.replace(name, "{{ instance.appslug }}");
                        w.clientConfig.service.namespace = "{{ instance.namespace }}";
                        webhooks.push(w);
                    }
                    doc.webhooks = webhooks;
                }
                if doc.kind == "APIService" {
                    doc.spec.service.name.replace(name, "{{ instance.appslug }}");
                    doc.spec.service.namespace = "{{ instance.namespace }}";
                }
                gen_yaml(`${path}/get_systems/${doc.kind}_${doc_name}.yaml.hbs`, doc);
            }
        }
    }
}