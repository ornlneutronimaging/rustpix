//! File writers for processed data.
#![allow(clippy::missing_errors_doc, clippy::doc_markdown)]
//!

use crate::Result;
use rustpix_core::neutron::{Neutron, NeutronBatch};
use std::fs::File;
use std::io::{BufWriter, Write};
use std::path::Path;

/// Writer for processed data output.
///
/// Writes processed neutron data to files in various formats.
pub struct DataFileWriter {
    writer: BufWriter<File>,
}

impl DataFileWriter {
    /// Creates a new file writer.
    pub fn create<P: AsRef<Path>>(path: P) -> Result<Self> {
        let file = File::create(path)?;
        let writer = BufWriter::new(file);
        Ok(Self { writer })
    }

    /// Writes neutrons as CSV.
    pub fn write_neutrons_csv(&mut self, neutrons: &[Neutron]) -> Result<()> {
        writeln!(self.writer, "x,y,tof,tot,n_hits,chip_id")?;

        for n in neutrons {
            writeln!(
                self.writer,
                "{},{},{},{},{},{}",
                n.x, n.y, n.tof, n.tot, n.n_hits, n.chip_id
            )?;
        }

        self.writer.flush()?;
        Ok(())
    }

    /// Writes neutrons as binary data.
    ///
    /// Format per neutron: f64 (x) + f64 (y) + u32 (tof) + u16 (tot) + u16 (n_hits) + u8 (chip_id) + 3 reserved
    /// Total: 28 bytes per neutron
    pub fn write_neutrons_binary(&mut self, neutrons: &[Neutron]) -> Result<()> {
        for n in neutrons {
            self.writer.write_all(&n.x.to_le_bytes())?;
            self.writer.write_all(&n.y.to_le_bytes())?;
            self.writer.write_all(&n.tof.to_le_bytes())?;
            self.writer.write_all(&n.tot.to_le_bytes())?;
            self.writer.write_all(&n.n_hits.to_le_bytes())?;
            self.writer.write_all(&[n.chip_id])?;
            self.writer.write_all(&[0u8; 3])?; // Reserved/padding
        }

        self.writer.flush()?;
        Ok(())
    }

    /// Writes neutron batch as CSV.
    pub fn write_neutron_batch_csv(&mut self, batch: &NeutronBatch, include_header: bool) -> Result<()> {
        if include_header {
            writeln!(self.writer, "x,y,tof,tot,n_hits,chip_id")?;
        }

        for i in 0..batch.len() {
            writeln!(
                self.writer,
                "{},{},{},{},{},{}",
                batch.x[i],
                batch.y[i],
                batch.tof[i],
                batch.tot[i],
                batch.n_hits[i],
                batch.chip_id[i]
            )?;
        }

        self.writer.flush()?;
        Ok(())
    }

    /// Writes neutron batch as binary data.
    pub fn write_neutron_batch_binary(&mut self, batch: &NeutronBatch) -> Result<()> {
        for i in 0..batch.len() {
            self.writer.write_all(&batch.x[i].to_le_bytes())?;
            self.writer.write_all(&batch.y[i].to_le_bytes())?;
            self.writer.write_all(&batch.tof[i].to_le_bytes())?;
            self.writer.write_all(&batch.tot[i].to_le_bytes())?;
            self.writer.write_all(&batch.n_hits[i].to_le_bytes())?;
            self.writer.write_all(&[batch.chip_id[i]])?;
            self.writer.write_all(&[0u8; 3])?; // Reserved/padding
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
    fn test_write_neutrons_csv() {
        let file = NamedTempFile::new().unwrap();
        let mut writer = DataFileWriter::create(file.path()).unwrap();

        let neutrons = vec![
            Neutron::new(1.5, 2.5, 1000, 100, 5, 0),
            Neutron::new(10.3, 20.7, 2000, 200, 8, 1),
        ];

        writer.write_neutrons_csv(&neutrons).unwrap();

        let content = std::fs::read_to_string(file.path()).unwrap();
        assert!(content.contains("x,y,tof,tot,n_hits,chip_id"));
        assert!(content.contains("1.5,2.5,1000,100,5,0"));
        assert!(content.contains("10.3,20.7,2000,200,8,1"));
    }

    #[test]
    fn test_write_neutrons_binary() {
        let file = NamedTempFile::new().unwrap();
        let mut writer = DataFileWriter::create(file.path()).unwrap();

        let neutrons = vec![Neutron::new(1.5, 2.5, 1000, 100, 5, 0)];

        writer.write_neutrons_binary(&neutrons).unwrap();

        let data = std::fs::read(file.path()).unwrap();
        // 8 (f64) + 8 (f64) + 4 (u32) + 2 (u16) + 2 (u16) + 1 (u8) + 3 (reserved) = 28 bytes
        assert_eq!(data.len(), 28);
    }
}
