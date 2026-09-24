//! Embeds the bundled browser extensions into the executable so the app
//! ships as a single exe (unpacked at runtime by `bundle.rs`).

use std::fmt::Write as _;
use std::path::{Path, PathBuf};

/// Must match `extension::BUNDLED`.
const EMBEDDED_EXTENSIONS: &[&str] = &["sponsorblock", "return-youtube-dislike"];

fn main() {
    let manifest_dir = PathBuf::from(std::env::var("CARGO_MANIFEST_DIR").expect("manifest dir"));
    let out_dir = PathBuf::from(std::env::var("OUT_DIR").expect("OUT_DIR"));

    let mut files = Vec::new();
    for name in EMBEDDED_EXTENSIONS {
        let dir = manifest_dir.join("extensions").join(name);
        collect(&dir, &dir, name, &mut files);
        println!("cargo:rerun-if-changed={}", dir.display());
    }
    files.sort();

    // FNV-1a over paths and contents: tells the runtime when to re-unpack.
    let mut hash: u64 = 0xcbf29ce484222325;
    let mut feed = |bytes: &[u8]| {
        for b in bytes {
            hash ^= *b as u64;
            hash = hash.wrapping_mul(0x100000001b3);
        }
    };

    let mut code = String::from("pub static EXTENSION_FILES: &[(&str, &[u8])] = &[\n");
    for (rel, path) in &files {
        feed(rel.as_bytes());
        feed(&std::fs::read(path).expect("read extension file"));
        writeln!(code, "    ({rel:?}, include_bytes!({:?})),", path.display().to_string()).unwrap();
    }
    code.push_str("];\n");
    writeln!(code, "pub static EXTENSION_NAMES: &[&str] = &{EMBEDDED_EXTENSIONS:?};").unwrap();
    writeln!(code, "pub const BUNDLE_HASH: u64 = {hash:#x};").unwrap();
    std::fs::write(out_dir.join("embedded.rs"), code).expect("write embedded.rs");

    println!("cargo:rerun-if-changed=vendor/webview2/WebView2Loader.dll");
}

fn collect(root: &Path, dir: &Path, prefix: &str, out: &mut Vec<(String, PathBuf)>) {
    let entries = std::fs::read_dir(dir).unwrap_or_else(|e| panic!("{}: {e}", dir.display()));
    for entry in entries {
        let path = entry.expect("dir entry").path();
        if path.is_dir() {
            collect(root, &path, prefix, out);
        } else {
            let rel = path.strip_prefix(root).unwrap().to_string_lossy().replace('\\', "/");
            out.push((format!("{prefix}/{rel}"), path));
        }
    }
}
