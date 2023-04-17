use kube::CustomResourceExt;
//use k8s_openapi::apiextensions_apiserver::pkg::apis::apiextensions::v1::JSONSchemaPropsOrBool;
fn main() {
    println!("---");
    print!("{}", serde_yaml::to_string(&controller::Distrib::crd()).unwrap());
    println!("---");
    let mut crd = controller::Install::crd();
    if let Some( ref mut schema) = crd.spec.versions[0].schema {
        if let Some(ref mut api) = schema.open_api_v3_schema {
            if let Some(ref mut props) = api.properties {
                props.entry("spec".into()).and_modify(|spec| {
                    if let Some(ref mut props) = spec.properties {
                        props.entry("options".into()).and_modify(|spec| {
                            spec.x_kubernetes_preserve_unknown_fields = Some(true);
                            spec.additional_properties = None;
                            //print!("{:?}", spec.additional_properties);
                        });
                    }
                });
                props.entry("status".into()).and_modify(|status| {
                    if let Some(ref mut props) = status.properties {
                        props.entry("tfstate".into()).and_modify(|spec| {
                            spec.x_kubernetes_preserve_unknown_fields = Some(true);
                            spec.additional_properties = None;
                        });
                        props.entry("plan".into()).and_modify(|spec| {
                            spec.x_kubernetes_preserve_unknown_fields = Some(true);
                            spec.additional_properties = None;
                        });
                        props.entry("errors".into()).and_modify(|spec| {
                            spec.x_kubernetes_preserve_unknown_fields = Some(true);
                            spec.additional_properties = None;
                            //spec.additional_properties = Some(JSONSchemaPropsOrBool::Bool(true));
                        });
                    }
                });
            }
        }
    }
    print!("{}", serde_yaml::to_string(&crd).unwrap());
}
