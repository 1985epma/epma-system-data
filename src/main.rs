use eframe::egui;
use epma_system_data::{app::App, categories, scanner, util::format_bytes};

fn main() -> eframe::Result {
    // `--scan` imprime o relatório no terminal, sem abrir a janela.
    if std::env::args().any(|a| a == "--scan") {
        print_report();
        return Ok(());
    }

    let options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default()
            .with_title("EPMA System Data")
            .with_inner_size([920.0, 720.0])
            .with_min_inner_size([680.0, 480.0]),
        ..Default::default()
    };
    eframe::run_native(
        "EPMA System Data",
        options,
        Box::new(|cc| Ok(Box::new(App::new(cc)))),
    )
}

fn print_report() {
    let mut total = 0;
    for cat in categories::all() {
        let scan = scanner::scan(&cat);
        let size = match &scan.note {
            Some(note) if scan.items.is_some() || scan.unavailable => note.clone(),
            _ => format_bytes(scan.total),
        };
        println!("{:<44} {size}", cat.name);
        total += scan.total;
    }
    println!("{:<44} {}", "Total em arquivos", format_bytes(total));
}
