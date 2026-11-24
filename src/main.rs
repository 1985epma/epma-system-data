use std::process::ExitCode;

use eframe::egui;
use epma_system_data::{app::App, cli};

fn main() -> ExitCode {
    // Sem argumentos (duplo clique no .app, ou `epma-system-data`): abre a janela.
    // Qualquer subcomando ou flag cai no CLI, que devolve o código de saída.
    if std::env::args_os().len() <= 1 {
        return gui();
    }
    match cli::run(std::env::args_os()) {
        cli::Outcome::Gui => gui(),
        cli::Outcome::Exit(code) => code,
    }
}

/// Tamanho inicial da janela. Em build de debug, `EPMA_WINDOW=680x520` permite testar outras larguras.
fn window_size() -> [f32; 2] {
    let default = [1100.0, 780.0];
    if !cfg!(debug_assertions) {
        return default;
    }
    std::env::var("EPMA_WINDOW")
        .ok()
        .and_then(|v| {
            let (w, h) = v.split_once('x')?;
            Some([w.parse().ok()?, h.parse().ok()?])
        })
        .unwrap_or(default)
}

fn gui() -> ExitCode {
    let size = window_size();
    let options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default()
            .with_title("EPMA System Data")
            .with_inner_size(size)
            .with_min_inner_size([680.0, 480.0]),
        ..Default::default()
    };
    match eframe::run_native(
        "EPMA System Data",
        options,
        Box::new(|cc| Ok(Box::new(App::new(cc)))),
    ) {
        Ok(()) => ExitCode::SUCCESS,
        Err(e) => {
            eprintln!("erro: não foi possível abrir a janela: {e}");
            ExitCode::FAILURE
        }
    }
}
