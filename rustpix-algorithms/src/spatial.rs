//! Spatial indexing for efficient neighbor lookup.
//!
//! See IMPLEMENTATION_PLAN.md Part 4 for detailed specification.

/// Spatial grid for efficient 2D neighbor queries.
///
/// Uses a dense grid-based approach where the detector area is divided into cells.
/// This implementation is optimized for fixed-size detectors and avoids
/// hashing overhead.
#[derive(Debug, Default)]
pub struct SpatialGrid<T> {
    cell_size: usize,
    width_cells: usize,
    height_cells: usize,
    // Flattened grid of cells. Index = y * width + x.
    cells: Vec<Vec<T>>,
}

impl<T: Clone> SpatialGrid<T> {
    /// Create a new spatial grid.
    ///
    /// # Arguments
    /// * `cell_size` - Size of each cell in pixels (e.g., 32).
    /// * `width` - Total width of the detector in pixels (e.g., 256).
    /// * `height` - Total height of the detector in pixels (e.g., 256).
    pub fn new(cell_size: usize, width: usize, height: usize) -> Self {
        // Ensure cell_size is non-zero
        let cell_size = cell_size.max(1);

        // Calculate grid dimensions
        let width_cells = width.div_ceil(cell_size);
        let height_cells = height.div_ceil(cell_size);
        let total_cells = width_cells * height_cells;

        // Pre-allocate cells
        let mut cells = Vec::with_capacity(total_cells);
        for _ in 0..total_cells {
            cells.push(Vec::with_capacity(4)); // Expect small number of hits per cell usually
        }

        Self {
            cell_size,
            width_cells,
            height_cells,
            cells,
        }
    }

    /// Clear all data but keep allocations.
    pub fn clear(&mut self) {
        for cell in &mut self.cells {
            cell.clear();
        }
    }

    /// Ensure the grid is large enough for the given dimensions.
    ///
    /// If the grid is resized, all existing data is CLEARED.
    pub fn ensure_dimensions(&mut self, width: usize, height: usize) {
        let req_width_cells = width.div_ceil(self.cell_size);
        let req_height_cells = height.div_ceil(self.cell_size);

        if req_width_cells > self.width_cells || req_height_cells > self.height_cells {
            let new_width_cells = req_width_cells.max(self.width_cells);
            let new_height_cells = req_height_cells.max(self.height_cells);
            let total_cells = new_width_cells * new_height_cells;

            self.width_cells = new_width_cells;
            self.height_cells = new_height_cells;

            // Re-allocate cells
            self.cells = Vec::with_capacity(total_cells);
            for _ in 0..total_cells {
                self.cells.push(Vec::with_capacity(4));
            }
        }
    }

    /// Insert a value at the given coordinates.
    ///
    /// Ignores values outside the grid bounds.
    #[inline]
    pub fn insert(&mut self, x: i32, y: i32, value: T) {
        if x < 0 || y < 0 {
            return;
        }

        let cx = (x as usize) / self.cell_size;
        let cy = (y as usize) / self.cell_size;

        if cx < self.width_cells && cy < self.height_cells {
            let idx = cy * self.width_cells + cx;
            // SAFETY: bounds checked above. `idx` is guaranteed to be < total_cells because:
            // 1. cx < width_cells and cy < height_cells (checked by if conditions).
            // 2. idx = cy * width_cells + cx.
            // 3. total_cells = width_cells * height_cells.
            // Therefore idx < total_cells.
            unsafe {
                self.cells.get_unchecked_mut(idx).push(value);
            }
        }
    }

    /// Get the cell index for a given coordinate.
    #[inline]
    fn get_cell_index(&self, cx: i32, cy: i32) -> Option<usize> {
        if cx < 0 || cy < 0 {
            return None;
        }
        let cx = cx as usize;
        let cy = cy as usize;

        if cx < self.width_cells && cy < self.height_cells {
            Some(cy * self.width_cells + cx)
        } else {
            None
        }
    }

    /// Remove a value from the given coordinates.
    pub fn remove(&mut self, x: i32, y: i32, value: T)
    where
        T: PartialEq,
    {
        if let Some(idx) = self.get_cell_index(x, y) {
            // SAFETY: get_cell_index checks bounds
            let cell = unsafe { self.cells.get_unchecked_mut(idx) };
            if let Some(pos) = cell.iter().position(|x| *x == value) {
                cell.swap_remove(pos);
            }
        }
    }

    /// Get reference to the slice of values in the cell at (x, y).
    #[inline]
    pub fn get_cell_slice(&self, x: i32, y: i32) -> Option<&[T]> {
        if x < 0 || y < 0 {
            return None;
        }
        let cx = (x as usize / self.cell_size) as i32;
        let cy = (y as usize / self.cell_size) as i32;
        self.get_cell_index(cx, cy).map(|idx| {
            // SAFETY: get_cell_index checks bounds
            unsafe { self.cells.get_unchecked(idx).as_slice() }
        })
    }

    /// Get the grid width in cells.
    #[inline]
    pub fn width_cells(&self) -> usize {
        self.width_cells
    }

    /// Get the grid height in cells.
    #[inline]
    pub fn height_cells(&self) -> usize {
        self.height_cells
    }

    /// Get the configured cell size.
    #[inline]
    pub fn cell_size(&self) -> usize {
        self.cell_size
    }

    /// Query the 3x3 neighborhood around a point.
    ///
    /// Appends neighbors to the provided buffer to avoid allocation.
    #[inline]
    pub fn query_neighborhood(&self, x: i32, y: i32, buffer: &mut Vec<T>) {
        let cx = (x as usize / self.cell_size) as i32;
        let cy = (y as usize / self.cell_size) as i32;

        // Check 3x3 area
        for dy in -1..=1 {
            let ny = cy + dy;
            if ny < 0 || ny >= self.height_cells as i32 {
                continue;
            }

            for dx in -1..=1 {
                let nx = cx + dx;
                if nx < 0 || nx >= self.width_cells as i32 {
                    continue;
                }

                let idx = (ny as usize) * self.width_cells + (nx as usize);
                // SAFETY: indexing logic guarantees bounds
                let cell = unsafe { self.cells.get_unchecked(idx) };
                buffer.extend_from_slice(cell);
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_spatial_grid() {
        let mut grid: SpatialGrid<usize> = SpatialGrid::new(32, 512, 512);
        grid.insert(100, 100, 0);
        grid.insert(105, 105, 1);
        grid.insert(300, 300, 2);

        let mut neighbors = Vec::new();
        grid.query_neighborhood(100, 100, &mut neighbors);

        assert!(neighbors.contains(&0));
        assert!(neighbors.contains(&1));
        assert!(!neighbors.contains(&2));
    }

    #[test]
    fn test_spatial_grid_boundaries() {
        let mut grid: SpatialGrid<usize> = SpatialGrid::new(50, 200, 200);

        // Insert at edges
        grid.insert(0, 0, 1);
        grid.insert(199, 199, 2);
        grid.insert(200, 200, 3); // Out of bounds

        let mut neighbors = Vec::new();
        grid.query_neighborhood(0, 0, &mut neighbors);
        assert_eq!(neighbors.len(), 1);
        assert_eq!(neighbors[0], 1);

        neighbors.clear();
        grid.query_neighborhood(199, 199, &mut neighbors);
        assert_eq!(neighbors.len(), 1);
        assert_eq!(neighbors[0], 2);
    }
}
