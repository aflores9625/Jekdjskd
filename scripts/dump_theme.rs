// One-off: `rustc scripts/dump_theme.rs && ./dump_theme.exe`
fn main() {
    include!("../src/theme.rs");
    let js = injection_script();
    std::fs::write("target/theme_fresh.js", &js).expect("write");
    eprintln!("wrote {} bytes", js.len());
}
