fn validate(yaml) {
    if ! ("apiVersion" in yaml) {
        log_warn("You should set your apiVersion");
        yaml.apiVersion = "vinyl.solidite.fr/v1beta1";
    }
    if ! ("kind" in yaml) {
        log_warn("You should set your kind");
        yaml.kind = "Package";
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
    } else if "options" in yaml {
        yaml.options = properties_improve(yaml.options);

    }
    if ! ("requirements" in yaml) {
        yaml.requirements = []
    } else if type_of(yaml.requirements) != "array" {
        throw "requirements should be an array.";
    } // TODO: validate requirement structure
    yaml
}
fn get_json_type(val) {
    switch type_of(val) {
        "string" => "string",
        "map" => "object",
        "array" => "array",
        "bool" => "boolean",
        "i64" => "integer",
        "f64" => "number"
    }
}
fn properties_improve(values) {
    let valid_type = ["string", "object", "array", "boolean", "integer", "number"];
    // TODO: Faire en sorte que les valeurs par défault déscendent des feuilles en plus de remonter du tron.
    // TODO: Support options auto_improvements
    if type_of(values) == "map" {
        for prop in values.keys() {
            if "default" in values[prop] && (! ("type" in values[prop]) || ! (values[prop].type in valid_type)) {
                values[prop].type = get_json_type(values[prop]["default"]);
            }
            if ! ("type" in values[prop]) {
                throw `no type and no default for property ${prop}`;
            } else if ! (values[prop].type in valid_type) {
                throw `Invalid type "${values[prop].type}" for property ${prop}`;
            }
            if values[prop].type == "object" {
                if ! ("properties" in values[prop]) {
                    values[prop].properties = #{};
                    if "default" in values[prop] {
                        for sub in values[prop]["default"].keys() {
                            values[prop].properties[sub] = #{
                                "default": values[prop]["default"][sub],
                            };
                        }
                    } else {
                        log_warn(`Properties definition left empty for property ${prop}`);
                    }
                }
                values[prop].properties = properties_improve(values[prop].properties);
            } else if values[prop].type == "array" {
                if ! ("items" in values[prop]) {
                    values[prop].items = #{};
                    if "default" in values[prop] && values[prop]["default"].len()>0 {
                        values[prop].items.type = get_json_type(values[prop]["default"][0]);
                        if values[prop].items.type == "object" {
                            values[prop].items.properties = #{};
                            for sub in values[prop]["default"][0].keys() {
                                values[prop].items.properties[sub] = #{
                                    "default": values[prop]["default"][0][sub],
                                };
                            }
                            values[prop].items.properties = properties_improve(values[prop].items.properties);
                        }
                    } else {
                        throw `property ${prop} is an array with no items definition and not enough values in default to guess.`;
                    }
                }
            }
        }
        values
    } else {{}}
}
