//! Componentes visuais reutilizados pela janela. Todos recebem a `Palette`: nenhuma cor
//! fixa aqui.

use eframe::egui::{
    self, vec2, Align, Color32, Layout, Margin, Response, RichText, Rounding, Sense, Shape, Stroke,
    Ui,
};

use crate::theme::{Bold, Palette};

/// Cartão branco (ou cinza escuro) com borda fina, como no padrão SaaS.
pub fn card<R>(ui: &mut Ui, p: &Palette, add: impl FnOnce(&mut Ui) -> R) -> R {
    egui::Frame::none()
        .fill(p.card)
        .stroke(Stroke::new(1.0_f32, p.border))
        .rounding(Rounding::same(12.0))
        .inner_margin(Margin::same(18.0))
        .show(ui, |ui| {
            ui.set_width(ui.available_width());
            add(ui)
        })
        .inner
}

/// Pílula de status: fundo suave, texto escuro, contorno sutil.
pub fn pill(ui: &mut Ui, text: &str, bg: Color32, fg: Color32) {
    egui::Frame::none()
        .fill(bg)
        .stroke(Stroke::new(1.0_f32, fg.gamma_multiply(0.28)))
        .rounding(Rounding::same(20.0))
        .inner_margin(Margin::symmetric(8.0, 1.0))
        .show(ui, |ui| {
            ui.label(RichText::new(text).size(11.5).bold().color(fg));
        });
}

pub fn pill_safe(ui: &mut Ui, p: &Palette, text: &str) {
    pill(ui, text, p.ok_bg, p.ok_fg);
}

pub fn pill_review(ui: &mut Ui, p: &Palette, text: &str) {
    pill(ui, text, p.warn_bg, p.warn_fg);
}

pub fn pill_critical(ui: &mut Ui, p: &Palette, text: &str) {
    pill(ui, text, p.err_bg, p.err_fg);
}

/// Checkbox no estilo da proposta: quadrado arredondado, preenchido com a cor de destaque.
pub fn check(ui: &mut Ui, p: &Palette, on: &mut bool, enabled: bool) -> Response {
    let sense = if enabled {
        Sense::click()
    } else {
        Sense::hover()
    };
    let (rect, mut response) = ui.allocate_exact_size(vec2(18.0, 18.0), sense);
    if enabled && response.clicked() {
        *on = !*on;
        response.mark_changed();
    }
    let rect = rect.shrink(1.0);
    let fade = if enabled { 1.0 } else { 0.4 };
    if *on {
        ui.painter()
            .rect_filled(rect, Rounding::same(4.5), p.accent.gamma_multiply(fade));
        let c = rect.center();
        let s = rect.width() / 18.0 * 18.0;
        let pts = vec![
            c + vec2(-0.27 * s, 0.02 * s),
            c + vec2(-0.07 * s, 0.24 * s),
            c + vec2(0.29 * s, -0.22 * s),
        ];
        ui.painter().add(Shape::line(
            pts,
            Stroke::new(2.0_f32, Color32::WHITE.gamma_multiply(fade)),
        ));
    } else {
        let stroke = if response.hovered() && enabled {
            p.faint
        } else {
            p.border.gamma_multiply(1.0)
        };
        ui.painter().rect(
            rect,
            Rounding::same(4.5),
            p.card,
            Stroke::new(1.3_f32, stroke),
        );
    }
    response
}

/// Botão primário: cor de destaque, texto branco. `danger` usa o vermelho.
pub fn primary_button(
    ui: &mut Ui,
    p: &Palette,
    text: &str,
    enabled: bool,
    danger: bool,
) -> Response {
    let (base, hover) = if danger {
        (p.danger, p.danger_hover)
    } else {
        (p.accent, p.accent_hover)
    };
    ui.scope(|ui| {
        let w = &mut ui.visuals_mut().widgets;
        for state in [&mut w.inactive, &mut w.hovered, &mut w.active] {
            state.bg_stroke = Stroke::NONE;
            state.fg_stroke = Stroke::new(1.0_f32, Color32::WHITE);
        }
        w.inactive.weak_bg_fill = base;
        w.hovered.weak_bg_fill = hover;
        w.active.weak_bg_fill = hover;
        ui.add_enabled(
            enabled,
            egui::Button::new(RichText::new(text).bold().color(Color32::WHITE))
                .rounding(Rounding::same(8.0))
                .min_size(vec2(0.0, 34.0)),
        )
    })
    .inner
}

/// Botão secundário (contorno, fundo do cartão).
pub fn ghost_button(ui: &mut Ui, text: &str, enabled: bool) -> Response {
    ui.add_enabled(
        enabled,
        egui::Button::new(RichText::new(text)).min_size(vec2(0.0, 34.0)),
    )
}

/// Barra fina proporcional (fração entre 0 e 1).
pub fn mini_bar(ui: &mut Ui, frac: f32, fill: Color32, track: Color32, width: f32) {
    let (rect, _) = ui.allocate_exact_size(vec2(width, 6.0), Sense::hover());
    ui.painter().rect_filled(rect, Rounding::same(3.0), track);
    if frac > 0.0 {
        let mut filled = rect;
        filled.set_width((rect.width() * frac.clamp(0.0, 1.0)).max(3.0));
        ui.painter().rect_filled(filled, Rounding::same(3.0), fill);
    }
}

/// Placeholder cinza que mantém a estrutura da tela enquanto os dados carregam.
pub fn skeleton(ui: &mut Ui, p: &Palette, width: f32, height: f32) {
    let (rect, _) = ui.allocate_exact_size(vec2(width, height), Sense::hover());
    ui.painter()
        .rect_filled(rect, Rounding::same(6.0), p.track.gamma_multiply(1.0));
    // Um brilho lento e discreto (sem animação pesada), só para indicar que está carregando.
    let t = ui.input(|i| i.time) as f32;
    let k = (t * 0.9).fract();
    let shine = egui::Rect::from_min_size(
        rect.min + vec2((rect.width() + 40.0) * k - 40.0, 0.0),
        vec2(40.0, rect.height()),
    )
    .intersect(rect);
    if shine.width() > 0.0 {
        let c = if p.dark {
            Color32::from_white_alpha(10)
        } else {
            Color32::from_white_alpha(120)
        };
        ui.painter().rect_filled(shine, Rounding::same(6.0), c);
    }
    ui.ctx()
        .request_repaint_after(std::time::Duration::from_millis(60));
}

#[derive(Clone, Copy, PartialEq, Eq)]
pub enum BannerKind {
    Error,
    Warn,
    Ok,
}

/// Aviso inline no topo da tela (nunca pop-up). Devolve `true` se o usuário fechou.
/// `actions` desenha os botões de ação abaixo do texto.
pub fn banner(
    ui: &mut Ui,
    p: &Palette,
    kind: BannerKind,
    title: &str,
    body: &str,
    actions: impl FnOnce(&mut Ui),
) -> bool {
    let (bg, fg, border, glyph) = match kind {
        BannerKind::Error => (p.err_bg, p.err_fg, p.err_border, "⚠"),
        BannerKind::Warn => (p.warn_bg, p.warn_fg, p.warn_fg.gamma_multiply(0.35), "⚠"),
        BannerKind::Ok => (p.ok_bg, p.ok_fg, p.ok_fg.gamma_multiply(0.35), "✔"),
    };
    let mut closed = false;
    egui::Frame::none()
        .fill(bg)
        .stroke(Stroke::new(1.0_f32, border))
        .rounding(Rounding::same(12.0))
        .inner_margin(Margin::symmetric(16.0, 12.0))
        .show(ui, |ui| {
            ui.set_width(ui.available_width());
            ui.horizontal_top(|ui| {
                ui.label(RichText::new(glyph).size(17.0).color(fg));
                ui.vertical(|ui| {
                    ui.label(RichText::new(title).bold().color(fg));
                    if !body.is_empty() {
                        ui.label(RichText::new(body).color(fg));
                    }
                    ui.add_space(2.0);
                    ui.horizontal_wrapped(|ui| {
                        ui.scope(|ui| {
                            let w = &mut ui.visuals_mut().widgets;
                            for state in [&mut w.inactive, &mut w.hovered, &mut w.active] {
                                state.fg_stroke = Stroke::new(1.0_f32, fg);
                                state.bg_stroke = Stroke::new(1.0_f32, fg.gamma_multiply(0.4));
                            }
                            ui.spacing_mut().button_padding = vec2(10.0, 3.0);
                            actions(ui);
                        });
                    });
                });
                ui.with_layout(Layout::right_to_left(Align::Min), |ui| {
                    if close_button(ui, fg).clicked() {
                        closed = true;
                    }
                });
            });
        });
    closed
}

/// Linha horizontal fina na cor da borda.
pub fn hline(ui: &mut Ui, p: &Palette) {
    let (rect, _) = ui.allocate_exact_size(vec2(ui.available_width(), 1.0), Sense::hover());
    ui.painter().rect_filled(rect, 0.0, p.border);
}

/// Seta desenhada com linhas (não depende de glifos da fonte): `open` aponta para baixo,
/// fechada aponta para a direita.
pub fn chevron(ui: &mut Ui, color: Color32, open: bool) {
    let (rect, _) = ui.allocate_exact_size(vec2(16.0, 16.0), Sense::hover());
    let c = rect.center();
    let pts = if open {
        vec![
            c + vec2(-4.0, -2.0),
            c + vec2(0.0, 2.5),
            c + vec2(4.0, -2.0),
        ]
    } else {
        vec![
            c + vec2(-2.0, -4.0),
            c + vec2(2.5, 0.0),
            c + vec2(-2.0, 4.0),
        ]
    };
    ui.painter()
        .add(Shape::line(pts, Stroke::new(1.6_f32, color)));
}

/// Bolinha de status desenhada (a fonte padrão do egui não tem o glifo "●").
pub fn dot(ui: &mut Ui, color: Color32, diameter: f32) {
    let (rect, _) = ui.allocate_exact_size(vec2(diameter + 2.0, diameter + 2.0), Sense::hover());
    ui.painter()
        .circle_filled(rect.center(), diameter / 2.0, color);
}

/// Botão "fechar" com o X desenhado (a fonte padrão não tem o glifo "✕").
pub fn close_button(ui: &mut Ui, color: Color32) -> Response {
    let (rect, resp) = ui.allocate_exact_size(vec2(22.0, 22.0), Sense::click());
    let resp = resp.on_hover_cursor(egui::CursorIcon::PointingHand);
    let c = if resp.hovered() {
        color
    } else {
        color.gamma_multiply(0.75)
    };
    let d = 4.5;
    let stroke = Stroke::new(1.7_f32, c);
    let center = rect.center();
    ui.painter()
        .line_segment([center + vec2(-d, -d), center + vec2(d, d)], stroke);
    ui.painter()
        .line_segment([center + vec2(-d, d), center + vec2(d, -d)], stroke);
    resp
}
