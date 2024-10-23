use clap::Args;
use common::{instancesystem::SystemInstance, instancetenant::TenantInstance, jukebox::JukeBox, Error};
use kube::CustomResourceExt;

#[derive(Args, Debug)]
pub struct Parameters {}

pub async fn run(_args: &Parameters) -> std::result::Result<(), Error> {
    println!("---");
    let mut crd = JukeBox::crd();
    if let Some(ref mut schema) = crd.spec.versions[0].schema {
        if let Some(ref mut api) = schema.open_api_v3_schema {
            if let Some(ref mut props) = api.properties {
                props.entry("status".into()).and_modify(|status| {
                    if let Some(ref mut props) = status.properties {
                        props.entry("packages".into()).and_modify(|pspec| {
                            if let Some(ref mut pprops) = pspec.items {
                                match pprops {
                                    k8s_openapi::apiextensions_apiserver::pkg::apis::apiextensions::v1::JSONSchemaPropsOrArray::Schema(sc) => {
                                        if let Some(ref mut pr) = sc.properties {
                                            pr.entry("options".into()).and_modify(|spec| {
                                                spec.x_kubernetes_preserve_unknown_fields = Some(true);
                                                spec.additional_properties = None;
                                            });
                                        }
                                    },k8s_openapi::apiextensions_apiserver::pkg::apis::apiextensions::v1::JSONSchemaPropsOrArray::Schemas(_x) => {}

                                }
                            }
                        });
                    }
                });
            }
        }
    }
    print!("{}", serde_yaml::to_string(&crd).unwrap());
    println!("---");
    let mut crd = TenantInstance::crd();
    if let Some(ref mut schema) = crd.spec.versions[0].schema {
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
            }
        }
    }
    print!("{}", serde_yaml::to_string(&crd).unwrap());
    println!("---");
    let mut crd = SystemInstance::crd();
    if let Some(ref mut schema) = crd.spec.versions[0].schema {
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
            }
        }
    }
    print!("{}", serde_yaml::to_string(&crd).unwrap());
    Ok(())
}
