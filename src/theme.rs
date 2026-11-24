//! Tema claro e escuro. Todas as cores da janela saem de `Palette` (mesmos nomes nos dois
//! temas); nenhum componente usa cor fixa. O escuro é preto neutro (cinzas sem tom azulado)
//! e o azul aparece só como destaque.

use eframe::egui::{
    self, vec2, Color32, FontData, FontDefinitions, FontFamily, FontId, RichText, Rounding, Stroke,
    TextStyle, Theme, Visuals,
};

use crate::prefs::ThemePref;

/// Inter (licença OFL, `assets/fonts/Inter-LICENSE.txt`): o egui só traz fontes finas e sem
/// negrito, e a hierarquia da proposta depende de peso (títulos em semibold, dados em regular).
const INTER_REGULAR: &[u8] = include_bytes!("../assets/fonts/Inter-Regular.ttf");
const INTER_SEMIBOLD: &[u8] = include_bytes!("../assets/fonts/Inter-SemiBold.ttf");
const BOLD_FAMILY: &str = "inter-semibold";

/// Texto em semibold: `RichText::new("x").bold()`. (O `strong()` do egui só muda a cor.)
pub trait Bold {
    fn bold(self) -> RichText;
}

impl Bold for RichText {
    fn bold(self) -> RichText {
        self.family(FontFamily::Name(BOLD_FAMILY.into()))
    }
}

fn install_fonts(ctx: &egui::Context) {
    let mut fonts = FontDefinitions::default();
    fonts
        .font_data
        .insert("inter".into(), FontData::from_static(INTER_REGULAR));
    fonts
        .font_data
        .insert(BOLD_FAMILY.into(), FontData::from_static(INTER_SEMIBOLD));
    if let Some(prop) = fonts.families.get_mut(&FontFamily::Proportional) {
        prop.insert(0, "inter".into());
    }
    // Semibold primeiro; o resto da lista serve de reserva para emojis e símbolos.
    let mut bold = vec![BOLD_FAMILY.to_string()];
    bold.extend(
        fonts
            .families
            .get(&FontFamily::Proportional)
            .cloned()
            .unwrap_or_default(),
    );
    fonts
        .families
        .insert(FontFamily::Name(BOLD_FAMILY.into()), bold);
    ctx.set_fonts(fonts);
}

const fn rgb(hex: u32) -> Color32 {
    Color32::from_rgb((hex >> 16) as u8, (hex >> 8) as u8, hex as u8)
}

#[derive(Clone, Copy, Debug)]
pub struct Palette {
    pub dark: bool,
    /// Fundo da janela.
    pub bg: Color32,
    /// Cartões, painéis e barras.
    pub card: Color32,
    pub border: Color32,
    /// Linha sob o mouse e cabeçalhos de seção.
    pub hover: Color32,
    /// Trilho de barras de progresso.
    pub track: Color32,
    pub text: Color32,
    pub muted: Color32,
    pub faint: Color32,
    /// Única cor de destaque: ação primária, checkbox marcado, item ativo.
    pub accent: Color32,
    pub accent_hover: Color32,
    pub accent_text: Color32,
    pub accent_soft: Color32,
    pub ok_bg: Color32,
    pub ok_fg: Color32,
    pub warn_bg: Color32,
    pub warn_fg: Color32,
    pub err_bg: Color32,
    pub err_fg: Color32,
    pub err_border: Color32,
    pub info_bg: Color32,
    pub info_fg: Color32,
    /// Barra de uso do disco: aviso e crítico.
    pub bar_warn: Color32,
    pub bar_crit: Color32,
    /// Ação destrutiva (exclusão permanente).
    pub danger: Color32,
    pub danger_hover: Color32,
}

impl Palette {
    pub fn light() -> Self {
        Self {
            dark: false,
            bg: rgb(0xF8FAFC),
            card: rgb(0xFFFFFF),
            border: rgb(0xE2E8F0),
            hover: rgb(0xF8FAFC),
            track: rgb(0xF1F5F9),
            text: rgb(0x1E293B),
            muted: rgb(0x64748B),
            faint: rgb(0x94A3B8),
            accent: rgb(0x2563EB),
            accent_hover: rgb(0x1D4ED8),
            accent_text: rgb(0x2563EB),
            accent_soft: rgb(0xEFF6FF),
            ok_bg: rgb(0xECFDF5),
            ok_fg: rgb(0x047857),
            warn_bg: rgb(0xFFFBEB),
            warn_fg: rgb(0xB45309),
            err_bg: rgb(0xFEF2F2),
            err_fg: rgb(0xB91C1C),
            err_border: rgb(0xFECACA),
            info_bg: rgb(0xEFF6FF),
            info_fg: rgb(0x1E40AF),
            bar_warn: rgb(0xF59E0B),
            bar_crit: rgb(0xEF4444),
            danger: rgb(0xDC2626),
            danger_hover: rgb(0xB91C1C),
        }
    }

    pub fn dark() -> Self {
        Self {
            dark: true,
            bg: rgb(0x0A0A0A),
            card: rgb(0x121212),
            border: rgb(0x2A2A2A),
            hover: rgb(0x181818),
            track: rgb(0x202020),
            text: rgb(0xE5E5E5),
            muted: rgb(0xA3A3A3),
            faint: rgb(0x737373),
            accent: rgb(0x3B82F6),
            accent_hover: rgb(0x2563EB),
            accent_text: rgb(0x93C5FD),
            accent_soft: Color32::from_rgba_premultiplied(9, 21, 39, 41),
            ok_bg: rgb(0x062C21),
            ok_fg: rgb(0x34D399),
            warn_bg: rgb(0x302308),
            warn_fg: rgb(0xFCD34D),
            err_bg: rgb(0x381418),
            err_fg: rgb(0xFCA5A5),
            err_border: rgb(0x7F1D1D),
            info_bg: rgb(0x0E182C),
            info_fg: rgb(0xBFDBFE),
            bar_warn: rgb(0xF59E0B),
            bar_crit: rgb(0xEF4444),
            danger: rgb(0xDC2626),
            danger_hover: rgb(0xEF4444),
        }
    }

    pub fn of(dark: bool) -> Self {
        if dark {
            Self::dark()
        } else {
            Self::light()
        }
    }

    /// Paleta do tema que está valendo agora (já resolvido o "seguir o sistema").
    pub fn current(ctx: &egui::Context) -> Self {
        Self::of(ctx.theme() == Theme::Dark)
    }
}

/// `Visuals` do egui montado a partir da paleta.
pub fn visuals(p: &Palette) -> Visuals {
    let mut v = if p.dark {
        Visuals::dark()
    } else {
        Visuals::light()
    };
    v.panel_fill = p.bg;
    v.window_fill = p.card;
    v.extreme_bg_color = p.card;
    v.faint_bg_color = p.hover;
    v.code_bg_color = p.track;
    v.window_stroke = Stroke::new(1.0_f32, p.border);
    v.window_rounding = Rounding::same(16.0);
    v.menu_rounding = Rounding::same(10.0);
    v.hyperlink_color = p.accent_text;
    v.selection.bg_fill = p.accent_soft;
    v.selection.stroke = Stroke::new(1.0_f32, p.accent);

    let rounding = Rounding::same(8.0);
    let w = &mut v.widgets;

    w.noninteractive.bg_fill = p.card;
    w.noninteractive.weak_bg_fill = p.card;
    w.noninteractive.bg_stroke = Stroke::new(1.0_f32, p.border);
    w.noninteractive.fg_stroke = Stroke::new(1.0_f32, p.text);
    w.noninteractive.rounding = rounding;

    w.inactive.bg_fill = p.card;
    w.inactive.weak_bg_fill = p.card;
    w.inactive.bg_stroke = Stroke::new(1.0_f32, p.border);
    w.inactive.fg_stroke = Stroke::new(1.0_f32, p.text);
    w.inactive.rounding = rounding;

    w.hovered.bg_fill = p.hover;
    w.hovered.weak_bg_fill = p.hover;
    w.hovered.bg_stroke = Stroke::new(1.0_f32, p.faint);
    w.hovered.fg_stroke = Stroke::new(1.0_f32, p.text);
    w.hovered.rounding = rounding;
    w.hovered.expansion = 0.0;

    w.active.bg_fill = p.track;
    w.active.weak_bg_fill = p.track;
    w.active.bg_stroke = Stroke::new(1.0_f32, p.faint);
    w.active.fg_stroke = Stroke::new(1.0_f32, p.text);
    w.active.rounding = rounding;
    w.active.expansion = 0.0;

    w.open.bg_fill = p.hover;
    w.open.weak_bg_fill = p.hover;
    w.open.bg_stroke = Stroke::new(1.0_f32, p.border);
    w.open.fg_stroke = Stroke::new(1.0_f32, p.text);
    w.open.rounding = rounding;

    v
}

/// Registra os dois temas e a preferência escolhida. Chamar uma vez ao abrir a janela.
pub fn install(ctx: &egui::Context, pref: ThemePref) {
    install_fonts(ctx);
    for (theme, palette) in [
        (Theme::Light, Palette::light()),
        (Theme::Dark, Palette::dark()),
    ] {
        ctx.set_visuals_of(theme, visuals(&palette));
        ctx.style_mut_of(theme, |style| {
            style.spacing.item_spacing = vec2(8.0, 8.0);
            style.spacing.button_padding = vec2(12.0, 6.0);
            style.spacing.interact_size.y = 32.0;
            style
                .text_styles
                .insert(TextStyle::Body, FontId::proportional(14.0));
            style
                .text_styles
                .insert(TextStyle::Button, FontId::proportional(14.0));
            style
                .text_styles
                .insert(TextStyle::Small, FontId::proportional(12.0));
            style
                .text_styles
                .insert(TextStyle::Heading, FontId::proportional(22.0));
            style
                .text_styles
                .insert(TextStyle::Monospace, FontId::monospace(13.0));
        });
    }
    apply_pref(ctx, pref);
}

/// Aplica "automático / claro / escuro".
pub fn apply_pref(ctx: &egui::Context, pref: ThemePref) {
    ctx.set_theme(match pref {
        ThemePref::Auto => egui::ThemePreference::System,
        ThemePref::Light => egui::ThemePreference::Light,
        ThemePref::Dark => egui::ThemePreference::Dark,
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Luminância relativa (WCAG) de uma cor opaca.
    fn luminance(c: Color32) -> f64 {
        let lin = |v: u8| {
            let v = v as f64 / 255.0;
            if v <= 0.03928 {
                v / 12.92
            } else {
                ((v + 0.055) / 1.055).powf(2.4)
            }
        };
        0.2126 * lin(c.r()) + 0.7152 * lin(c.g()) + 0.0722 * lin(c.b())
    }

    fn contrast(a: Color32, b: Color32) -> f64 {
        let (la, lb) = (luminance(a), luminance(b));
        let (hi, lo) = if la > lb { (la, lb) } else { (lb, la) };
        (hi + 0.05) / (lo + 0.05)
    }

    #[test]
    fn text_is_readable_on_every_surface_in_both_themes() {
        for p in [Palette::light(), Palette::dark()] {
            for surface in [p.bg, p.card, p.hover] {
                assert!(contrast(p.text, surface) >= 7.0, "texto principal");
                assert!(contrast(p.muted, surface) >= 4.5, "texto secundário");
            }
            assert!(
                contrast(p.accent_text, p.card) >= 4.5,
                "link/destaque em texto"
            );
            assert!(
                contrast(Color32::WHITE, p.accent) >= 3.0,
                "texto no botão primário"
            );
            assert!(
                contrast(Color32::WHITE, p.danger) >= 4.5,
                "texto no botão de perigo"
            );
            for (fg, bg) in [
                (p.ok_fg, p.ok_bg),
                (p.warn_fg, p.warn_bg),
                (p.err_fg, p.err_bg),
                (p.info_fg, p.info_bg),
            ] {
                assert!(
                    contrast(fg, bg) >= 4.5,
                    "texto semântico sobre o fundo suave"
                );
            }
        }
    }

    #[test]
    fn dark_theme_is_neutral_black_not_blue() {
        let p = Palette::dark();
        for c in [p.bg, p.card, p.border, p.hover, p.track] {
            let (r, g, b) = (c.r() as i16, c.g() as i16, c.b() as i16);
            assert!(
                (r - b).abs() <= 2 && (g - b).abs() <= 2,
                "{c:?} tem tom de cor"
            );
            assert!(r < 0x30, "{c:?} não é preto");
        }
    }

    #[test]
    fn light_and_dark_are_opposites() {
        assert!(luminance(Palette::light().bg) > 0.9);
        assert!(luminance(Palette::dark().bg) < 0.01);
    }
}
