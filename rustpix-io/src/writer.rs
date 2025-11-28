//! File writers for TPX3 data.

use crate::Result;
use rustpix_core::Centroid;
use std::fs::File;
use std::io::{BufWriter, Write};
use std::path::Path;

/// Writer for TPX3 processed data output.
///
/// Writes processed neutron/centroid data to files in various formats.
pub struct Tpx3FileWriter {
    writer: BufWriter<File>,
}

impl Tpx3FileWriter {
    /// Creates a new file writer.
    pub fn create<P: AsRef<Path>>(path: P) -> Result<Self> {
        let file = File::create(path)?;
        let writer = BufWriter::new(file);
        Ok(Self { writer })
    }

    /// Writes centroids as CSV.
    pub fn write_centroids_csv(&mut self, centroids: &[Centroid]) -> Result<()> {
        writeln!(self.writer, "x,y,toa,tot_sum,cluster_size")?;

        for c in centroids {
            writeln!(
                self.writer,
                "{},{},{},{},{}",
                c.x,
                c.y,
                c.toa.as_u64(),
                c.tot_sum,
                c.cluster_size
            )?;
        }

        self.writer.flush()?;
        Ok(())
    }

    /// Writes centroids as binary data.
    ///
    /// Format: For each centroid: f64 (x) + f64 (y) + u64 (toa) + u32 (tot_sum) + u16 (cluster_size)
    /// Total: 30 bytes per centroid
    pub fn write_centroids_binary(&mut self, centroids: &[Centroid]) -> Result<()> {
        for c in centroids {
            self.writer.write_all(&c.x.to_le_bytes())?;
            self.writer.write_all(&c.y.to_le_bytes())?;
            self.writer.write_all(&c.toa.as_u64().to_le_bytes())?;
            self.writer.write_all(&c.tot_sum.to_le_bytes())?;
            self.writer.write_all(&c.cluster_size.to_le_bytes())?;
        }

        self.writer.flush()?;
        Ok(())
    }

    /// Flushes the writer.
    pub fn flush(&mut self) -> Result<()> {
        self.writer.flush()?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::NamedTempFile;

    #[test]
    fn test_write_centroids_csv() {
        let file = NamedTempFile::new().unwrap();
        let mut writer = Tpx3FileWriter::create(file.path()).unwrap();

        let centroids = vec![
            Centroid::new(1.5, 2.5, 1000, 100, 5),
            Centroid::new(10.3, 20.7, 2000, 200, 8),
        ];

        writer.write_centroids_csv(&centroids).unwrap();

        let content = std::fs::read_to_string(file.path()).unwrap();
        assert!(content.contains("x,y,toa,tot_sum,cluster_size"));
        assert!(content.contains("1.5,2.5,1000,100,5"));
        assert!(content.contains("10.3,20.7,2000,200,8"));
    }

    #[test]
    fn test_write_centroids_binary() {
        let file = NamedTempFile::new().unwrap();
        let mut writer = Tpx3FileWriter::create(file.path()).unwrap();

        let centroids = vec![Centroid::new(1.5, 2.5, 1000, 100, 5)];

        writer.write_centroids_binary(&centroids).unwrap();

        let data = std::fs::read(file.path()).unwrap();
        // 8 (f64) + 8 (f64) + 8 (u64) + 4 (u32) + 2 (u16) = 30 bytes
        assert_eq!(data.len(), 30);
    }
}
