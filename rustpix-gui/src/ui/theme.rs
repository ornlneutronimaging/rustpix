//! Application theme and color definitions.
//!
//! Provides light and dark themes with monospace fonts, following system preference.

use eframe::egui::{
    self, Color32, FontFamily, FontId, Rounding, Stroke, TextStyle, Theme, Visuals,
};

/// Color palette for the application (dark theme).
pub mod dark {
    use eframe::egui::Color32;

    // Base colors
    pub const BG_DARK: Color32 = Color32::from_rgb(0x1a, 0x1a, 0x1a);
    pub const BG_PANEL: Color32 = Color32::from_rgb(0x1f, 0x1f, 0x1f);
    pub const BG_HEADER: Color32 = Color32::from_rgb(0x25, 0x25, 0x25);
    pub const BG_INPUT: Color32 = Color32::from_rgb(0x2a, 0x2a, 0x2a);

    // Border colors
    pub const BORDER: Color32 = Color32::from_rgb(0x33, 0x33, 0x33);
    pub const BORDER_LIGHT: Color32 = Color32::from_rgb(0x44, 0x44, 0x44);

    // Text colors
    pub const TEXT_PRIMARY: Color32 = Color32::from_rgb(0xe0, 0xe0, 0xe0);
    pub const TEXT_MUTED: Color32 = Color32::from_rgb(0x88, 0x88, 0x88);
    pub const TEXT_DIM: Color32 = Color32::from_rgb(0x66, 0x66, 0x66);

    // Button colors
    pub const BUTTON_BG: Color32 = Color32::from_rgb(0x33, 0x33, 0x33);
    pub const BUTTON_HOVER: Color32 = Color32::from_rgb(0x3a, 0x3a, 0x3a);
}

/// Color palette for the application (light theme).
#[allow(dead_code)]
pub mod light {
    use eframe::egui::Color32;

    // Base colors
    pub const BG_DARK: Color32 = Color32::from_rgb(0xf5, 0xf5, 0xf5);
    pub const BG_PANEL: Color32 = Color32::from_rgb(0xff, 0xff, 0xff);
    pub const BG_HEADER: Color32 = Color32::from_rgb(0xfa, 0xfa, 0xfa);
    pub const BG_INPUT: Color32 = Color32::from_rgb(0xf0, 0xf0, 0xf0);

    // Border colors
    pub const BORDER: Color32 = Color32::from_rgb(0xd0, 0xd0, 0xd0);
    pub const BORDER_LIGHT: Color32 = Color32::from_rgb(0xc0, 0xc0, 0xc0);

    // Text colors
    pub const TEXT_PRIMARY: Color32 = Color32::from_rgb(0x1a, 0x1a, 0x1a);
    pub const TEXT_MUTED: Color32 = Color32::from_rgb(0x66, 0x66, 0x66);
    pub const TEXT_DIM: Color32 = Color32::from_rgb(0x88, 0x88, 0x88);

    // Button colors
    pub const BUTTON_BG: Color32 = Color32::from_rgb(0xe8, 0xe8, 0xe8);
    pub const BUTTON_HOVER: Color32 = Color32::from_rgb(0xdd, 0xdd, 0xdd);
}

/// Shared accent colors (same for both themes).
pub mod accent {
    use eframe::egui::Color32;

    pub const BLUE: Color32 = Color32::from_rgb(0x4a, 0x9e, 0xff);
    pub const GREEN: Color32 = Color32::from_rgb(0x10, 0xb9, 0x81);
    pub const RED: Color32 = Color32::from_rgb(0xef, 0x44, 0x44);
}

/// Re-export colors for backward compatibility (dark theme defaults).
#[allow(unused_imports)]
pub mod colors {
    pub use super::accent::BLUE as ACCENT_BLUE;
    pub use super::accent::GREEN as ACCENT_GREEN;
    pub use super::accent::RED as ACCENT_RED;
    pub use super::dark::*;
}

/// Theme-aware color accessor.
/// Use this to get colors that adapt to the current light/dark mode.
#[derive(Clone, Copy)]
#[allow(dead_code)]
pub struct ThemeColors {
    pub bg_dark: Color32,
    pub bg_panel: Color32,
    pub bg_header: Color32,
    pub bg_input: Color32,
    pub border: Color32,
    pub border_light: Color32,
    pub text_primary: Color32,
    pub text_muted: Color32,
    pub text_dim: Color32,
    pub button_bg: Color32,
    pub button_hover: Color32,
}

impl ThemeColors {
    /// Get colors for the current theme from context.
    pub fn from_ctx(ctx: &egui::Context) -> Self {
        Self::from_dark_mode(ctx.style().visuals.dark_mode)
    }

    /// Get colors for the current theme from UI.
    pub fn from_ui(ui: &egui::Ui) -> Self {
        Self::from_dark_mode(ui.visuals().dark_mode)
    }

    /// Get colors based on dark mode flag.
    pub fn from_dark_mode(is_dark: bool) -> Self {
        if is_dark {
            Self {
                bg_dark: dark::BG_DARK,
                bg_panel: dark::BG_PANEL,
                bg_header: dark::BG_HEADER,
                bg_input: dark::BG_INPUT,
                border: dark::BORDER,
                border_light: dark::BORDER_LIGHT,
                text_primary: dark::TEXT_PRIMARY,
                text_muted: dark::TEXT_MUTED,
                text_dim: dark::TEXT_DIM,
                button_bg: dark::BUTTON_BG,
                button_hover: dark::BUTTON_HOVER,
            }
        } else {
            Self {
                bg_dark: light::BG_DARK,
                bg_panel: light::BG_PANEL,
                bg_header: light::BG_HEADER,
                bg_input: light::BG_INPUT,
                border: light::BORDER,
                border_light: light::BORDER_LIGHT,
                text_primary: light::TEXT_PRIMARY,
                text_muted: light::TEXT_MUTED,
                text_dim: light::TEXT_DIM,
                button_bg: light::BUTTON_BG,
                button_hover: light::BUTTON_HOVER,
            }
        }
    }
}

/// Configure egui style for the given theme.
pub fn configure_style_for_theme(ctx: &egui::Context, theme: Theme) {
    let visuals = match theme {
        Theme::Dark => build_dark_visuals(),
        Theme::Light => build_light_visuals(),
    };

    ctx.set_visuals(visuals);
    configure_fonts_and_spacing(ctx);
}

/// Configure style based on current visuals (dark/light mode).
pub fn configure_style(ctx: &egui::Context) {
    let is_dark = ctx.style().visuals.dark_mode;
    let theme = if is_dark { Theme::Dark } else { Theme::Light };
    configure_style_for_theme(ctx, theme);
}

/// Build dark theme visuals.
fn build_dark_visuals() -> Visuals {
    let mut visuals = Visuals::dark();

    visuals.window_fill = dark::BG_PANEL;
    visuals.panel_fill = dark::BG_PANEL;
    visuals.faint_bg_color = dark::BG_DARK;
    visuals.extreme_bg_color = dark::BG_INPUT;

    visuals.widgets.noninteractive.bg_fill = dark::BG_INPUT;
    visuals.widgets.noninteractive.fg_stroke = Stroke::new(1.0, dark::TEXT_MUTED);
    visuals.widgets.noninteractive.bg_stroke = Stroke::new(1.0, dark::BORDER);
    visuals.widgets.noninteractive.rounding = Rounding::same(4.0);

    visuals.widgets.inactive.bg_fill = dark::BG_INPUT;
    visuals.widgets.inactive.fg_stroke = Stroke::new(1.0, dark::TEXT_PRIMARY);
    visuals.widgets.inactive.bg_stroke = Stroke::new(1.0, dark::BORDER_LIGHT);
    visuals.widgets.inactive.rounding = Rounding::same(4.0);

    visuals.widgets.hovered.bg_fill = dark::BUTTON_HOVER;
    visuals.widgets.hovered.fg_stroke = Stroke::new(1.0, dark::TEXT_PRIMARY);
    visuals.widgets.hovered.bg_stroke = Stroke::new(1.0, accent::BLUE);
    visuals.widgets.hovered.rounding = Rounding::same(4.0);

    visuals.widgets.active.bg_fill = accent::BLUE;
    visuals.widgets.active.fg_stroke = Stroke::new(1.0, Color32::WHITE);
    visuals.widgets.active.bg_stroke = Stroke::new(1.0, accent::BLUE);
    visuals.widgets.active.rounding = Rounding::same(4.0);

    visuals.widgets.open.bg_fill = dark::BG_INPUT;
    visuals.widgets.open.fg_stroke = Stroke::new(1.0, dark::TEXT_PRIMARY);
    visuals.widgets.open.bg_stroke = Stroke::new(1.0, dark::BORDER_LIGHT);
    visuals.widgets.open.rounding = Rounding::same(4.0);

    visuals.selection.bg_fill = accent::BLUE.gamma_multiply(0.3);
    visuals.selection.stroke = Stroke::new(1.0, accent::BLUE);

    visuals
}

/// Build light theme visuals.
fn build_light_visuals() -> Visuals {
    let mut visuals = Visuals::light();

    visuals.window_fill = light::BG_PANEL;
    visuals.panel_fill = light::BG_PANEL;
    visuals.faint_bg_color = light::BG_DARK;
    visuals.extreme_bg_color = light::BG_INPUT;

    visuals.widgets.noninteractive.bg_fill = light::BG_INPUT;
    visuals.widgets.noninteractive.fg_stroke = Stroke::new(1.0, light::TEXT_MUTED);
    visuals.widgets.noninteractive.bg_stroke = Stroke::new(1.0, light::BORDER);
    visuals.widgets.noninteractive.rounding = Rounding::same(4.0);

    visuals.widgets.inactive.bg_fill = light::BG_INPUT;
    visuals.widgets.inactive.fg_stroke = Stroke::new(1.0, light::TEXT_PRIMARY);
    visuals.widgets.inactive.bg_stroke = Stroke::new(1.0, light::BORDER_LIGHT);
    visuals.widgets.inactive.rounding = Rounding::same(4.0);

    visuals.widgets.hovered.bg_fill = light::BUTTON_HOVER;
    visuals.widgets.hovered.fg_stroke = Stroke::new(1.0, light::TEXT_PRIMARY);
    visuals.widgets.hovered.bg_stroke = Stroke::new(1.0, accent::BLUE);
    visuals.widgets.hovered.rounding = Rounding::same(4.0);

    visuals.widgets.active.bg_fill = accent::BLUE;
    visuals.widgets.active.fg_stroke = Stroke::new(1.0, Color32::WHITE);
    visuals.widgets.active.bg_stroke = Stroke::new(1.0, accent::BLUE);
    visuals.widgets.active.rounding = Rounding::same(4.0);

    visuals.widgets.open.bg_fill = light::BG_INPUT;
    visuals.widgets.open.fg_stroke = Stroke::new(1.0, light::TEXT_PRIMARY);
    visuals.widgets.open.bg_stroke = Stroke::new(1.0, light::BORDER_LIGHT);
    visuals.widgets.open.rounding = Rounding::same(4.0);

    visuals.selection.bg_fill = accent::BLUE.gamma_multiply(0.2);
    visuals.selection.stroke = Stroke::new(1.0, accent::BLUE);

    visuals
}

/// Configure fonts and spacing (theme-independent).
fn configure_fonts_and_spacing(ctx: &egui::Context) {
    let mut style = (*ctx.style()).clone();

    // Use monospace for everything
    style.text_styles = [
        (TextStyle::Small, FontId::new(10.0, FontFamily::Monospace)),
        (TextStyle::Body, FontId::new(12.0, FontFamily::Monospace)),
        (TextStyle::Button, FontId::new(12.0, FontFamily::Monospace)),
        (TextStyle::Heading, FontId::new(14.0, FontFamily::Monospace)),
        (
            TextStyle::Monospace,
            FontId::new(12.0, FontFamily::Monospace),
        ),
    ]
    .into();

    // Spacing
    style.spacing.item_spacing = egui::vec2(8.0, 6.0);
    style.spacing.button_padding = egui::vec2(10.0, 6.0);
    style.spacing.indent = 16.0;

    ctx.set_style(style);
}

/// Style a button as the primary action button.
pub fn primary_button(text: &str) -> egui::Button<'_> {
    egui::Button::new(egui::RichText::new(text).color(Color32::WHITE))
        .fill(accent::GREEN)
        .rounding(Rounding::same(4.0))
}

/// Style a button as a secondary/ghost button (theme-aware).
#[allow(dead_code)]
pub fn secondary_button_themed<'a>(ctx: &egui::Context, text: &'a str) -> egui::Button<'a> {
    let is_dark = ctx.style().visuals.dark_mode;
    let (text_color, bg, border) = if is_dark {
        (dark::TEXT_PRIMARY, dark::BUTTON_BG, dark::BORDER_LIGHT)
    } else {
        (light::TEXT_PRIMARY, light::BUTTON_BG, light::BORDER_LIGHT)
    };
    egui::Button::new(egui::RichText::new(text).color(text_color))
        .fill(bg)
        .stroke(Stroke::new(1.0, border))
        .rounding(Rounding::same(4.0))
}

/// Style a button as a secondary/ghost button (dark theme, for compatibility).
#[allow(dead_code)]
pub fn secondary_button(text: &str) -> egui::Button<'_> {
    egui::Button::new(egui::RichText::new(text).color(dark::TEXT_PRIMARY))
        .fill(dark::BUTTON_BG)
        .stroke(Stroke::new(1.0, dark::BORDER_LIGHT))
        .rounding(Rounding::same(4.0))
}

/// Create a section header label.
#[allow(dead_code)]
pub fn section_header(text: &str) -> egui::RichText {
    egui::RichText::new(text.to_uppercase()).size(11.0).strong()
}

/// Create a form label.
pub fn form_label(text: &str) -> egui::RichText {
    egui::RichText::new(text.to_uppercase()).size(10.0)
}

/// Create a stat label (left column).
pub fn stat_label(text: &str) -> egui::RichText {
    egui::RichText::new(text).size(11.0).weak()
}

/// Create a stat value (right column).
pub fn stat_value(text: &str) -> egui::RichText {
    egui::RichText::new(text).size(11.0)
}

/// Create a highlighted stat value (e.g., neutron count).
pub fn stat_value_highlight(text: &str) -> egui::RichText {
    egui::RichText::new(text)
        .size(11.0)
        .color(accent::GREEN)
        .strong()
}

/// Track the last applied theme to detect system theme changes.
static LAST_DARK_MODE: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(true);
static THEME_INITIALIZED: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);

/// Apply the current system theme, re-applying styles if the theme changed.
/// Call this in the update loop to follow system theme changes.
pub fn apply_system_theme(ctx: &egui::Context) {
    use std::sync::atomic::Ordering;

    let is_dark = ctx.style().visuals.dark_mode;
    let was_initialized = THEME_INITIALIZED.swap(true, Ordering::Relaxed);
    let last_dark = LAST_DARK_MODE.swap(is_dark, Ordering::Relaxed);

    // Re-apply custom styling if theme changed or on first run
    if !was_initialized || last_dark != is_dark {
        let theme = if is_dark { Theme::Dark } else { Theme::Light };
        configure_style_for_theme(ctx, theme);
    }
}
