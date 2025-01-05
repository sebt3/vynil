// TODO: implement more from: https://storageclass.info/csidrivers/
// https://github.com/storageclass/storageclass.github.io/blob/main/data/csidrivers.yaml
const KNOWN_CLASS = [
    #{
        "driverClass": "cephfs.csi.ceph.com",
        "accessModes": #{"readOnlyMany": true, "readWriteMany": true, "readWriteOnce": true, "readWriteOncePod": false},
        "capabilities": #{"clone": true, "dynamic": true, "expansion": true, "raw": false, "snapshot": true, "topology": false, "tracking": false},
    }, #{
        "driverClass": "rbd.csi.ceph.com",
        "accessModes": #{"readOnlyMany": true, "readWriteMany": true, "readWriteOnce": true, "readWriteOncePod": true},
        "capabilities": #{"clone": true, "dynamic": true, "expansion": true, "raw": true, "snapshot": true, "topology": true, "tracking": false},
    }, #{
        "driverClass": "smb.csi.k8s.io",
        "accessModes": #{"readOnlyMany": true, "readWriteMany": true, "readWriteOnce": true, "readWriteOncePod": false},
        "capabilities": #{"dynamic": true},
    }, #{
        "driverClass": "nfs.csi.k8s.io",
        "accessModes": #{"readOnlyMany": true, "readWriteMany": true, "readWriteOnce": true, "readWriteOncePod": false},
        "capabilities": #{"dynamic": true},
    }, #{
        "driverClass": "k8s-sigs.io/nfs-provisioner",
        "accessModes": #{"readOnlyMany": true, "readWriteMany": true, "readWriteOnce": true, "readWriteOncePod": false},
        "capabilities": #{"dynamic": true},
    }, #{
        "driverClass": "rancher.io/local-path",
        "accessModes": #{"readOnlyMany": false, "readWriteMany": false, "readWriteOnce": true, "readWriteOncePod": true},
        "capabilities": #{"dynamic": true},
    }, #{
        "driverClass": "csi.scaleway.com",
        "accessModes": #{"readOnlyMany": true, "readWriteMany": true, "readWriteOnce": true, "readWriteOncePod": true},
        "capabilities": #{"clone": false, "dynamic": true, "expansion": true, "raw": true, "snapshot": true, "topology": true, "tracking": false},
    }, #{
        "driverClass": "filestore.csi.storage.gke.io",
        "accessModes": #{"readOnlyMany": true, "readWriteMany": true, "readWriteOnce": true, "readWriteOncePod": false},
        "capabilities": #{"dynamic": true, "file": true},
    }, #{
        "driverClass": "file.csi.azure.com",
        "accessModes": #{"readOnlyMany": true, "readWriteMany": true, "readWriteOnce": true, "readWriteOncePod": false},
        "capabilities": #{"clone": false, "dynamic": true, "expansion": true, "file": true, "raw": false, "snapshot": false, "topology": false, "tracking": false},
    }, #{
        "driverClass": "efs.csi.aws.com",
        "accessModes": #{"readOnlyMany": true, "readWriteMany": true, "readWriteOnce": true, "readWriteOncePod": false},
        "capabilities": #{"dynamic": true, "file": true},
    }
];

fn get_default_access_modes_from_all(all) {
    if all.contains("ReadWriteMany") {
        ["ReadWriteMany"]
    } else if all.contains("ReadOnlyMany") && all.contains("ReadWriteOnce") {
        ["ReadOnlyMany", "ReadWriteOnce"]
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
        let f = global::KNOWN_CLASS.find(|k| i.provisioner.contains(k.driverClass));
        if f != () {
            i["capabilities"] = f.capabilities;
            let keys = f.accessModes.keys();
            i["allAccessModes"] = keys.filter(|k| f.accessModes[k]).map(|s| {s.replace("read","Read");s});
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

fn get_prefered_sc(scs) {
    let pref = #{
        "block_readWriteMany": (),
        "block_readWriteOnce": (),
        "fs_cheap_readWriteMany": (),
        "fs_fast_readWriteMany": (),
        "fs_readWriteMany": (),
        "fs_cheap_readWriteOnce": (),
        "fs_fast_readWriteOnce": (),
        "fs_readWriteOnce": (),
    };
    let block = scs.filter(|s| s.volumeMode=="Block");
    let fs_only = scs.filter(|s| s.volumeMode=="Filesystem" && [["ReadWriteOncePod"],["ReadWriteOnce"]].contains(s.accessModes));
    let fs_many = scs.filter(|s| s.volumeMode=="Filesystem" && s.accessModes==["ReadWriteMany"]);
    if block.len() > 0 {
        let many = block.filter(|s| s.accessModes==["ReadWriteMany"]);
        let others = block.filter(|s| s.accessModes!=["ReadWriteMany"]);
        if many.len() > 0 {
            pref["block_readWriteMany"] = many[0].name;
        }
        if others.len() > 0 {
            pref["block_readWriteOnce"] = others[0].name;
        } else {
            pref["block_readWriteOnce"] = many[0].name;
        }
    }
    if fs_many.len() > 0 {
        pref["fs_readWriteMany"] = fs_many[0].name;
    }
    if fs_only.len() > 0 {
        pref["fs_readWriteOnce"] = fs_only[0].name;
    }
    pref
}
