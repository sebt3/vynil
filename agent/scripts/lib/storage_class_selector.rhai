fn for_deployments(context, typed) {
    let scs = context.cluster.storage_classes;
    if typed=="fast" && context.cluster.prefered_storage.fs_fast_readWriteMany != () {
        scs.find(|s| s.name == context.cluster.prefered_storage.fs_fast_readWriteMany && s.volumeMode == "Filesystem")
    } else if typed=="cheap" && context.cluster.prefered_storage.fs_cheap_readWriteMany != () {
        scs.find(|s| s.name == context.cluster.prefered_storage.fs_cheap_readWriteMany && s.volumeMode == "Filesystem")
    } else if context.cluster.prefered_storage.fs_readWriteMany != () {
        scs.find(|s| s.name == context.cluster.prefered_storage.fs_readWriteMany && s.volumeMode == "Filesystem")
    } else if typed=="fast" && context.cluster.prefered_storage.fs_fast_readWriteOnce != () {
        scs.find(|s| s.name == context.cluster.prefered_storage.fs_fast_readWriteOnce && s.volumeMode == "Filesystem")
    } else if typed=="cheap" && context.cluster.prefered_storage.fs_cheap_readWriteOnce != () {
        scs.find(|s| s.name == context.cluster.prefered_storage.fs_cheap_readWriteOnce && s.volumeMode == "Filesystem")
    } else if context.cluster.prefered_storage.fs_readWriteOnce != () {
        scs.find(|s| s.name == context.cluster.prefered_storage.fs_readWriteOnce && s.volumeMode == "Filesystem")
    } else {
        scs.find(|s| s.is_default)
    }
}
fn for_deployments(context) {
    for_deployments(context, ());
}

fn for_statefulsets(context, typed) {
    let scs = context.cluster.storage_classes;
    if typed=="fast" && context.cluster.prefered_storage.fs_fast_readWriteOnce != () {
        scs.find(|s| s.name == context.cluster.prefered_storage.fs_fast_readWriteOnce && s.volumeMode == "Filesystem")
    } else if typed=="cheap" && context.cluster.prefered_storage.fs_cheap_readWriteOnce != () {
        scs.find(|s| s.name == context.cluster.prefered_storage.fs_cheap_readWriteOnce && s.volumeMode == "Filesystem")
    } else if context.cluster.prefered_storage.fs_readWriteOnce != () {
        scs.find(|s| s.name == context.cluster.prefered_storage.fs_readWriteOnce && s.volumeMode == "Filesystem")
    } else {
        scs.find(|s| s.is_default)
    }
}
fn for_statefulsets(context) {
    for_statefulsets(context, ());
}

fn for_vm(context) {
    let scs = context.cluster.storage_classes;
    if context.cluster.prefered_storage.block_readWriteMany != () {
        scs.find(|s| s.name == context.cluster.prefered_storage.block_readWriteMany && s.volumeMode == "Block")
    } else if context.cluster.prefered_storage.block_readWriteOnce != () {
        scs.find(|s| s.name == context.cluster.prefered_storage.block_readWriteOnce && s.volumeMode == "Block")
    } else {
        scs.find(|s| s.is_default)
    }
}
