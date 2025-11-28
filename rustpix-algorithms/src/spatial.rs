//! Spatial indexing for efficient neighbor lookup.

use rustpix_core::{Hit, PixelCoord};
use std::collections::HashMap;

/// Spatial index for efficient 2D neighbor queries.
///
/// Uses a grid-based approach where the detector area is divided
/// into cells. Each cell contains a list of hit indices that fall
/// within that cell.
#[derive(Debug)]
pub struct SpatialIndex {
    /// Cell size (in pixels).
    cell_size: u16,
    /// Map from cell coordinates to list of hit indices.
    cells: HashMap<(u16, u16), Vec<usize>>,
    /// Detector width.
    width: u16,
    /// Detector height.
    height: u16,
}

impl SpatialIndex {
    /// Creates a new spatial index with the given cell size.
    pub fn new(cell_size: u16) -> Self {
        Self {
            cell_size,
            cells: HashMap::new(),
            width: 256,
            height: 256,
        }
    }

    /// Creates a new spatial index with custom detector dimensions.
    pub fn with_dimensions(cell_size: u16, width: u16, height: u16) -> Self {
        Self {
            cell_size,
            cells: HashMap::new(),
            width,
            height,
        }
    }

    /// Builds the spatial index from a slice of hits.
    pub fn build<H: Hit>(&mut self, hits: &[H]) {
        self.cells.clear();

        for (idx, hit) in hits.iter().enumerate() {
            let cell = self.coord_to_cell(hit.coord());
            self.cells.entry(cell).or_default().push(idx);
        }
    }

    /// Converts a pixel coordinate to a cell coordinate.
    #[inline]
    fn coord_to_cell(&self, coord: PixelCoord) -> (u16, u16) {
        (coord.x / self.cell_size, coord.y / self.cell_size)
    }

    /// Finds all hit indices within the spatial neighborhood of a coordinate.
    ///
    /// Returns indices of hits in the same cell and all neighboring cells.
    pub fn find_neighbors(&self, coord: PixelCoord) -> Vec<usize> {
        let (cx, cy) = self.coord_to_cell(coord);
        let mut neighbors = Vec::new();

        let cell_x_max = self.width / self.cell_size;
        let cell_y_max = self.height / self.cell_size;

        for dx in 0..=2 {
            for dy in 0..=2 {
                let nx = (cx as i32 + dx - 1) as u16;
                let ny = (cy as i32 + dy - 1) as u16;

                if nx < cell_x_max && ny < cell_y_max {
                    if let Some(indices) = self.cells.get(&(nx, ny)) {
                        neighbors.extend(indices);
                    }
                }
            }
        }

        neighbors
    }

    /// Returns the number of cells in the index.
    pub fn cell_count(&self) -> usize {
        self.cells.len()
    }

    /// Returns the total number of indexed hits.
    pub fn hit_count(&self) -> usize {
        self.cells.values().map(|v| v.len()).sum()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use rustpix_core::HitData;

    #[test]
    fn test_spatial_index_build() {
        let hits = vec![
            HitData::new(0, 0, 100, 10),
            HitData::new(5, 5, 110, 15),
            HitData::new(100, 100, 120, 20),
        ];

        let mut index = SpatialIndex::new(16);
        index.build(&hits);

        assert_eq!(index.hit_count(), 3);
    }

    #[test]
    fn test_spatial_index_neighbors() {
        let hits = vec![
            HitData::new(0, 0, 100, 10),
            HitData::new(1, 0, 110, 15),
            HitData::new(100, 100, 120, 20),
        ];

        let mut index = SpatialIndex::new(16);
        index.build(&hits);

        // Hits at (0,0) and (1,0) should be in the same cell
        let neighbors = index.find_neighbors(PixelCoord::new(0, 0));
        assert!(neighbors.contains(&0));
        assert!(neighbors.contains(&1));
        assert!(!neighbors.contains(&2));
    }
}
