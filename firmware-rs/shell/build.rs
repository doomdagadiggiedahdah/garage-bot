//! No secrets at build time: WiFi, Telegram and NOTIFY are read from the NVS
//! partition at boot. This only bakes in the short proof root for /version.
use std::{env, fs, path::Path};

fn main() {
    embuild::espidf::sysenv::output();
    println!("cargo:rerun-if-changed=../ROOT.txt");
    let root = fs::read_to_string("../ROOT.txt").unwrap_or_default();
    // First line is "root <64 hex>".
    let full = root.lines().next().and_then(|l| l.strip_prefix("root ")).unwrap_or("");
    let hex: String = full.chars().take_while(|c| c.is_ascii_hexdigit()).take(8).collect();
    let short = if hex.len() == 8 { hex } else { "unknown".into() };
    let out = Path::new(&env::var("OUT_DIR").unwrap()).join("proof_root.rs");
    fs::write(out, format!("pub const PROOF_ROOT: &str = {short:?};\n")).unwrap();
}
