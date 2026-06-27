// TODO: implement more from: https://storageclass.info/csidrivers/
// https://github.com/storageclass/storageclass.github.io/blob/main/data/csidrivers.yaml
fn get_known_class() { [
    #{
        "driverClass": "cephfs.csi.ceph.com",
        "accessModes": #{"ReadOnlyMany": true, "ReadWriteMany": true, "ReadWriteOnce": true, "ReadWriteOncePod": false},
        "capabilities": #{"clone": true, "dynamic": true, "expansion": true, "raw": false, "snapshot": true, "topology": false, "tracking": false},
    }, #{
        "driverClass": "rbd.csi.ceph.com",
        "accessModes": #{"ReadOnlyMany": true, "ReadWriteMany": true, "ReadWriteOnce": true, "ReadWriteOncePod": true},
        "capabilities": #{"clone": true, "dynamic": true, "expansion": true, "raw": true, "snapshot": true, "topology": true, "tracking": false},
    }, #{
        "driverClass": "smb.csi.k8s.io",
        "accessModes": #{"ReadOnlyMany": true, "ReadWriteMany": true, "ReadWriteOnce": true, "ReadWriteOncePod": false},
        "capabilities": #{"dynamic": true},
    }, #{
        "driverClass": "nfs.csi.k8s.io",
        "accessModes": #{"ReadOnlyMany": true, "ReadWriteMany": true, "ReadWriteOnce": true, "ReadWriteOncePod": false},
        "capabilities": #{"dynamic": true},
    }, #{
        "driverClass": "k8s-sigs.io/nfs-provisioner",
        "accessModes": #{"ReadOnlyMany": true, "ReadWriteMany": true, "ReadWriteOnce": true, "ReadWriteOncePod": false},
        "capabilities": #{"dynamic": true},
    }, #{
        "driverClass": "rancher.io/local-path",
        "accessModes": #{"ReadOnlyMany": false, "ReadWriteMany": false, "ReadWriteOnce": true, "ReadWriteOncePod": true},
        "capabilities": #{"dynamic": true},
    }, #{
        "driverClass": "csi.scaleway.com",
        "accessModes": #{"ReadOnlyMany": true, "ReadWriteMany": true, "ReadWriteOnce": true, "ReadWriteOncePod": true},
        "capabilities": #{"clone": false, "dynamic": true, "expansion": true, "raw": true, "snapshot": true, "topology": true, "tracking": false},
    }, #{
        "driverClass": "filestore.csi.storage.gke.io",
        "accessModes": #{"ReadOnlyMany": true, "ReadWriteMany": true, "ReadWriteOnce": true, "ReadWriteOncePod": false},
        "capabilities": #{"dynamic": true, "file": true},
    }, #{
        "driverClass": "file.csi.azure.com",
        "accessModes": #{"ReadOnlyMany": true, "ReadWriteMany": true, "ReadWriteOnce": true, "ReadWriteOncePod": false},
        "capabilities": #{"clone": false, "dynamic": true, "expansion": true, "file": true, "raw": false, "snapshot": false, "topology": false, "tracking": false},
    }, #{
        "driverClass": "efs.csi.aws.com",
        "accessModes": #{"ReadOnlyMany": true, "ReadWriteMany": true, "ReadWriteOnce": true, "ReadWriteOncePod": false},
        "capabilities": #{"dynamic": true, "file": true},
    }, #{
        "driverClass": "ebs.csi.aws.com",
        "accessModes": #{"ReadOnlyMany": false, "ReadWriteMany": false, "ReadWriteOnce": true, "ReadWriteOncePod": true},
        "capabilities": #{"clone": false, "dynamic": true, "expansion": true, "raw": true, "snapshot": true, "topology": true, "tracking": false},
    }, #{
        "driverClass": "ebs.csi.eks.amazonaws.com",
        "accessModes": #{"ReadOnlyMany": false, "ReadWriteMany": false, "ReadWriteOnce": true, "ReadWriteOncePod": true},
        "capabilities": #{"clone": false, "dynamic": true, "expansion": true, "raw": true, "snapshot": true, "topology": true, "tracking": false},
    }
] }

fn get_default_access_modes_from_all(all) {
    if all.contains("ReadWriteMany") {
        ["ReadWriteMany"]
    } else if all.contains("ReadWriteOnce") {
        ["ReadWriteOnce"]
    } else {
        all
    }
}

fn classes_enrich(scs) {
    let enriched = [];
    for i in scs {
        i["volumeMode"] = "Filesystem";
        let f = get_known_class().find(|k| i.provisioner.contains(k.driverClass));
        if f != () {
            i["capabilities"] = f.capabilities;
            let keys = f.accessModes.keys();
            i["allAccessModes"] = keys.filter(|k| f.accessModes[k]);
            if (f.capabilities.raw == true) {
                let b = i;
                b["volumeMode"] = "Block";
                i["allAccessModes"] = keys.filter(|k| f.accessModes[k] && k != "ReadWriteMany");
                b["accessModes"] = get_default_access_modes_from_all(b["allAccessModes"]);
                enriched.push(b);
            }
            i["accessModes"] = get_default_access_modes_from_all(i["allAccessModes"]);
        } else {
            i["capabilities"] = #{};
            i["allAccessModes"] = ["ReadWriteOnce"];
            i["accessModes"] = ["ReadWriteOnce"];
        }
        enriched.push(i);
    }
    enriched
}

fn pick_sc(scs) {
    let def = scs.find(|s| s.is_default);
    if def != () { def.name } else { scs[0].name }
}

fn get_prefered_sc(scs) {
    let pref = #{
        "block_readWriteMany": (),
        "block_readWriteOnce": (),
        "fs_cheap_readWriteMany": (),
        "fs_fast_readWriteMany": (),
        "fs_readWriteMany": (),
        "fs_cheap_readWriteOnce": (),
        "fs_fast_readWriteOnce": (),
        "distibuted_readWriteOnce": (),
        "fs_readWriteOnce": (),
    };
    let block = scs.filter(|s| s.volumeMode=="Block");
    let fs_only = scs.filter(|s| s.volumeMode=="Filesystem" && [["ReadWriteOncePod"],["ReadWriteOnce"]].contains(s.accessModes));
    let fs_many = scs.filter(|s| s.volumeMode=="Filesystem" && s.accessModes==["ReadWriteMany"]);
    if block.len() > 0 {
        let many = block.filter(|s| s.accessModes==["ReadWriteMany"]);
        let others = block.filter(|s| s.accessModes!=["ReadWriteMany"]);
        if many.len() > 0 {
            pref["block_readWriteMany"] = pick_sc(many);
        }
        if others.len() > 0 {
            pref["block_readWriteOnce"] = pick_sc(others);
        } else {
            pref["block_readWriteOnce"] = pick_sc(many);
        }
    }
    if fs_many.len() > 0 {
        pref["fs_readWriteMany"] = pick_sc(fs_many);
    }
    if fs_only.len() > 1 {
        let onlys = fs_only.filter(|s| !s.accessModes.contains(["ReadWriteMany"]));
        if onlys.len() > 1 {
            let local = onlys.filter(|s| s.provisioner=="rancher.io/local-path");
            let other = onlys.filter(|s| s.provisioner!="rancher.io/local-path");
            if local.len() > 0 && other.len() > 0 {
                pref["distibuted_readWriteOnce"] = pick_sc(other);
                pref["fs_readWriteOnce"] = pick_sc(local);
            } else if local.len() > 0 {
                pref["distibuted_readWriteOnce"] = pick_sc(local);
                pref["fs_readWriteOnce"] = pick_sc(local);
            } else if other.len() > 0 {
                pref["distibuted_readWriteOnce"] = pick_sc(other);
                pref["fs_readWriteOnce"] = pick_sc(onlys);
            } else {
                pref["distibuted_readWriteOnce"] = pick_sc(onlys);
                pref["fs_readWriteOnce"] = pick_sc(onlys);
            }
        } else if onlys.len() > 0 {
            pref["distibuted_readWriteOnce"] = pick_sc(onlys);
            pref["fs_readWriteOnce"] = pick_sc(fs_only);
        } else {
            pref["distibuted_readWriteOnce"] = pick_sc(fs_only);
            pref["fs_readWriteOnce"] = pick_sc(fs_only);
        }
    } else if fs_only.len() > 0 {
        pref["distibuted_readWriteOnce"] = pick_sc(fs_only);
        pref["fs_readWriteOnce"] = pick_sc(fs_only);
    }
    pref
}
