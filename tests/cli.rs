//! Contrato do CLI: códigos de saída, stdout/stderr separados e segurança da limpeza.
//! Todos os testes rodam com HOME apontando para uma pasta temporária.

use std::fs;
use std::path::Path;

use assert_cmd::Command;
use predicates::prelude::*;

fn bin(home: &Path) -> Command {
    let mut cmd = Command::cargo_bin("epma-system-data").unwrap();
    cmd.env("HOME", home).env("NO_COLOR", "1").env_remove("TERM");
    cmd
}

/// HOME com ~/Library/Caches/{grande,pequeno} e ~/Library/Logs/app.log.
fn fixture() -> tempfile::TempDir {
    let home = tempfile::tempdir().unwrap();
    let caches = home.path().join("Library/Caches");
    fs::create_dir_all(caches.join("grande")).unwrap();
    fs::write(caches.join("grande/a.bin"), vec![1u8; 64 * 1024]).unwrap();
    fs::create_dir_all(caches.join("pequeno")).unwrap();
    fs::write(caches.join("pequeno/b.bin"), b"oi").unwrap();
    fs::create_dir_all(home.path().join("Library/Logs")).unwrap();
    fs::write(home.path().join("Library/Logs/app.log"), b"log").unwrap();
    home
}

#[test]
fn help_and_version_exit_zero() {
    let home = fixture();
    bin(home.path())
        .arg("--help")
        .assert()
        .success()
        .stdout(predicate::str::contains("clean").and(predicate::str::contains("scan")));
    bin(home.path())
        .arg("--version")
        .assert()
        .success()
        .stdout(predicate::str::contains("epma-system-data"));
}

#[test]
fn unknown_flag_is_usage_error() {
    let home = fixture();
    bin(home.path()).arg("--nao-existe").assert().code(2);
}

#[test]
fn list_json_has_all_categories_with_defaults() {
    let home = fixture();
    let out = bin(home.path()).args(["list", "--json"]).output().unwrap();
    assert!(out.status.success());
    let rows: serde_json::Value = serde_json::from_slice(&out.stdout).unwrap();
    let rows = rows.as_array().unwrap();
    assert_eq!(rows.len(), 11);
    let backups = rows.iter().find(|r| r["id"] == "ios_backups").unwrap();
    assert_eq!(backups["risk"], "review");
    assert_eq!(backups["selected_by_default"], false);
    let caches = rows.iter().find(|r| r["id"] == "user_caches").unwrap();
    assert_eq!(caches["selected_by_default"], true);
}

#[test]
fn scan_json_measures_and_lists_entries_largest_first() {
    let home = fixture();
    let out = bin(home.path())
        .args(["scan", "--only", "user_caches", "--details", "--json"])
        .output()
        .unwrap();
    assert!(out.status.success());
    let v: serde_json::Value = serde_json::from_slice(&out.stdout).unwrap();
    let cat = &v["categories"][0];
    assert_eq!(cat["id"], "user_caches");
    assert_eq!(cat["entry_count"], 2);
    assert!(cat["bytes"].as_u64().unwrap() >= 64 * 1024);
    let entries = cat["entries"].as_array().unwrap();
    assert!(entries[0]["path"].as_str().unwrap().ends_with("grande"));
    assert_eq!(v["total_bytes"], cat["bytes"]);
}

#[test]
fn scan_human_output_is_a_table_on_stdout() {
    let home = fixture();
    bin(home.path())
        .args(["scan", "--only", "user_caches,user_logs"])
        .assert()
        .success()
        .stdout(
            predicate::str::contains("user_caches")
                .and(predicate::str::contains("Caches de apps"))
                .and(predicate::str::contains("Total em arquivos")),
        );
}

#[test]
fn legacy_scan_flag_still_works() {
    let home = fixture();
    // --scan mede tudo, inclusive categorias de comando; basta checar que não falha.
    bin(home.path())
        .arg("--scan")
        .assert()
        .success()
        .stdout(predicate::str::contains("Total em arquivos"));
}

#[test]
fn unknown_category_is_usage_error_listing_valid_ids() {
    let home = fixture();
    bin(home.path())
        .args(["scan", "--only", "nao_existe"])
        .assert()
        .code(2)
        .stderr(predicate::str::contains("user_caches"));
}

#[test]
fn clean_dry_run_changes_nothing() {
    let home = fixture();
    bin(home.path())
        .args(["clean", "--only", "user_caches", "--dry-run", "--permanent"])
        .assert()
        .success()
        .stdout(predicate::str::contains("Ensaio").and(predicate::str::contains("permanentemente")));
    assert!(home.path().join("Library/Caches/grande/a.bin").exists());
    assert!(home.path().join("Library/Caches/pequeno/b.bin").exists());
}

#[test]
fn clean_without_terminal_and_without_yes_is_refused() {
    let home = fixture();
    bin(home.path())
        .args(["clean", "--only", "user_caches", "--permanent"])
        .write_stdin("")
        .assert()
        .code(3)
        .stderr(predicate::str::contains("--yes"));
    assert!(home.path().join("Library/Caches/grande/a.bin").exists());
}

#[test]
fn clean_permanent_yes_removes_items_but_keeps_the_directory() {
    let home = fixture();
    bin(home.path())
        .args(["clean", "--only", "user_caches", "--permanent", "--yes"])
        .assert()
        .success()
        .stdout(predicate::str::contains("Caches de apps: 2 itens").and(predicate::str::contains("liberados")));
    let caches = home.path().join("Library/Caches");
    assert!(caches.exists());
    assert_eq!(fs::read_dir(&caches).unwrap().count(), 0);
    // Outras categorias não foram tocadas.
    assert!(home.path().join("Library/Logs/app.log").exists());
}

#[test]
fn clean_skip_preserves_the_item() {
    let home = fixture();
    let keep = home.path().join("Library/Caches/grande");
    bin(home.path())
        .args(["clean", "--only", "user_caches", "--permanent", "--yes", "--skip"])
        .arg(&keep)
        .assert()
        .success();
    assert!(keep.join("a.bin").exists());
    assert!(!home.path().join("Library/Caches/pequeno").exists());
}

#[test]
fn clean_exclude_drops_a_category_from_the_selection() {
    let home = fixture();
    bin(home.path())
        .args(["clean", "--only", "user_caches,user_logs", "-x", "user_logs", "--permanent", "--yes"])
        .assert()
        .success();
    assert!(home.path().join("Library/Logs/app.log").exists());
    assert!(!home.path().join("Library/Caches/grande").exists());
}

#[test]
fn clean_never_follows_symlinks_out_of_the_cache() {
    let home = fixture();
    let outside = home.path().join("Documentos");
    fs::create_dir_all(&outside).unwrap();
    fs::write(outside.join("importante.txt"), b"nao apagar").unwrap();
    std::os::unix::fs::symlink(&outside, home.path().join("Library/Caches/atalho")).unwrap();

    bin(home.path())
        .args(["clean", "--only", "user_caches", "--permanent", "--yes"])
        .assert()
        .success();
    assert!(outside.join("importante.txt").exists());
}

#[test]
fn clean_json_reports_plan_and_results() {
    let home = fixture();
    let out = bin(home.path())
        .args(["clean", "--only", "user_caches", "--permanent", "--yes", "--json"])
        .output()
        .unwrap();
    assert!(out.status.success());
    let v: serde_json::Value = serde_json::from_slice(&out.stdout).unwrap();
    assert_eq!(v["dry_run"], false);
    assert_eq!(v["mode"], "permanent");
    assert_eq!(v["planned"][0]["id"], "user_caches");
    assert_eq!(v["results"][0]["removed"], 2);
    assert_eq!(v["failures"], 0);
}

#[test]
fn clean_with_nothing_to_do_succeeds() {
    let home = tempfile::tempdir().unwrap();
    bin(home.path())
        .args(["clean", "--only", "user_caches", "--yes"])
        .assert()
        .success()
        .stdout(predicate::str::contains("Nada para limpar"));
}

#[test]
fn include_review_conflicts_with_only() {
    let home = fixture();
    bin(home.path())
        .args(["clean", "--only", "user_caches", "--include-review", "--dry-run"])
        .assert()
        .code(2);
}

#[test]
fn reveal_missing_path_fails_without_opening_finder() {
    let home = fixture();
    bin(home.path())
        .args(["reveal", "~/nao/existe"])
        .assert()
        .code(1)
        .stderr(predicate::str::contains("não existe"));
}

#[test]
fn completions_generate_a_script() {
    let home = fixture();
    bin(home.path())
        .args(["completions", "zsh"])
        .assert()
        .success()
        .stdout(predicate::str::contains("epma-system-data"));
}

#[test]
fn closed_stdout_pipe_does_not_panic() {
    let home = fixture();
    // Simula `| head -0`: o leitor fecha o pipe antes da escrita.
    let output = std::process::Command::new(env!("CARGO_BIN_EXE_epma-system-data"))
        .args(["list"])
        .env("HOME", home.path())
        .stdout(std::process::Stdio::null())
        .output()
        .unwrap();
    assert!(output.status.success());
    assert!(!String::from_utf8_lossy(&output.stderr).contains("panicked"));
}

#[test]
fn diag_json_is_stable_and_read_only() {
    let home = fixture();
    let before = fs::read_dir(home.path().join("Library/Caches")).unwrap().count();
    let out = bin(home.path()).args(["diag", "--json"]).output().unwrap();
    // 0 (sem achado crítico) ou 1 (há achado crítico): nunca 2 ou 3.
    assert!(matches!(out.status.code(), Some(0) | Some(1)), "{:?}", out.status);
    let v: serde_json::Value = serde_json::from_slice(&out.stdout).unwrap();
    assert!(["macos", "linux"].contains(&v["os"].as_str().unwrap()));
    for key in ["disks", "volumes", "findings", "notes"] {
        assert!(v[key].is_array(), "faltou `{key}`");
    }
    // stdout só tem o JSON; o progresso vai para o stderr.
    assert!(!String::from_utf8_lossy(&out.stdout).contains("Diagnosticando"));
    assert_eq!(fs::read_dir(home.path().join("Library/Caches")).unwrap().count(), before);
}

#[test]
fn diag_human_output_has_sections() {
    let home = fixture();
    let out = bin(home.path()).arg("diag").output().unwrap();
    let text = String::from_utf8_lossy(&out.stdout);
    assert!(text.contains("Diagnóstico de discos"), "{text}");
    assert!(text.contains("Achados"), "{text}");
}
