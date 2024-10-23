fn dir_exts(src, dst, exts, recursiv) {
    create_dir(dst);
    for item in read_dir(src) {
        let base = basename(item);
        if is_file(item) {
            if exts.some(|ext| item.ends_with(ext)) {
                file_copy(item, dst+"/"+base);
            }
        } else if recursiv && is_dir(item) {
            copy_dir_exts(src+"/"+base, dst+"/"+base, exts, recursiv);
        }
    }
}
fn dir_exts(src, dst, exts) { dir_exts(src, dst, exts, false); }
fn dir_all(src, dst, recursiv) {
    create_dir(dst);
    for item in read_dir(src) {
        let base = basename(item);
        if is_file(item) {
            file_copy(item, dst+"/"+base);
        } else if recursiv && is_dir(item) {
            copy_dir_all(src+"/"+base, dst+"/"+base, recursiv);
        }
    }
}
fn dir_all(src, dst) { dir_all(src, dst, false); }
