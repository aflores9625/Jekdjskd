//! Copy bundled browser extensions next to the built executable so
//! `extension::resolve()` finds them when the app is launched from
//! `target/debug` or `target/release` without relying on CARGO_MANIFEST_DIR.

use std::path::Path;

fn main() {
    let manifest_dir = Path::new(env!("CARGO_MANIFEST_DIR"));
    let out_dir_env = std::env::var("OUT_DIR").expect("OUT_DIR");
    let out_dir = Path::new(&out_dir_env);
    let target_dir = out_dir
        .ancestors()
        .nth(3)
        .expect("target dir from OUT_DIR");

    let src_ext = manifest_dir.join("extensions");
    let dst_ext = target_dir.join("extensions");
    if src_ext.is_dir() {
        let _ = copy_dir_recursive(&src_ext, &dst_ext);
    }

    println!("cargo:rerun-if-changed=extensions");
}

fn copy_dir_recursive(src: &Path, dst: &Path) -> std::io::Result<()> {
    std::fs::create_dir_all(dst)?;
    for entry in std::fs::read_dir(src)? {
        let entry = entry?;
        let ty = entry.file_type()?;
        let from = entry.path();
        let to = dst.join(entry.file_name());
        if ty.is_dir() {
            copy_dir_recursive(&from, &to)?;
        } else if ty.is_file() {
            std::fs::copy(&from, &to)?;
        }
    }
    Ok(())
}
