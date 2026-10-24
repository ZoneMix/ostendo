//! A ZIP writer with just what an Office package needs: deflated entries
//! with UTF-8 names, no ZIP64.

use std::io::Write;

use anyhow::{Context, Result};
use flate2::write::DeflateEncoder;
use flate2::Compression;

pub(crate) struct Zip {
    out: Vec<u8>,
    central: Vec<u8>,
    entries: u16,
}

/// Entries carry no timestamps: the DOS epoch, 1980-01-01 00:00.
const DOS_DATE: u16 = (1 << 5) | 1;
/// Bit 11: names are UTF-8.
const FLAGS: u16 = 1 << 11;
const DEFLATE: u16 = 8;
const VERSION: u16 = 20;

impl Zip {
    pub fn new() -> Self {
        Self {
            out: Vec::new(),
            central: Vec::new(),
            entries: 0,
        }
    }

    pub fn add(&mut self, name: &str, data: &[u8]) -> Result<()> {
        let mut encoder = DeflateEncoder::new(Vec::new(), Compression::default());
        encoder.write_all(data)?;
        let deflated = encoder.finish()?;
        let too_big = || format!("{name} is too large for the package");
        let offset = u32::try_from(self.out.len()).with_context(too_big)?;
        let size = u32::try_from(data.len()).with_context(too_big)?;
        let packed = u32::try_from(deflated.len()).with_context(too_big)?;
        let name_len = u16::try_from(name.len()).with_context(too_big)?;
        let crc = crc32fast::hash(data);

        // Fields shared by the local header and the central directory entry.
        let mut common = Vec::new();
        for v in [VERSION, FLAGS, DEFLATE, 0, DOS_DATE] {
            common.extend(v.to_le_bytes());
        }
        for v in [crc, packed, size] {
            common.extend(v.to_le_bytes());
        }
        common.extend(name_len.to_le_bytes());
        common.extend(0u16.to_le_bytes()); // extra field length

        self.out.extend(0x0403_4b50_u32.to_le_bytes());
        self.out.extend(&common);
        self.out.extend(name.as_bytes());
        self.out.extend(deflated);

        self.central.extend(0x0201_4b50_u32.to_le_bytes());
        self.central.extend(VERSION.to_le_bytes()); // made by
        self.central.extend(&common);
        // Comment length, disk number, internal and external attributes.
        self.central.extend([0u8; 10]);
        self.central.extend(offset.to_le_bytes());
        self.central.extend(name.as_bytes());
        self.entries = self.entries.checked_add(1).context("too many files")?;
        Ok(())
    }

    pub fn finish(mut self) -> Result<Vec<u8>> {
        let offset = u32::try_from(self.out.len()).context("package too large")?;
        let size = u32::try_from(self.central.len()).context("package too large")?;
        self.out.append(&mut self.central);
        self.out.extend(0x0605_4b50_u32.to_le_bytes());
        self.out.extend([0u8; 4]); // this disk, disk with the directory
        self.out.extend(self.entries.to_le_bytes());
        self.out.extend(self.entries.to_le_bytes());
        self.out.extend(size.to_le_bytes());
        self.out.extend(offset.to_le_bytes());
        self.out.extend(0u16.to_le_bytes()); // comment length
        Ok(self.out)
    }
}
