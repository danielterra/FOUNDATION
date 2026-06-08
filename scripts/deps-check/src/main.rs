//! Relatório de atualizações de dependências do FOUNDATION (npm + Cargo).
//!
//! Modos:
//!   --hook   Saída como envelope JSON de SessionStart (additionalContext); respeita
//!            o cache diário e fica em silêncio quando não há nada para atualizar.
//!   --force  Ignora o cache e imprime o relatório em markdown puro (usado pela skill).
//!   (default) igual a --force, porém sem ignorar o cache.

use std::process::Command;
use std::time::{SystemTime, UNIX_EPOCH};

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let hook_mode = args.iter().any(|a| a == "--hook");
    let force = args.iter().any(|a| a == "--force");

    let today = day_bucket();
    let cache = std::env::temp_dir().join("foundation-deps-check.cache");

    // No modo automático (hook), só roda uma vez por dia: a primeira sessão do dia
    // dispara a verificação; as seguintes ficam em silêncio.
    if hook_mode && !force {
        if let Ok(content) = std::fs::read_to_string(&cache) {
            if content.trim() == today.to_string() {
                return;
            }
        }
    }

    let npm = check_npm();
    let cargo = check_cargo();
    let has_updates = !npm.is_empty() || !cargo.is_empty();

    // Marca o dia como verificado para não consultar a rede de novo nas próximas sessões.
    let _ = std::fs::write(&cache, today.to_string());

    if hook_mode {
        if !has_updates {
            return;
        }
        let report = build_report(&npm, &cargo, today);
        let context = format!(
            "[Verificação automática diária de dependências — apresente este relatório ao usuário de forma concisa]\n\n{report}"
        );
        println!(
            "{{\"hookSpecificOutput\":{{\"hookEventName\":\"SessionStart\",\"additionalContext\":\"{}\"}}}}",
            escape_json(&context)
        );
    } else {
        println!("{}", build_report(&npm, &cargo, today));
    }
}

/// Dias desde a época Unix — chave do cache diário.
fn day_bucket() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs() / 86_400)
        .unwrap_or(0)
}

/// `npm outdated` imprime uma tabela no stdout e sai com código 1 quando há pacotes
/// desatualizados — capturamos o stdout independentemente do código de saída.
fn check_npm() -> String {
    let output = if cfg!(windows) {
        Command::new("cmd").args(["/C", "npm", "outdated"]).output()
    } else {
        Command::new("npm").arg("outdated").output()
    };
    match output {
        Ok(o) => String::from_utf8_lossy(&o.stdout).trim().to_string(),
        Err(_) => String::new(),
    }
}

/// `cargo update --dry-run` lista no stderr o que seria atualizado dentro do semver do
/// Cargo.toml. Mantemos apenas as linhas com a seta de versão (` -> `); a linha de
/// "Updating crates.io index" não tem seta e é descartada.
fn check_cargo() -> String {
    let output = Command::new("cargo")
        .args([
            "update",
            "--dry-run",
            "--manifest-path",
            "src-tauri/Cargo.toml",
        ])
        .output();
    let raw = match output {
        Ok(o) => {
            let mut s = String::from_utf8_lossy(&o.stderr).into_owned();
            s.push_str(&String::from_utf8_lossy(&o.stdout));
            s
        }
        Err(_) => return String::new(),
    };
    raw.lines()
        .map(|l| l.trim())
        .filter(|l| l.contains(" -> "))
        .map(|l| format!("- {l}"))
        .collect::<Vec<_>>()
        .join("\n")
}

fn build_report(npm: &str, cargo: &str, day: u64) -> String {
    let (y, m, d) = civil_from_days(day as i64);
    let npm_section = if npm.is_empty() {
        "Tudo atualizado ✅".to_string()
    } else {
        format!("```\n{npm}\n```")
    };
    let cargo_section = if cargo.is_empty() {
        "Tudo atualizado ✅ (dentro do semver do Cargo.toml)".to_string()
    } else {
        cargo.to_string()
    };
    format!(
        "## 📦 Atualizações de dependências disponíveis\n\
         _Verificação automática — {y:04}-{m:02}-{d:02}_\n\n\
         ### Frontend (npm — `package.json`)\n{npm_section}\n\n\
         ### Backend (Cargo — `src-tauri/Cargo.toml`, semver-compatível)\n{cargo_section}\n\n\
         > Bumps **major** de crates Rust não aparecem aqui (exigiriam `cargo-outdated`). \
         Para reverificar a qualquer momento: `/deps-check` ou `npm run deps:check`."
    )
}

/// Converte dias desde 1970-01-01 em (ano, mês, dia) — algoritmo civil de Howard Hinnant,
/// evita depender de uma crate de datas.
fn civil_from_days(days: i64) -> (i64, u64, u64) {
    let z = days + 719_468;
    let era = if z >= 0 { z } else { z - 146_096 } / 146_097;
    let doe = (z - era * 146_097) as u64;
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let y = yoe as i64 + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = doy - (153 * mp + 2) / 5 + 1;
    let m = if mp < 10 { mp + 3 } else { mp - 9 };
    (if m <= 2 { y + 1 } else { y }, m, d)
}

fn escape_json(s: &str) -> String {
    let mut out = String::with_capacity(s.len() + 16);
    for c in s.chars() {
        match c {
            '"' => out.push_str("\\\""),
            '\\' => out.push_str("\\\\"),
            '\n' => out.push_str("\\n"),
            '\r' => out.push_str("\\r"),
            '\t' => out.push_str("\\t"),
            c if (c as u32) < 0x20 => out.push_str(&format!("\\u{:04x}", c as u32)),
            c => out.push(c),
        }
    }
    out
}
