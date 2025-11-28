//! Spatial indexing for efficient neighbor lookup.
//!
//! See IMPLEMENTATION_PLAN.md Part 4 for detailed specification.

use std::collections::HashMap;

/// Spatial grid for efficient 2D neighbor queries.
///
/// Uses a grid-based approach where the detector area is divided into cells.
#[derive(Debug, Default)]
pub struct SpatialGrid<T> {
    cell_size: usize,
    cells: HashMap<(i32, i32), Vec<T>>,
}

impl<T: Clone> SpatialGrid<T> {
    /// Create a new spatial grid.
    pub fn new(cell_size: usize, _width: usize, _height: usize) -> Self {
        Self {
            cell_size,
            cells: HashMap::new(),
        }
    }

    /// Clear all data.
    pub fn clear(&mut self) {
        self.cells.clear();
    }

    /// Insert a value at the given coordinates.
    pub fn insert(&mut self, x: i32, y: i32, value: T) {
        let cell = (x / self.cell_size as i32, y / self.cell_size as i32);
        self.cells.entry(cell).or_default().push(value);
    }

    /// Remove a value from the given coordinates.
    pub fn remove(&mut self, x: i32, y: i32, _value: T) {
        let cell = (x / self.cell_size as i32, y / self.cell_size as i32);
        if let Some(values) = self.cells.get_mut(&cell) {
            values.pop(); // Simplified removal
        }
    }

    /// Query the 3x3 neighborhood around a point.
    pub fn query_neighborhood(&self, x: i32, y: i32) -> Vec<&T> {
        let cx = x / self.cell_size as i32;
        let cy = y / self.cell_size as i32;
        let mut result = Vec::new();

        for dx in -1..=1 {
            for dy in -1..=1 {
                if let Some(values) = self.cells.get(&(cx + dx, cy + dy)) {
                    result.extend(values.iter());
                }
            }
        }

        result
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

        let neighbors = grid.query_neighborhood(100, 100);
        assert!(neighbors.contains(&&0));
        assert!(neighbors.contains(&&1));
        assert!(!neighbors.contains(&&2));
    }
}
