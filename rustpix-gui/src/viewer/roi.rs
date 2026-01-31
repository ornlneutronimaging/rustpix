//! ROI data structures and rendering helpers.

use eframe::egui::{Align2, Color32, Stroke};
use egui_plot::{PlotBounds, PlotPoint, PlotUi, Polygon, Text};

/// ROI selection mode.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
#[allow(dead_code)]
pub enum RoiSelectionMode {
    #[default]
    Rectangle,
    Polygon,
}

/// Region of interest definition.
#[derive(Debug, Clone)]
#[allow(dead_code)]
pub struct Roi {
    pub id: usize,
    pub name: String,
    pub color: Color32,
    pub shape: RoiShape,
    pub visible: bool,
    pub selected: bool,
    pub edit_mode: bool,
}

/// ROI shape variants.
#[derive(Debug, Clone)]
#[allow(dead_code)]
pub enum RoiShape {
    Rectangle { x1: f64, y1: f64, x2: f64, y2: f64 },
    Polygon { vertices: Vec<(f64, f64)> },
}

/// In-progress ROI drawing state.
#[derive(Debug, Clone)]
pub struct RoiDraft {
    pub start: PlotPoint,
    pub current: PlotPoint,
}

#[derive(Debug, Clone)]
pub struct RoiDrag {
    pub roi_id: usize,
    pub last: PlotPoint,
    pub bounds: PlotBounds,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RoiHandle {
    North,
    South,
    East,
    West,
    NorthEast,
    NorthWest,
    SouthEast,
    SouthWest,
}

#[derive(Debug, Clone)]
pub struct RoiEditDrag {
    pub roi_id: usize,
    pub handle: RoiHandle,
    pub last: PlotPoint,
    pub bounds: PlotBounds,
}

/// ROI session state.
#[derive(Debug)]
pub struct RoiState {
    pub mode: RoiSelectionMode,
    pub rois: Vec<Roi>,
    pub draft: Option<RoiDraft>,
    pub debounce_updates: bool,
    drag: Option<RoiDrag>,
    edit_drag: Option<RoiEditDrag>,
    context_menu: Option<usize>,
    next_id: usize,
}

impl Default for RoiState {
    fn default() -> Self {
        Self {
            mode: RoiSelectionMode::default(),
            rois: Vec::new(),
            draft: None,
            debounce_updates: false,
            drag: None,
            edit_drag: None,
            context_menu: None,
            next_id: 1,
        }
    }
}

impl RoiState {
    /// Clear all ROIs and reset numbering.
    pub fn clear(&mut self) {
        self.rois.clear();
        self.draft = None;
        self.drag = None;
        self.edit_drag = None;
        self.context_menu = None;
        self.next_id = 1;
    }

    /// Delete the currently selected ROI.
    pub fn delete_selected(&mut self) -> bool {
        let Some(selected_id) = self.rois.iter().find(|roi| roi.selected).map(|roi| roi.id) else {
            return false;
        };
        self.rois.retain(|roi| roi.id != selected_id);
        self.draft = None;
        self.drag = None;
        self.edit_drag = None;
        self.context_menu = None;
        true
    }

    /// Delete a ROI by id.
    pub fn delete_id(&mut self, roi_id: usize) -> bool {
        let before = self.rois.len();
        self.rois.retain(|roi| roi.id != roi_id);
        if self.rois.len() == before {
            return false;
        }
        self.draft = None;
        self.drag = None;
        self.edit_drag = None;
        self.context_menu = None;
        true
    }

    /// Set edit mode for a ROI.
    pub fn set_edit_mode(&mut self, roi_id: usize, enabled: bool) {
        for roi in &mut self.rois {
            if roi.id == roi_id {
                roi.edit_mode = enabled;
                roi.selected = true;
            } else if enabled {
                roi.edit_mode = false;
            }
        }
    }

    /// Clear edit mode for all ROIs.
    pub fn clear_edit_mode(&mut self) {
        for roi in &mut self.rois {
            roi.edit_mode = false;
        }
        self.edit_drag = None;
    }

    /// Begin drawing a rectangle ROI.
    pub fn begin_rectangle(&mut self, start: PlotPoint) {
        self.draft = Some(RoiDraft {
            start,
            current: start,
        });
    }

    /// Update rectangle draft while dragging.
    pub fn update_rectangle(&mut self, current: PlotPoint) {
        if let Some(draft) = &mut self.draft {
            draft.current = current;
        }
    }

    /// Commit the rectangle draft as a new ROI.
    pub fn commit_rectangle(&mut self, min_size: f64) {
        let Some(draft) = self.draft.take() else {
            return;
        };

        let (min_x, max_x) = if draft.start.x <= draft.current.x {
            (draft.start.x, draft.current.x)
        } else {
            (draft.current.x, draft.start.x)
        };
        let (min_y, max_y) = if draft.start.y <= draft.current.y {
            (draft.start.y, draft.current.y)
        } else {
            (draft.current.y, draft.start.y)
        };

        if (max_x - min_x) < min_size || (max_y - min_y) < min_size {
            return;
        }

        let id = self.next_id.max(1);
        self.next_id = id + 1;
        let color = roi_palette_color(id - 1);
        let roi = Roi {
            id,
            name: format!("ROI {id}"),
            color,
            shape: RoiShape::Rectangle {
                x1: min_x,
                y1: min_y,
                x2: max_x,
                y2: max_y,
            },
            visible: true,
            selected: false,
            edit_mode: false,
        };

        self.rois.push(roi);
        self.set_selected(Some(id));
    }

    /// Select the topmost ROI containing the point.
    pub fn select_at(&mut self, point: PlotPoint) {
        if let Some(hit_id) = self.hit_test(point) {
            self.set_selected(Some(hit_id));
        }
    }

    /// Return the topmost ROI id containing the point.
    pub fn hit_test(&self, point: PlotPoint) -> Option<usize> {
        for roi in self.rois.iter().rev() {
            if roi.contains(point) {
                return Some(roi.id);
            }
        }
        None
    }

    /// Begin dragging a ROI.
    pub fn start_drag(&mut self, roi_id: usize, start: PlotPoint, bounds: PlotBounds) {
        self.set_selected(Some(roi_id));
        self.drag = Some(RoiDrag {
            roi_id,
            last: start,
            bounds,
        });
    }

    /// Update drag with the new pointer position.
    pub fn update_drag(&mut self, current: PlotPoint) {
        let Some(drag) = &mut self.drag else {
            return;
        };
        let dx = current.x - drag.last.x;
        let dy = current.y - drag.last.y;
        if dx == 0.0 && dy == 0.0 {
            return;
        }
        if let Some(roi) = self.rois.iter_mut().find(|roi| roi.id == drag.roi_id) {
            roi.translate(dx, dy);
        }
        drag.last = current;
    }

    /// End ROI drag.
    pub fn end_drag(&mut self) {
        self.drag = None;
    }

    /// Whether a drag is in progress.
    pub fn is_dragging(&self) -> bool {
        self.drag.is_some()
    }

    /// Whether an edit drag is in progress.
    pub fn is_edit_dragging(&self) -> bool {
        self.edit_drag.is_some()
    }

    /// Bounds captured at drag start (used to freeze pan).
    pub fn drag_bounds(&self) -> Option<PlotBounds> {
        self.drag.as_ref().map(|drag| drag.bounds)
    }

    /// Start edit drag (resize handles).
    pub fn start_edit_drag(
        &mut self,
        roi_id: usize,
        handle: RoiHandle,
        start: PlotPoint,
        bounds: PlotBounds,
    ) {
        self.set_edit_mode(roi_id, true);
        self.edit_drag = Some(RoiEditDrag {
            roi_id,
            handle,
            last: start,
            bounds,
        });
    }

    /// Update edit drag.
    pub fn update_edit_drag(&mut self, current: PlotPoint, min_size: f64) {
        let Some(edit) = &mut self.edit_drag else {
            return;
        };
        let dx = current.x - edit.last.x;
        let dy = current.y - edit.last.y;
        if dx == 0.0 && dy == 0.0 {
            return;
        }
        if let Some(roi) = self.rois.iter_mut().find(|roi| roi.id == edit.roi_id) {
            roi.resize(edit.handle, dx, dy, min_size);
        }
        edit.last = current;
    }

    /// End edit drag.
    pub fn end_edit_drag(&mut self) {
        self.edit_drag = None;
    }

    /// Bounds captured at edit drag start (used to freeze pan).
    pub fn edit_drag_bounds(&self) -> Option<PlotBounds> {
        self.edit_drag.as_ref().map(|drag| drag.bounds)
    }

    /// Set the context menu target.
    pub fn set_context_menu(&mut self, roi_id: Option<usize>) {
        self.context_menu = roi_id;
    }

    /// Current context menu target.
    pub fn context_menu_target(&self) -> Option<usize> {
        self.context_menu
    }

    /// Find resize handle hit for selected ROI in edit mode.
    pub fn hit_test_handle(&self, point: PlotPoint, threshold: f64) -> Option<(usize, RoiHandle)> {
        for roi in self.rois.iter().rev() {
            if !roi.edit_mode {
                continue;
            }
            if let Some(handle) = roi.handle_hit(point, threshold) {
                return Some((roi.id, handle));
            }
        }
        None
    }

    /// Render all ROIs to the plot.
    pub fn draw(&self, plot_ui: &mut PlotUi) {
        for roi in &self.rois {
            if !roi.visible {
                continue;
            }
            let stroke_width = if roi.selected { 2.0 } else { 1.0 };
            let stroke = Stroke::new(stroke_width, roi.color);
            let fill = roi_fill_color(roi.color);
            let points = roi.plot_points();
            plot_ui.polygon(Polygon::new(points).stroke(stroke).fill_color(fill));

            let label_pos = roi.label_position();
            plot_ui.text(
                Text::new(label_pos, roi.name.clone())
                    .color(roi.color)
                    .anchor(Align2::LEFT_TOP),
            );
        }
    }

    /// Render the draft ROI while dragging.
    pub fn draw_draft(&self, plot_ui: &mut PlotUi) {
        let Some(draft) = &self.draft else {
            return;
        };
        let color = roi_palette_color(self.next_id.saturating_sub(1));
        let stroke = Stroke::new(1.0, color);
        let fill = roi_fill_color(color);
        let points = draft_plot_points(draft);
        plot_ui.polygon(Polygon::new(points).stroke(stroke).fill_color(fill));
    }

    fn set_selected(&mut self, id: Option<usize>) {
        for roi in &mut self.rois {
            roi.selected = Some(roi.id) == id;
        }
    }

    /// Select a ROI by id.
    pub fn select_id(&mut self, roi_id: usize) {
        self.set_selected(Some(roi_id));
    }
}

impl Roi {
    fn plot_points(&self) -> Vec<[f64; 2]> {
        match &self.shape {
            RoiShape::Rectangle { x1, y1, x2, y2 } => {
                vec![[*x1, *y1], [*x2, *y1], [*x2, *y2], [*x1, *y2]]
            }
            RoiShape::Polygon { vertices } => {
                vertices.iter().map(|(x, y)| [*x, *y]).collect::<Vec<_>>()
            }
        }
    }

    fn label_position(&self) -> PlotPoint {
        let (min_x, _max_x, _min_y, max_y) = self.bounds();
        PlotPoint::new(min_x + 2.0, max_y - 2.0)
    }

    fn bounds(&self) -> (f64, f64, f64, f64) {
        match &self.shape {
            RoiShape::Rectangle { x1, y1, x2, y2 } => {
                let min_x = x1.min(*x2);
                let max_x = x1.max(*x2);
                let min_y = y1.min(*y2);
                let max_y = y1.max(*y2);
                (min_x, max_x, min_y, max_y)
            }
            RoiShape::Polygon { vertices } => {
                let mut min_x = f64::INFINITY;
                let mut max_x = f64::NEG_INFINITY;
                let mut min_y = f64::INFINITY;
                let mut max_y = f64::NEG_INFINITY;
                for (x, y) in vertices {
                    min_x = min_x.min(*x);
                    max_x = max_x.max(*x);
                    min_y = min_y.min(*y);
                    max_y = max_y.max(*y);
                }
                if !min_x.is_finite() || !min_y.is_finite() {
                    (0.0, 0.0, 0.0, 0.0)
                } else {
                    (min_x, max_x, min_y, max_y)
                }
            }
        }
    }

    fn contains(&self, point: PlotPoint) -> bool {
        match &self.shape {
            RoiShape::Rectangle { x1, y1, x2, y2 } => {
                let min_x = x1.min(*x2);
                let max_x = x1.max(*x2);
                let min_y = y1.min(*y2);
                let max_y = y1.max(*y2);
                point.x >= min_x && point.x <= max_x && point.y >= min_y && point.y <= max_y
            }
            RoiShape::Polygon { vertices } => point_in_polygon(point, vertices),
        }
    }

    fn translate(&mut self, dx: f64, dy: f64) {
        match &mut self.shape {
            RoiShape::Rectangle { x1, y1, x2, y2 } => {
                *x1 += dx;
                *x2 += dx;
                *y1 += dy;
                *y2 += dy;
            }
            RoiShape::Polygon { vertices } => {
                for (x, y) in vertices {
                    *x += dx;
                    *y += dy;
                }
            }
        }
    }

    fn handle_hit(&self, point: PlotPoint, threshold: f64) -> Option<RoiHandle> {
        let RoiShape::Rectangle { x1, y1, x2, y2 } = self.shape else {
            return None;
        };
        let min_x = x1.min(x2);
        let max_x = x1.max(x2);
        let min_y = y1.min(y2);
        let max_y = y1.max(y2);

        let near_left = (point.x - min_x).abs() <= threshold;
        let near_right = (point.x - max_x).abs() <= threshold;
        let near_bottom = (point.y - min_y).abs() <= threshold;
        let near_top = (point.y - max_y).abs() <= threshold;

        if near_left && near_bottom {
            Some(RoiHandle::SouthWest)
        } else if near_left && near_top {
            Some(RoiHandle::NorthWest)
        } else if near_right && near_bottom {
            Some(RoiHandle::SouthEast)
        } else if near_right && near_top {
            Some(RoiHandle::NorthEast)
        } else if near_left && point.y >= min_y && point.y <= max_y {
            Some(RoiHandle::West)
        } else if near_right && point.y >= min_y && point.y <= max_y {
            Some(RoiHandle::East)
        } else if near_bottom && point.x >= min_x && point.x <= max_x {
            Some(RoiHandle::South)
        } else if near_top && point.x >= min_x && point.x <= max_x {
            Some(RoiHandle::North)
        } else {
            None
        }
    }

    fn resize(&mut self, handle: RoiHandle, dx: f64, dy: f64, min_size: f64) {
        let RoiShape::Rectangle { x1, y1, x2, y2 } = &mut self.shape else {
            return;
        };
        let (mut left, mut right) = if *x1 <= *x2 { (*x1, *x2) } else { (*x2, *x1) };
        let (mut bottom, mut top) = if *y1 <= *y2 { (*y1, *y2) } else { (*y2, *y1) };

        match handle {
            RoiHandle::West => left += dx,
            RoiHandle::East => right += dx,
            RoiHandle::South => bottom += dy,
            RoiHandle::North => top += dy,
            RoiHandle::SouthWest => {
                left += dx;
                bottom += dy;
            }
            RoiHandle::SouthEast => {
                right += dx;
                bottom += dy;
            }
            RoiHandle::NorthWest => {
                left += dx;
                top += dy;
            }
            RoiHandle::NorthEast => {
                right += dx;
                top += dy;
            }
        }

        if right - left < min_size {
            let mid = (right + left) * 0.5;
            left = mid - min_size * 0.5;
            right = mid + min_size * 0.5;
        }
        if top - bottom < min_size {
            let mid = (top + bottom) * 0.5;
            bottom = mid - min_size * 0.5;
            top = mid + min_size * 0.5;
        }

        *x1 = left;
        *x2 = right;
        *y1 = bottom;
        *y2 = top;
    }
}

fn draft_plot_points(draft: &RoiDraft) -> Vec<[f64; 2]> {
    let (min_x, max_x) = if draft.start.x <= draft.current.x {
        (draft.start.x, draft.current.x)
    } else {
        (draft.current.x, draft.start.x)
    };
    let (min_y, max_y) = if draft.start.y <= draft.current.y {
        (draft.start.y, draft.current.y)
    } else {
        (draft.current.y, draft.start.y)
    };
    vec![
        [min_x, min_y],
        [max_x, min_y],
        [max_x, max_y],
        [min_x, max_y],
    ]
}

fn point_in_polygon(point: PlotPoint, vertices: &[(f64, f64)]) -> bool {
    let n = vertices.len();
    if n < 3 {
        return false;
    }
    let mut inside = false;
    let mut j = n - 1;
    for i in 0..n {
        let (xi, yi) = vertices[i];
        let (xj, yj) = vertices[j];
        let intersects = ((yi > point.y) != (yj > point.y))
            && (point.x < (xj - xi) * (point.y - yi) / (yj - yi + f64::EPSILON) + xi);
        if intersects {
            inside = !inside;
        }
        j = i;
    }
    inside
}

fn roi_fill_color(color: Color32) -> Color32 {
    Color32::from_rgba_unmultiplied(color.r(), color.g(), color.b(), 48)
}

fn roi_palette_color(index: usize) -> Color32 {
    const PALETTE: [Color32; 10] = [
        Color32::from_rgb(0x4a, 0x9e, 0xff),
        Color32::from_rgb(0xef, 0x44, 0x44),
        Color32::from_rgb(0x10, 0xb9, 0x81),
        Color32::from_rgb(0xf5, 0x9e, 0x0b),
        Color32::from_rgb(0x8b, 0x5c, 0xff),
        Color32::from_rgb(0xf4, 0x72, 0xb6),
        Color32::from_rgb(0x22, 0xc5, 0xe5),
        Color32::from_rgb(0x84, 0xcc, 0x16),
        Color32::from_rgb(0xf9, 0x73, 0x16),
        Color32::from_rgb(0x06, 0xb6, 0xd4),
    ];
    PALETTE[index % PALETTE.len()]
}
