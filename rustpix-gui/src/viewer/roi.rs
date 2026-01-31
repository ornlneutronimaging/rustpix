//! ROI data structures and rendering helpers.

use eframe::egui::{Align2, Color32, Stroke};
use egui_plot::{
    Line, MarkerShape, PlotBounds, PlotPoint, PlotPoints, PlotUi, Points, Polygon, Text,
};

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
pub struct RoiPolygonDraft {
    pub vertices: Vec<(f64, f64)>,
    pub hover: Option<PlotPoint>,
}

#[derive(Debug, Clone)]
pub struct RoiDrag {
    pub roi_id: usize,
    pub last: PlotPoint,
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
}

#[derive(Debug, Clone)]
pub struct RoiVertexDrag {
    pub roi_id: usize,
    pub index: usize,
    pub last: PlotPoint,
    pub previous: Vec<(f64, f64)>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RoiCommitError {
    TooFewPoints,
    SelfIntersecting,
}

/// ROI session state.
#[derive(Debug)]
pub struct RoiState {
    pub mode: RoiSelectionMode,
    pub rois: Vec<Roi>,
    pub draft: Option<RoiDraft>,
    pub polygon_draft: Option<RoiPolygonDraft>,
    pub debounce_updates: bool,
    drag: Option<RoiDrag>,
    edit_drag: Option<RoiEditDrag>,
    vertex_drag: Option<RoiVertexDrag>,
    context_menu: Option<usize>,
    next_id: usize,
}

impl Default for RoiState {
    fn default() -> Self {
        Self {
            mode: RoiSelectionMode::default(),
            rois: Vec::new(),
            draft: None,
            polygon_draft: None,
            debounce_updates: false,
            drag: None,
            edit_drag: None,
            vertex_drag: None,
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
        self.polygon_draft = None;
        self.drag = None;
        self.edit_drag = None;
        self.vertex_drag = None;
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
        self.polygon_draft = None;
        self.drag = None;
        self.edit_drag = None;
        self.vertex_drag = None;
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
        self.polygon_draft = None;
        self.drag = None;
        self.edit_drag = None;
        self.vertex_drag = None;
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
        self.vertex_drag = None;
    }

    /// Cancel any in-progress ROI draft.
    pub fn cancel_draft(&mut self) {
        self.draft = None;
        self.polygon_draft = None;
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

    /// Add a point to the polygon draft.
    pub fn add_polygon_point(&mut self, point: PlotPoint) {
        let point = (point.x, point.y);
        if let Some(draft) = &mut self.polygon_draft {
            draft.vertices.push(point);
        } else {
            self.polygon_draft = Some(RoiPolygonDraft {
                vertices: vec![point],
                hover: None,
            });
        }
    }

    /// Update hover point for polygon draft preview.
    pub fn update_polygon_hover(&mut self, point: Option<PlotPoint>) {
        if let Some(draft) = &mut self.polygon_draft {
            draft.hover = point;
        }
    }

    /// Commit polygon draft into a ROI.
    pub fn commit_polygon(&mut self, min_points: usize) -> Result<(), RoiCommitError> {
        let Some(draft) = self.polygon_draft.clone() else {
            return Ok(());
        };
        if draft.vertices.len() < min_points {
            return Err(RoiCommitError::TooFewPoints);
        }
        if polygon_self_intersects(&draft.vertices) {
            return Err(RoiCommitError::SelfIntersecting);
        }
        let _ = self.polygon_draft.take();

        let id = self.next_id.max(1);
        self.next_id = id + 1;
        let color = roi_palette_color(id - 1);
        let roi = Roi {
            id,
            name: format!("ROI {id}"),
            color,
            shape: RoiShape::Polygon {
                vertices: draft.vertices,
            },
            visible: true,
            selected: false,
            edit_mode: false,
        };

        self.rois.push(roi);
        self.set_selected(Some(id));
        Ok(())
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
        let _ = bounds;
        self.drag = Some(RoiDrag {
            roi_id,
            last: start,
        });
    }

    /// Update drag with the new pointer position.
    pub fn update_drag(&mut self, current: PlotPoint, min: f64, max: f64) {
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
            roi.clamp_within(min, max);
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
        self.edit_drag.is_some() || self.vertex_drag.is_some()
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
        let _ = bounds;
        self.edit_drag = Some(RoiEditDrag {
            roi_id,
            handle,
            last: start,
        });
    }

    /// Update edit drag.
    pub fn update_edit_drag(&mut self, current: PlotPoint, min_size: f64, min: f64, max: f64) {
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
            roi.clamp_within(min, max);
        }
        edit.last = current;
    }

    /// End edit drag.
    pub fn end_edit_drag(&mut self) {
        self.edit_drag = None;
    }

    /// Start vertex drag (polygon edit).
    pub fn start_vertex_drag(
        &mut self,
        roi_id: usize,
        index: usize,
        start: PlotPoint,
        bounds: PlotBounds,
    ) {
        self.set_edit_mode(roi_id, true);
        let _ = bounds;
        let previous = self
            .rois
            .iter()
            .find(|roi| roi.id == roi_id)
            .and_then(|roi| match &roi.shape {
                RoiShape::Polygon { vertices } => Some(vertices.clone()),
                RoiShape::Rectangle { .. } => None,
            })
            .unwrap_or_default();
        self.vertex_drag = Some(RoiVertexDrag {
            roi_id,
            index,
            last: start,
            previous,
        });
    }

    /// Update vertex drag.
    pub fn update_vertex_drag(&mut self, current: PlotPoint) {
        let Some(drag) = &mut self.vertex_drag else {
            return;
        };
        if let Some(roi) = self.rois.iter_mut().find(|roi| roi.id == drag.roi_id) {
            roi.move_vertex(drag.index, current);
        }
        drag.last = current;
    }

    /// End vertex drag.
    pub fn end_vertex_drag(&mut self) -> Result<(), RoiCommitError> {
        let Some(drag) = self.vertex_drag.take() else {
            return Ok(());
        };
        if let Some(roi) = self.rois.iter_mut().find(|roi| roi.id == drag.roi_id) {
            if let RoiShape::Polygon { vertices } = &mut roi.shape {
                if polygon_self_intersects(vertices) {
                    *vertices = drag.previous;
                    return Err(RoiCommitError::SelfIntersecting);
                }
            }
        }
        Ok(())
    }

    /// Delete a polygon vertex by hit test.
    pub fn delete_vertex_at(&mut self, point: PlotPoint, threshold: f64) -> bool {
        if let Some((roi_id, index)) = self.hit_test_vertex(point, threshold) {
            if let Some(roi) = self.rois.iter_mut().find(|roi| roi.id == roi_id) {
                return roi.delete_vertex(index);
            }
        }
        false
    }

    /// Insert a polygon vertex on edge hit.
    pub fn insert_vertex_at(
        &mut self,
        point: PlotPoint,
        threshold: f64,
    ) -> Result<bool, RoiCommitError> {
        if let Some((roi_id, edge_index)) = self.hit_test_edge(point, threshold) {
            if let Some(roi) = self.rois.iter_mut().find(|roi| roi.id == roi_id) {
                if let RoiShape::Polygon { vertices } = &mut roi.shape {
                    let insert_at = (edge_index + 1).min(vertices.len());
                    vertices.insert(insert_at, (point.x, point.y));
                    if polygon_self_intersects(vertices) {
                        vertices.remove(insert_at);
                        return Err(RoiCommitError::SelfIntersecting);
                    }
                    return Ok(true);
                }
            }
        }
        Ok(false)
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

    /// Hit test polygon vertices in edit mode.
    pub fn hit_test_vertex(&self, point: PlotPoint, threshold: f64) -> Option<(usize, usize)> {
        for roi in self.rois.iter().rev() {
            if !roi.edit_mode {
                continue;
            }
            if let Some(index) = roi.vertex_hit(point, threshold) {
                return Some((roi.id, index));
            }
        }
        None
    }

    /// Hit test polygon edges in edit mode.
    pub fn hit_test_edge(&self, point: PlotPoint, threshold: f64) -> Option<(usize, usize)> {
        for roi in self.rois.iter().rev() {
            if !roi.edit_mode {
                continue;
            }
            if let Some(index) = roi.edge_hit(point, threshold) {
                return Some((roi.id, index));
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

            if let RoiShape::Polygon { vertices } = &roi.shape {
                if polygon_is_convex(vertices) {
                    let points = roi.plot_points();
                    plot_ui.polygon(Polygon::new(points).stroke(stroke).fill_color(fill));
                } else {
                    let triangles = triangulate_polygon(vertices);
                    if triangles.is_empty() {
                        let line_points = roi.closed_line_points();
                        plot_ui.line(
                            Line::new(PlotPoints::new(line_points))
                                .color(roi.color)
                                .width(stroke_width),
                        );
                    } else {
                        for tri in triangles {
                            plot_ui.polygon(
                                Polygon::new(vec![tri[0], tri[1], tri[2]])
                                    .stroke(Stroke::new(0.0, Color32::TRANSPARENT))
                                    .fill_color(fill),
                            );
                        }
                        let line_points = roi.closed_line_points();
                        plot_ui.line(
                            Line::new(PlotPoints::new(line_points))
                                .color(roi.color)
                                .width(stroke_width),
                        );
                    }
                }
            } else {
                let points = roi.plot_points();
                plot_ui.polygon(Polygon::new(points).stroke(stroke).fill_color(fill));
            }

            let label_pos = roi.label_position();
            plot_ui.text(
                Text::new(label_pos, roi.name.clone())
                    .color(roi.color)
                    .anchor(Align2::LEFT_TOP),
            );

            if roi.edit_mode {
                let handle_points = roi.handle_points();
                if !handle_points.is_empty() {
                    plot_ui.points(
                        Points::new(handle_points)
                            .color(roi.color)
                            .shape(MarkerShape::Square)
                            .radius(3.0),
                    );
                }
            }
        }
    }

    /// Render the draft ROI while dragging.
    pub fn draw_draft(&self, plot_ui: &mut PlotUi) {
        if let Some(draft) = &self.draft {
            let color = roi_palette_color(self.next_id.saturating_sub(1));
            let stroke = Stroke::new(1.0, color);
            let fill = roi_fill_color(color);
            let points = draft_plot_points(draft);
            plot_ui.polygon(Polygon::new(points).stroke(stroke).fill_color(fill));
        }

        if let Some(draft) = &self.polygon_draft {
            let color = roi_palette_color(self.next_id.saturating_sub(1));
            if !draft.vertices.is_empty() {
                let mut line_points: Vec<[f64; 2]> =
                    draft.vertices.iter().map(|(x, y)| [*x, *y]).collect();
                if let Some(hover) = draft.hover {
                    line_points.push([hover.x, hover.y]);
                }
                plot_ui.line(Line::new(PlotPoints::new(line_points)).color(color));
                let handle_points: Vec<[f64; 2]> =
                    draft.vertices.iter().map(|(x, y)| [*x, *y]).collect();
                plot_ui.points(
                    Points::new(handle_points)
                        .color(color)
                        .shape(MarkerShape::Circle)
                        .radius(3.0),
                );
            }
        }
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

    fn clamp_within(&mut self, min: f64, max: f64) {
        match &mut self.shape {
            RoiShape::Rectangle { x1, y1, x2, y2 } => {
                let min_x = x1.min(*x2);
                let max_x = x1.max(*x2);
                let min_y = y1.min(*y2);
                let max_y = y1.max(*y2);
                let mut shift_x = 0.0;
                let mut shift_y = 0.0;
                if min_x < min {
                    shift_x = min - min_x;
                } else if max_x > max {
                    shift_x = max - max_x;
                }
                if min_y < min {
                    shift_y = min - min_y;
                } else if max_y > max {
                    shift_y = max - max_y;
                }
                *x1 += shift_x;
                *x2 += shift_x;
                *y1 += shift_y;
                *y2 += shift_y;
            }
            RoiShape::Polygon { vertices } => {
                if vertices.is_empty() {
                    return;
                }
                let mut min_x = f64::INFINITY;
                let mut max_x = f64::NEG_INFINITY;
                let mut min_y = f64::INFINITY;
                let mut max_y = f64::NEG_INFINITY;
                for (x, y) in vertices.iter() {
                    min_x = min_x.min(*x);
                    max_x = max_x.max(*x);
                    min_y = min_y.min(*y);
                    max_y = max_y.max(*y);
                }
                let mut shift_x = 0.0;
                let mut shift_y = 0.0;
                if min_x < min {
                    shift_x = min - min_x;
                } else if max_x > max {
                    shift_x = max - max_x;
                }
                if min_y < min {
                    shift_y = min - min_y;
                } else if max_y > max {
                    shift_y = max - max_y;
                }
                if shift_x != 0.0 || shift_y != 0.0 {
                    for (x, y) in vertices.iter_mut() {
                        *x += shift_x;
                        *y += shift_y;
                    }
                }
            }
        }
    }

    fn handle_points(&self) -> Vec<[f64; 2]> {
        match &self.shape {
            RoiShape::Rectangle { x1, y1, x2, y2 } => {
                let min_x = x1.min(*x2);
                let max_x = x1.max(*x2);
                let min_y = y1.min(*y2);
                let max_y = y1.max(*y2);
                let center_x = (min_x + max_x) * 0.5;
                let center_y = (min_y + max_y) * 0.5;
                vec![
                    [min_x, min_y],
                    [center_x, min_y],
                    [max_x, min_y],
                    [max_x, center_y],
                    [max_x, max_y],
                    [center_x, max_y],
                    [min_x, max_y],
                    [min_x, center_y],
                ]
            }
            RoiShape::Polygon { vertices } => {
                vertices.iter().map(|(x, y)| [*x, *y]).collect::<Vec<_>>()
            }
        }
    }

    fn closed_line_points(&self) -> Vec<[f64; 2]> {
        match &self.shape {
            RoiShape::Polygon { vertices } => {
                if vertices.len() < 2 {
                    return Vec::new();
                }
                let mut points: Vec<[f64; 2]> = vertices.iter().map(|(x, y)| [*x, *y]).collect();
                let first = points[0];
                points.push(first);
                points
            }
            RoiShape::Rectangle { x1, y1, x2, y2 } => {
                vec![[*x1, *y1], [*x2, *y1], [*x2, *y2], [*x1, *y2], [*x1, *y1]]
            }
        }
    }

    fn vertex_hit(&self, point: PlotPoint, threshold: f64) -> Option<usize> {
        let RoiShape::Polygon { vertices } = &self.shape else {
            return None;
        };
        for (idx, (x, y)) in vertices.iter().enumerate() {
            let dx = point.x - *x;
            let dy = point.y - *y;
            if (dx * dx + dy * dy).sqrt() <= threshold {
                return Some(idx);
            }
        }
        None
    }

    fn edge_hit(&self, point: PlotPoint, threshold: f64) -> Option<usize> {
        let RoiShape::Polygon { vertices } = &self.shape else {
            return None;
        };
        if vertices.len() < 2 {
            return None;
        }
        let mut closest = None;
        let mut closest_dist = f64::INFINITY;
        for i in 0..vertices.len() {
            let (ax, ay) = vertices[i];
            let (bx, by) = vertices[(i + 1) % vertices.len()];
            let dist = distance_point_to_segment(point, (ax, ay), (bx, by));
            if dist < closest_dist {
                closest_dist = dist;
                closest = Some(i);
            }
        }
        if closest_dist <= threshold {
            closest
        } else {
            None
        }
    }

    fn move_vertex(&mut self, index: usize, point: PlotPoint) {
        let RoiShape::Polygon { vertices } = &mut self.shape else {
            return;
        };
        if let Some(vertex) = vertices.get_mut(index) {
            *vertex = (point.x, point.y);
        }
    }

    fn delete_vertex(&mut self, index: usize) -> bool {
        let RoiShape::Polygon { vertices } = &mut self.shape else {
            return false;
        };
        if vertices.len() <= 3 || index >= vertices.len() {
            return false;
        }
        vertices.remove(index);
        true
    }
}

fn distance_point_to_segment(point: PlotPoint, a: (f64, f64), b: (f64, f64)) -> f64 {
    let (px, py) = (point.x, point.y);
    let (ax, ay) = a;
    let (bx, by) = b;
    let abx = bx - ax;
    let aby = by - ay;
    let apx = px - ax;
    let apy = py - ay;
    let ab_len_sq = abx * abx + aby * aby;
    if ab_len_sq <= f64::EPSILON {
        return ((px - ax).powi(2) + (py - ay).powi(2)).sqrt();
    }
    let t = (apx * abx + apy * aby) / ab_len_sq;
    let t = t.clamp(0.0, 1.0);
    let closest_x = ax + abx * t;
    let closest_y = ay + aby * t;
    ((px - closest_x).powi(2) + (py - closest_y).powi(2)).sqrt()
}

fn polygon_self_intersects(vertices: &[(f64, f64)]) -> bool {
    let n = vertices.len();
    if n < 4 {
        return false;
    }
    for i in 0..n {
        let a1 = vertices[i];
        let a2 = vertices[(i + 1) % n];
        for j in (i + 1)..n {
            let b1 = vertices[j];
            let b2 = vertices[(j + 1) % n];
            if shares_endpoint(i, j, n) {
                continue;
            }
            if segments_intersect(a1, a2, b1, b2) {
                return true;
            }
        }
    }
    false
}

fn polygon_is_convex(vertices: &[(f64, f64)]) -> bool {
    if vertices.len() < 4 {
        return true;
    }
    let mut sign = 0.0;
    let n = vertices.len();
    for i in 0..n {
        let (x1, y1) = vertices[i];
        let (x2, y2) = vertices[(i + 1) % n];
        let (x3, y3) = vertices[(i + 2) % n];
        let cross = (x2 - x1) * (y3 - y2) - (y2 - y1) * (x3 - x2);
        if cross.abs() <= f64::EPSILON {
            continue;
        }
        if sign == 0.0 {
            sign = cross;
        } else if sign * cross < 0.0 {
            return false;
        }
    }
    true
}

fn triangulate_polygon(vertices: &[(f64, f64)]) -> Vec<[[f64; 2]; 3]> {
    let n = vertices.len();
    if n < 3 {
        return Vec::new();
    }
    let area = polygon_area2(vertices);
    if area.abs() <= f64::EPSILON {
        return Vec::new();
    }
    let ccw = area > 0.0;
    let mut indices: Vec<usize> = (0..n).collect();
    let mut triangles = Vec::new();
    let mut guard = 0;
    while indices.len() > 2 && guard < n * n {
        guard += 1;
        let mut ear_found = false;
        for i in 0..indices.len() {
            let prev = indices[(i + indices.len() - 1) % indices.len()];
            let curr = indices[i];
            let next = indices[(i + 1) % indices.len()];
            if !is_convex(vertices[prev], vertices[curr], vertices[next], ccw) {
                continue;
            }
            if triangle_area2(vertices[prev], vertices[curr], vertices[next]).abs() <= f64::EPSILON
            {
                continue;
            }
            let mut contains = false;
            for &idx in &indices {
                if idx == prev || idx == curr || idx == next {
                    continue;
                }
                if point_in_triangle(
                    vertices[idx],
                    vertices[prev],
                    vertices[curr],
                    vertices[next],
                ) {
                    contains = true;
                    break;
                }
            }
            if contains {
                continue;
            }
            triangles.push([
                [vertices[prev].0, vertices[prev].1],
                [vertices[curr].0, vertices[curr].1],
                [vertices[next].0, vertices[next].1],
            ]);
            indices.remove(i);
            ear_found = true;
            break;
        }
        if !ear_found {
            break;
        }
    }
    triangles
}

fn polygon_area2(vertices: &[(f64, f64)]) -> f64 {
    let mut area = 0.0;
    for i in 0..vertices.len() {
        let (x1, y1) = vertices[i];
        let (x2, y2) = vertices[(i + 1) % vertices.len()];
        area += x1 * y2 - x2 * y1;
    }
    area
}

fn triangle_area2(a: (f64, f64), b: (f64, f64), c: (f64, f64)) -> f64 {
    (b.0 - a.0) * (c.1 - a.1) - (b.1 - a.1) * (c.0 - a.0)
}

fn is_convex(a: (f64, f64), b: (f64, f64), c: (f64, f64), ccw: bool) -> bool {
    let cross = triangle_area2(a, b, c);
    if ccw {
        cross > f64::EPSILON
    } else {
        cross < -f64::EPSILON
    }
}

fn point_in_triangle(p: (f64, f64), a: (f64, f64), b: (f64, f64), c: (f64, f64)) -> bool {
    let area1 = triangle_area2(p, a, b);
    let area2 = triangle_area2(p, b, c);
    let area3 = triangle_area2(p, c, a);

    let has_neg = area1 < -f64::EPSILON || area2 < -f64::EPSILON || area3 < -f64::EPSILON;
    let has_pos = area1 > f64::EPSILON || area2 > f64::EPSILON || area3 > f64::EPSILON;
    !(has_neg && has_pos)
}

fn shares_endpoint(i: usize, j: usize, n: usize) -> bool {
    if i == j {
        return true;
    }
    let next_i = (i + 1) % n;
    let next_j = (j + 1) % n;
    i == next_j || j == next_i
}

fn segments_intersect(a1: (f64, f64), a2: (f64, f64), b1: (f64, f64), b2: (f64, f64)) -> bool {
    let d1 = direction(a1, a2, b1);
    let d2 = direction(a1, a2, b2);
    let d3 = direction(b1, b2, a1);
    let d4 = direction(b1, b2, a2);

    if (d1 > 0.0 && d2 < 0.0 || d1 < 0.0 && d2 > 0.0)
        && (d3 > 0.0 && d4 < 0.0 || d3 < 0.0 && d4 > 0.0)
    {
        return true;
    }

    if d1.abs() <= f64::EPSILON && on_segment(a1, a2, b1) {
        return true;
    }
    if d2.abs() <= f64::EPSILON && on_segment(a1, a2, b2) {
        return true;
    }
    if d3.abs() <= f64::EPSILON && on_segment(b1, b2, a1) {
        return true;
    }
    if d4.abs() <= f64::EPSILON && on_segment(b1, b2, a2) {
        return true;
    }
    false
}

fn direction(a: (f64, f64), b: (f64, f64), c: (f64, f64)) -> f64 {
    (c.0 - a.0) * (b.1 - a.1) - (c.1 - a.1) * (b.0 - a.0)
}

fn on_segment(a: (f64, f64), b: (f64, f64), c: (f64, f64)) -> bool {
    let min_x = a.0.min(b.0) - f64::EPSILON;
    let max_x = a.0.max(b.0) + f64::EPSILON;
    let min_y = a.1.min(b.1) - f64::EPSILON;
    let max_y = a.1.max(b.1) + f64::EPSILON;
    c.0 >= min_x && c.0 <= max_x && c.1 >= min_y && c.1 <= max_y
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
