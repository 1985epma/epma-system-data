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

fn gui() -> ExitCode {
    let options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default()
            .with_title("EPMA System Data")
            .with_inner_size([920.0, 720.0])
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
