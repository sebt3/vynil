fn resolv_cm(svc) {
    if svc?.definition?.kind == "ConfigMap" {
        try {
            let cfg = k8s_resource("ConfigMap", svc.definition.namespace).get(svc.definition.name);
            svc += cfg.data;
        } catch {}
        svc.remove("definition");
    }
    svc
}

fn get_from(svcs, names) {
    if svcs == () {
        return ();
    }
    if type_of(names) == "string" {
        return resolv_cm(svcs[names]);
    }
    for n in names {
        if svcs[n] != () {
            return resolv_cm(svcs[n]);
        }
    }
    ()
}

fn get_system(context, names) {
    get_from(context?.cluster?.services, names)
}

fn get_tenant(context, names) {
    get_from(context?.tenant?.services, names)
}

fn resolv_secret(svc) {
    if svc?.definition?.kind == "Secret" {
        try {
            let data = k8s_resource("Secret", svc.definition.namespace).get(svc.definition.name).data;
            for k in data.keys() {
                svc[k] = base64_decode(data[k]);
            }
        } catch {}
        svc.remove("definition");
    }
    svc
}
