import "gen_package" as gen;

fn run(args) {
    let hub = new_http_client("https://artifacthub.io/api/v1");
    let pck = json_decode(hub.get(`packages/helm/traefik/traefik`).body);
    let yaml = yaml_decode(file_read(args.source+"/package.yaml"));
    let chart_version = if type_of(yaml["metadata"]) == "map" {yaml["metadata"]["app_version"]} else {"0.0.0"};
    let more = pck.available_versions.map(|v| v.version).filter(|v| parse_int(v.split(".")[0])>=parse_int(chart_version.split(".")[0]));
    if more.len > 0 {
        print(`Using Chart version ${chart_version}, availables:`);
        print(yaml_encode(more));
    }
    let name = gen::placeholder();   // remplacé par {{instance.appslug}} dans les templates
    let ns   = gen::placeholder();   // remplacé par {{instance.namespace}} dans les templates
    let cmd  = `helm template ${name}`;
    cmd += " oci://ghcr.io/traefik/helm/traefik";
    cmd += " --include-crds";
    cmd += ` --version ${chart_version}`;
    cmd += ` --namespace=${ns}`;
    cmd += ` -a "monitoring.coreos.com/v1/Probe"`;
    cmd += ` -a "monitoring.coreos.com/v1/PodMonitor"`;
    cmd += ` -a "monitoring.coreos.com/v1/ServiceMonitor"`;
    cmd += ` -a "monitoring.coreos.com/v1/PrometheusRule"`;
    cmd += ` --values ${args.source}/values.yml 2>&1`;
    let out = shell_output(cmd);
    gen::gen_system(args.source, yaml_decode_multi(out), name, ns);
}
