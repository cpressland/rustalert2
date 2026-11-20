//! Reading data out of a 32-bit Windows executable (PE) by virtual address,
//! for the few tables the game keeps in its code rather than its data files
//! (the voxel normals). Only section mapping is supported; nothing is run.

/// A PE image's sections, mapped by virtual address.
#[derive(Debug, Clone)]
pub struct Pe<'a> {
    data: &'a [u8],
    image_base: u32,
    /// (virtual address, virtual size, file offset, file size)
    sections: Vec<(u32, u32, u32, u32)>,
}

fn u16_at(d: &[u8], o: usize) -> Option<u16> {
    Some(u16::from_le_bytes(d.get(o..o + 2)?.try_into().ok()?))
}

fn u32_at(d: &[u8], o: usize) -> Option<u32> {
    Some(u32::from_le_bytes(d.get(o..o + 4)?.try_into().ok()?))
}

impl<'a> Pe<'a> {
    /// Parse the headers. `None` if this isn't a PE32 image.
    pub fn parse(data: &'a [u8]) -> Option<Pe<'a>> {
        if data.get(..2)? != b"MZ" {
            return None;
        }
        let pe = usize::try_from(u32_at(data, 0x3C)?).ok()?;
        if data.get(pe..pe + 4)? != b"PE\0\0" {
            return None;
        }
        let count = usize::from(u16_at(data, pe + 6)?);
        let optional = usize::from(u16_at(data, pe + 20)?);
        // PE32 (not PE32+): the image base is a u32 at +28 of the optional header.
        if u16_at(data, pe + 24)? != 0x10B {
            return None;
        }
        let image_base = u32_at(data, pe + 24 + 28)?;
        let table = pe + 24 + optional;
        let mut sections = Vec::with_capacity(count.min(96));
        for i in 0..count.min(96) {
            let o = table + 40 * i;
            sections.push((
                u32_at(data, o + 12)?,
                u32_at(data, o + 8)?,
                u32_at(data, o + 20)?,
                u32_at(data, o + 16)?,
            ));
        }
        Some(Pe {
            data,
            image_base,
            sections,
        })
    }

    /// `len` bytes at virtual address `va`, if they're all stored in the
    /// file (uninitialised data isn't).
    pub fn read(&self, va: u32, len: usize) -> Option<&'a [u8]> {
        let rva = va.checked_sub(self.image_base)?;
        let (start, _, raw, raw_size) = *self
            .sections
            .iter()
            .find(|s| rva >= s.0 && rva - s.0 < s.1.max(s.3))?;
        let off = rva - start;
        let end = usize::try_from(off).ok()?.checked_add(len)?;
        if end > usize::try_from(raw_size).ok()? {
            return None;
        }
        let at = usize::try_from(raw.checked_add(off)?).ok()?;
        self.data.get(at..at.checked_add(len)?)
    }

    pub fn u32(&self, va: u32) -> Option<u32> {
        Some(u32::from_le_bytes(self.read(va, 4)?.try_into().ok()?))
    }

    pub fn f32(&self, va: u32) -> Option<f32> {
        Some(f32::from_le_bytes(self.read(va, 4)?.try_into().ok()?))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A minimal PE32 with one section holding `body` at RVA 0x1000.
    fn image(body: &[u8]) -> Vec<u8> {
        let mut d = vec![0u8; 0x200];
        d[..2].copy_from_slice(b"MZ");
        d[0x3C..0x40].copy_from_slice(&0x40u32.to_le_bytes());
        d[0x40..0x44].copy_from_slice(b"PE\0\0");
        d[0x46..0x48].copy_from_slice(&1u16.to_le_bytes());
        d[0x54..0x56].copy_from_slice(&0xE0u16.to_le_bytes());
        d[0x58..0x5A].copy_from_slice(&0x10Bu16.to_le_bytes());
        d[0x58 + 28..0x58 + 32].copy_from_slice(&0x0040_0000u32.to_le_bytes());
        let s = 0x58 + 0xE0;
        d[s..s + 5].copy_from_slice(b".data");
        let size = u32::try_from(body.len()).unwrap();
        d[s + 8..s + 12].copy_from_slice(&(size + 0x100).to_le_bytes());
        d[s + 12..s + 16].copy_from_slice(&0x1000u32.to_le_bytes());
        d[s + 16..s + 20].copy_from_slice(&size.to_le_bytes());
        d[s + 20..s + 24].copy_from_slice(&0x200u32.to_le_bytes());
        d.extend_from_slice(body);
        d
    }

    #[test]
    fn reads_by_virtual_address() {
        let d = image(&[1, 0, 0, 0, 0, 0, 0x80, 0x3F]);
        let pe = Pe::parse(&d).unwrap();
        assert_eq!(pe.u32(0x0040_1000), Some(1));
        assert_eq!(pe.f32(0x0040_1004), Some(1.0));
        assert_eq!(pe.read(0x0040_1006, 4), None, "past the stored bytes");
        assert_eq!(pe.read(0x0030_0000, 1), None, "below the image");
        assert!(Pe::parse(b"MZ").is_none());
    }
}
