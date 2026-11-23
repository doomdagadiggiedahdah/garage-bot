// verifier_proofs.rs is written by an independent agent and may not exist yet.
// Set GB_NO_VERIFIER=1 to build without it.
fn main() {
    println!("cargo:rerun-if-changed=src/verifier_proofs.rs");
    println!("cargo:rerun-if-env-changed=GB_NO_VERIFIER");
    if std::path::Path::new("src/verifier_proofs.rs").exists() && std::env::var_os("GB_NO_VERIFIER").is_none() {
        println!("cargo:rustc-cfg=has_verifier_proofs");
    }
}
