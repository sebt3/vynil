fn template(inst, ctx, args) {
    let pod = k8s_resource("Pod", "default", "my-pod");
    return "done";
}
