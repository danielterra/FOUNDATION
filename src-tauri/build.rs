const MODEL_FILE: &str = "resources/models/gemma-4-E4B-it-Q4_K_M.gguf";
const MODEL_URL: &str = "https://huggingface.co/unsloth/gemma-4-E4B-it-GGUF/resolve/main/gemma-4-E4B-it-Q4_K_M.gguf";

fn main() {
    println!("cargo:rerun-if-changed=../core-ontology");
    println!("cargo:rerun-if-changed=../node_modules/material-symbols/index.d.ts");
    println!("cargo:rerun-if-changed=../node_modules/material-symbols/package.json");
    println!("cargo:rerun-if-changed={}", MODEL_FILE);

    download_model_if_missing();
    generate_material_symbols_list();

    tauri_build::build()
}

fn download_model_if_missing() {
    if std::path::Path::new(MODEL_FILE).exists() {
        return;
    }

    println!("cargo:warning=┌─────────────────────────────────────────────────────────┐");
    println!("cargo:warning=│  Modelo Gemma 4 E4B não encontrado. Baixando ~4.9 GB... │");
    println!("cargo:warning=│  Fonte: HuggingFace / unsloth                           │");
    println!("cargo:warning=│  O download pode ser retomado se interrompido.          │");
    println!("cargo:warning=└─────────────────────────────────────────────────────────┘");

    std::fs::create_dir_all("resources/models")
        .expect("Falha ao criar diretório resources/models");

    let status = std::process::Command::new("curl")
        .args([
            "-L",            // seguir redirecionamentos
            "-C", "-",       // retomar download interrompido
            "--progress-bar",
            "-o", MODEL_FILE,
            MODEL_URL,
        ])
        .status()
        .expect("'curl' não encontrado. Instale curl e tente novamente.");

    if !status.success() {
        // Remove arquivo parcial para não deixar lixo
        let _ = std::fs::remove_file(MODEL_FILE);
        panic!(
            "Falha ao baixar o modelo Gemma (curl saiu com {:?}). \
             Verifique sua conexão e tente novamente.",
            status.code()
        );
    }

    println!("cargo:warning=Modelo baixado com sucesso.");
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
