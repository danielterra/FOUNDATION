fn main() {
    println!("cargo:rerun-if-changed=../core-ontology");
    println!("cargo:rerun-if-changed=../node_modules/material-symbols/index.d.ts");
    println!("cargo:rerun-if-changed=../node_modules/material-symbols/package.json");

    generate_material_symbols_list();

    tauri_build::build()
}

fn generate_material_symbols_list() {
    use std::fmt::Write as _;
    use std::path::PathBuf;

    let dts_path = PathBuf::from("../node_modules/material-symbols/index.d.ts");
    let pkg_path = PathBuf::from("../node_modules/material-symbols/package.json");
    let out_dir = std::env::var("OUT_DIR").unwrap();
    let out_path = PathBuf::from(&out_dir).join("material_symbols.rs");

    let content = std::fs::read_to_string(&dts_path)
        .expect("material-symbols index.d.ts not found — run `npm install`");

    let pkg_content = std::fs::read_to_string(&pkg_path)
        .expect("material-symbols package.json not found — run `npm install`");

    let version = pkg_content
        .lines()
        .find_map(|line| {
            let trimmed = line.trim();
            if trimmed.starts_with("\"version\"") {
                trimmed.split(':').nth(1).map(|v| v.trim().trim_matches(|c| c == '"' || c == ',').to_string())
            } else {
                None
            }
        })
        .expect("version not found in material-symbols package.json");

    let mut names: Vec<&str> = content
        .lines()
        .filter_map(|line| {
            let trimmed = line.trim();
            if trimmed.starts_with('"') && trimmed.ends_with("\",") {
                Some(&trimmed[1..trimmed.len() - 2])
            } else {
                None
            }
        })
        .collect();

    names.sort_unstable();

    let mut buf = String::new();
    writeln!(buf, "pub static MATERIAL_SYMBOLS_VERSION: &str = \"{version}\";").unwrap();
    writeln!(buf, "pub static MATERIAL_SYMBOLS: &[&str] = &[").unwrap();
    for name in &names {
        writeln!(buf, "    \"{name}\",").unwrap();
    }
    writeln!(buf, "];").unwrap();

    std::fs::write(&out_path, buf).unwrap();
}
