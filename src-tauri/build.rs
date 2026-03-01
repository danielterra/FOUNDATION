fn main() {
    // Tell Cargo to rerun build script if core-ontology changes
    println!("cargo:rerun-if-changed=../core-ontology");

    tauri_build::build()
}
