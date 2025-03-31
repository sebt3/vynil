fn validate(yaml) {
    if ! ("apiVersion" in yaml) {
        throw "You should set your apiVersion";
    }
    if ! ("kind" in yaml) {
        throw "You should set your kind";
    }
    if ! ("metadata" in yaml) {
        throw "No metadata defined in package.yaml";
    }
    for key in ["name","category", "type"] {
        if ! (key in yaml.metadata) {
            throw `No metadata.${key} defined in package.yaml`;
        } else if type_of(yaml.metadata[key]) != "string" {
            throw `metadata.${key} should be a string`;
        }
    }
    if "options" in yaml && type_of(yaml.options) != "map" {
        throw "options should be a map.";
    }
    if ! ("requirements" in yaml) {
        yaml.requirements = []
    } else if type_of(yaml.requirements) != "array" {
        throw "requirements should be an array.";
    }
    yaml
}

fn run(args) {
    let yaml = yaml_decode(file_read(args.source+"/package.yaml"));
    validate(yaml);
    let valid = read_package_yaml(args.source+"/package.yaml");
    valid.validate_options();
}
