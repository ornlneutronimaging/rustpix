//! Open-file conveniences shared with the other VENUS Rust tools (ported
//! from `rust_nexus_viewer`): a browse dialog that starts in the last used
//! directory, a type/paste-a-path modal, and a persisted recent-files menu.

use std::path::{Path, PathBuf};

use eframe::egui;

use crate::app::RustpixApp;
use crate::recent;

/// State for the open-file tools (recent list and "Open path" popup).
pub(crate) struct FileOpenState {
    /// Most-recently opened files, newest first (persisted across restarts).
    pub(crate) recent: Vec<PathBuf>,
    /// "Open path" popup: visible, typed text, focus request, last error.
    pub(crate) path_popup: bool,
    pub(crate) path_input: String,
    pub(crate) path_focus: bool,
    pub(crate) path_error: Option<String>,
}

impl Default for FileOpenState {
    fn default() -> Self {
        Self {
            recent: recent::load(),
            path_popup: false,
            path_input: String::new(),
            path_focus: false,
            path_error: None,
        }
    }
}

/// `…/IPTS-####/shared` for a path inside an experiment folder (the IPTS
/// number is taken from the innermost `IPTS-<digits>` path component).
fn ipts_shared_dir(path: &Path) -> Option<PathBuf> {
    path.ancestors()
        .find(|dir| {
            dir.file_name()
                .and_then(|name| name.to_str())
                .and_then(|name| name.strip_prefix("IPTS-"))
                .is_some_and(|num| !num.is_empty() && num.bytes().all(|b| b.is_ascii_digit()))
        })
        .map(|dir| dir.join("shared"))
}

impl RustpixApp {
    /// Directory to start browsing in: the current file's, else the most
    /// recently opened one's.
    pub(crate) fn last_used_dir(&self) -> Option<&Path> {
        self.selected_file
            .as_deref()
            .or_else(|| self.file_open.recent.first().map(PathBuf::as_path))
            .and_then(Path::parent)
    }

    /// Directory to start export dialogs in: the `shared/` folder of the
    /// loaded file's experiment (`…/IPTS-####/shared`), else the last used
    /// directory.
    pub(crate) fn export_start_dir(&self) -> Option<PathBuf> {
        self.selected_file
            .as_deref()
            .and_then(ipts_shared_dir)
            .filter(|dir| dir.is_dir())
            .or_else(|| self.last_used_dir().map(Path::to_path_buf))
    }

    /// Save/pick-folder dialog starting in [`Self::export_start_dir`].
    pub(crate) fn export_dialog(&self) -> rfd::FileDialog {
        let dialog = rfd::FileDialog::new();
        match self.export_start_dir() {
            Some(dir) => dialog.set_directory(dir),
            None => dialog,
        }
    }

    /// Browse for a TPX3 or SNS `NeXus` file, starting in `start` (or the
    /// last used directory).
    pub(crate) fn open_dialog(&mut self, start: Option<&Path>) {
        let mut dialog = rfd::FileDialog::new()
            .add_filter("All supported", &["tpx3", "h5", "hdf5", "nxs"])
            .add_filter("TPX3", &["tpx3"])
            .add_filter("NeXus (SNS)", &["h5", "hdf5", "nxs"])
            .add_filter("All files", &["*"]);
        if let Some(dir) = start.or_else(|| self.last_used_dir()) {
            dialog = dialog.set_directory(dir);
        }
        if let Some(path) = dialog.pick_file() {
            self.load_file(path);
        }
    }

    /// Open what is typed in the path popup: a file directly, a directory as
    /// the starting point of the browse dialog.
    fn open_typed_path(&mut self) {
        let mut typed = self.file_open.path_input.trim().to_owned();
        if let Some(rest) = typed.strip_prefix("~/") {
            if let Ok(home) = std::env::var("HOME") {
                typed = format!("{home}/{rest}");
            }
        }
        if typed.is_empty() {
            return;
        }
        let path = PathBuf::from(&typed);
        if path.is_file() {
            self.file_open.path_popup = false;
            self.file_open.path_error = None;
            self.load_file(path);
        } else if path.is_dir() {
            self.file_open.path_popup = false;
            self.file_open.path_error = None;
            self.open_dialog(Some(&path));
        } else {
            self.file_open.path_error = Some(format!("Not found: {}", path.display()));
        }
    }

    /// Open the path popup, pre-filled with the last used directory.
    pub(crate) fn show_path_popup_request(&mut self) {
        if self.file_open.path_input.is_empty() {
            if let Some(dir) = self.last_used_dir() {
                let dir = dir.display().to_string();
                self.file_open.path_input = dir;
            }
        }
        self.file_open.path_popup = true;
        self.file_open.path_focus = true;
        self.file_open.path_error = None;
    }

    /// Modal with a text field to type/paste a path instead of browsing.
    pub(crate) fn show_path_popup(&mut self, ctx: &egui::Context) {
        if !self.file_open.path_popup {
            return;
        }
        let modal = egui::Modal::new(egui::Id::new("open_path")).show(ctx, |ui| {
            ui.set_width(620.0);
            ui.heading("Open path");
            ui.label("File path opens it directly; directory path starts the browser there.");
            ui.add_space(6.0);
            let resp = ui.add(
                egui::TextEdit::singleline(&mut self.file_open.path_input)
                    .hint_text("/SNS/VENUS/IPTS-xxxxx/…")
                    .desired_width(f32::INFINITY),
            );
            if self.file_open.path_focus {
                resp.request_focus();
                self.file_open.path_focus = false;
            }
            if resp.changed() {
                self.file_open.path_error = None;
            }
            if let Some(err) = &self.file_open.path_error {
                ui.colored_label(ui.visuals().error_fg_color, err);
            }
            ui.add_space(6.0);
            let entered = resp.lost_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter));
            ui.horizontal(|ui| {
                if ui.button("Open").clicked() || entered {
                    self.open_typed_path();
                }
                if ui.button("Cancel").clicked() {
                    self.file_open.path_popup = false;
                    self.file_open.path_error = None;
                }
            });
        });
        if modal.should_close() {
            self.file_open.path_popup = false;
            self.file_open.path_error = None;
        }
    }

    /// Recent-files menu button. Returns without drawing when the list is
    /// empty (the button is still shown, disabled).
    pub(crate) fn render_recent_menu(&mut self, ui: &mut egui::Ui, can_load: bool) {
        let mut reopen: Option<PathBuf> = None;
        let mut clear_recent = false;
        let enabled = can_load && !self.file_open.recent.is_empty();
        ui.add_enabled_ui(enabled, |ui| {
            ui.menu_button(egui::RichText::new("🕘").size(14.0), |ui| {
                for p in &self.file_open.recent {
                    let name = p.file_name().map_or_else(
                        || p.display().to_string(),
                        |s| s.to_string_lossy().into_owned(),
                    );
                    let exists = p.exists();
                    let resp = ui
                        .add_enabled(exists, egui::Button::new(name))
                        .on_hover_text(p.display().to_string())
                        .on_disabled_hover_text(format!("File not found: {}", p.display()));
                    if resp.clicked() {
                        reopen = Some(p.clone());
                        ui.close();
                    }
                }
                ui.separator();
                if ui.button("Clear list").clicked() {
                    clear_recent = true;
                    ui.close();
                }
            })
            .response
            .on_hover_text("Reopen one of the last files");
        });
        if let Some(p) = reopen {
            self.load_file(p);
        }
        if clear_recent {
            recent::clear(&mut self.file_open.recent);
        }
    }

    /// Open a `.tpx3` or SNS `NeXus` file dropped onto the window.
    pub(crate) fn handle_dropped_file(&mut self, ctx: &egui::Context) {
        let dropped: Option<PathBuf> =
            ctx.input(|i| i.raw.dropped_files.first().and_then(|f| f.path.clone()));
        let Some(path) = dropped else { return };
        let can_load = !self.processing.is_loading && !self.processing.is_processing;
        let supported = path
            .extension()
            .is_some_and(|e| e.eq_ignore_ascii_case("tpx3"))
            || crate::pipeline::is_sns_nexus_path(&path);
        if can_load && supported {
            self.load_file(path);
        } else if !supported {
            log::warn!(
                "ignoring dropped file (not .tpx3/.h5/.nxs): {}",
                path.display()
            );
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ipts_shared_dir_from_experiment_path() {
        let path = Path::new("/SNS/VENUS/IPTS-36967/images/tpx3/Run_1234/file.tpx3");
        assert_eq!(
            ipts_shared_dir(path),
            Some(PathBuf::from("/SNS/VENUS/IPTS-36967/shared"))
        );
    }

    #[test]
    fn ipts_shared_dir_none_outside_experiment() {
        assert_eq!(ipts_shared_dir(Path::new("/tmp/IPTS-abc/file.tpx3")), None);
        assert_eq!(ipts_shared_dir(Path::new("/home/user/file.tpx3")), None);
    }
}
