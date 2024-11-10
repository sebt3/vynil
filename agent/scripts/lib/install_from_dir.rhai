fn install(instance, context, dir, allow_cluster, force_ns) {
    let hbs = new_hbs();
    hbs.register_partial_dir(`${context.package_dir}/handlebars/partials`);
    hbs.register_helper_dir(`${context.package_dir}/handlebars/helpers`);
    let applied_objs = [];
    for file in read_dir(dir) {
        log_info(`Applying ${file}`);
        let objects = if file.ends_with(".yaml") || file.ends_with(".yml") {
            yaml_decode_multi(file_read(file))
        } else if file.ends_with(".yaml.hbs") || file.ends_with(".yml.hbs") {
            yaml_decode_multi(hbs.render_from(file_read(file), context))
        } else {[]};
        for obj in objects {
            if obj.kind != () && obj.metadata != () {
                let ns = if (force_ns) {
                    instance.metadata.namespace
                } else {
                    if obj.metadata.namespace != () {obj.metadata.namespace} else {instance.metadata.namespace}
                };
                let api = k8s_resource(obj.kind, ns);
                if api.exist() {
                    if force_ns && api.scope != "cluster" {
                        obj.metadata.namespace = ns;
                    }
                    if api.scope == "cluster" {
                        if allow_cluster {
                            api.apply(obj.metadata.name, obj);
                            applied_objs.push(#{
                                kind: obj.kind,
                                name: obj.metadata.name
                            });
                        } else {
                            log_warn(`Ignoring ${obj.kind} ${obj.metadata.name} as it is a clusterwide resource type.`);
                        }
                    } else {
                        api.apply(obj.metadata.name, obj);
                        applied_objs.push(#{
                            kind: obj.kind,
                            name: obj.metadata.name,
                            namespace: ns
                        });
                    }
                } else {
                    log_debug(yaml_encode(obj));
                    throw `kind: ${obj.kind} is unknown from the apiServer.`;
                }
            } else {
                log_warn(`No kind for an object in file ${file}`);
                log_debug(yaml_encode(obj));
            }
        }
    }
    applied_objs
}

fn install_allowed(instance, context, dir, allowed) {
    let hbs = new_hbs();
    hbs.register_partial_dir(`${context.package_dir}/handlebars/partials`);
    hbs.register_helper_dir(`${context.package_dir}/handlebars/helpers`);
    let applied_objs = [];
    for file in read_dir(dir) {
        log_info(`Applying ${file}`);
        let objects = if file.ends_with(".yaml") || file.ends_with(".yml") {
            yaml_decode_multi(file_read(file))
        } else if file.ends_with(".yaml.hbs") || file.ends_with(".yml.hbs") {
            yaml_decode_multi(hbs.render_from(file_read(file), context))
        } else {[]};
        for obj in objects {
            if obj.kind != () && obj.metadata != () {
                let ns = if (obj.metadata.namespace != () && obj.metadata.namespace in allowed) {
                    obj.metadata.namespace
                } else {
                    instance.metadata.namespace
                };
                let api = k8s_resource(obj.kind, ns);
                if api.exist() {
                    if api.scope != "cluster" {
                        obj.metadata.namespace = ns;
                        api.apply(obj.metadata.name, obj);
                        applied_objs.push(#{
                            kind: obj.kind,
                            name: obj.metadata.name,
                            namespace: ns
                        });
                    } else {
                        log_warn(`Ignoring ${obj.kind} ${obj.metadata.name} as it is a clusterwide resource type.`);
                    }
                } else {
                    log_debug(yaml_encode(obj));
                    throw `kind: ${obj.kind} is unknown from the apiServer.`;
                }
            } else {
                log_warn(`No kind for an object in file ${file}`);
                log_debug(yaml_encode(obj));
            }
        }
    }
    applied_objs
}

fn remove(applied, previous, default_ns) {
    let deleted = [];
    for old in previous {
        if ! applied.some(|n| n.kind == old.kind && n.namespace == old.namespace && n.name == old.name) {
            let ns = if old.namespace != () {old.namespace} else {default_ns};
            let api = k8s_resource(old.kind, ns);
            if api.exist() {
                try {
                    let o = api.get_obj(old.name);
                    o.delete();
                    deleted.push(o);
                } catch(e) {}
            }
        }
    }
    for d in deleted {
        log_info(`Waiting for ${d.metadata.name} to be deleted`);
        d.wait_deleted(60*5);
    }
}