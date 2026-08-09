fn main() {
    let commit = std::env::var("CONFIG_EDITOR_COMMIT").unwrap_or_else(|_| "unknown".to_string());
    let date = std::env::var("CONFIG_EDITOR_DATE").unwrap_or_else(|_| "unknown".to_string());
    println!("cargo:rustc-env=CONFIG_EDITOR_COMMIT={commit}");
    println!("cargo:rustc-env=CONFIG_EDITOR_DATE={date}");
    println!("cargo:rerun-if-env-changed=CONFIG_EDITOR_COMMIT");
    println!("cargo:rerun-if-env-changed=CONFIG_EDITOR_DATE");
}
