fn get_auth_from(secret, namespace, registry) {
    let sec = k8s_resource("Secret", namespace).get(secret);
    let auths = json_decode(base64_decode(sec.data[".dockerconfigjson"])).auths;
    let auth = base64_decode(auths[registry].auth).split(":");
    #{user: auth[0], pass: auth[1]}
}
