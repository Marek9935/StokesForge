// SPDX-License-Identifier: GPL-3.0-or-later
//
// Port of the object framing of liborigin 3.0.3 (OriginAnyParser.cpp,
// https://sourceforge.net/projects/liborigin/):
//   SPDX-FileCopyrightText: 2017 Miquel Garriga <gbmiquel@gmail.com>
//   SPDX-FileCopyrightText: 2021-2024 Stefan Gerlach <stefan.gerlach@uni.kn>
//   SPDX-License-Identifier: GPL-3.0-or-later
// (and, through it, the work of Alex Kargovsky, Ion Vasilief and Knut Franke).

//! The object stream of an Origin project: `u32 LE size`, `0x0A`, `size`
//! bytes, `0x0A`; a size of 0 (no payload, no second delimiter) ends a list.
//! Every read is checked (invariant 10): a defect is an `Err`, never a
//! panic, and nothing is allocated from a size the file declares.

use super::OriginError;
use crate::decode::windows_1252;

pub(crate) struct Blocks<'a> {
    bytes: &'a [u8],
    pos: usize,
}

impl<'a> Blocks<'a> {
    pub(crate) fn new(bytes: &'a [u8], pos: usize) -> Self {
        Self { bytes, pos }
    }

    /// Reads `u32 size` + `0x0A`.
    pub(crate) fn size(&mut self) -> Result<usize, OriginError> {
        let at = self.pos;
        let end = at
            .checked_add(5)
            .ok_or(OriginError::Truncated { offset: at })?;
        let raw = self
            .bytes
            .get(at..end)
            .ok_or(OriginError::Truncated { offset: at })?;
        let [a, b, c, d, delimiter] = *raw else {
            return Err(OriginError::Truncated { offset: at });
        };
        if delimiter != 0x0A {
            return Err(OriginError::BadDelimiter { offset: end - 1 });
        }
        self.pos = end;
        Ok(u32::from_le_bytes([a, b, c, d]) as usize)
    }

    /// Reads `n` bytes and their closing `0x0A`; nothing for `n == 0`.
    pub(crate) fn blob(&mut self, n: usize) -> Result<&'a [u8], OriginError> {
        if n == 0 {
            return Ok(&[]);
        }
        let at = self.pos;
        let out_of_range = OriginError::SizeOutOfRange {
            offset: at,
            size: n,
        };
        let end = at.checked_add(n).ok_or(out_of_range.clone())?;
        let data = self.bytes.get(at..end).ok_or(out_of_range)?;
        match self.bytes.get(end) {
            Some(0x0A) => {}
            Some(_) => return Err(OriginError::BadDelimiter { offset: end }),
            None => return Err(OriginError::Truncated { offset: end }),
        }
        self.pos = end + 1;
        Ok(data)
    }

    /// A size then the object it announces; `None` for the end mark.
    pub(crate) fn object(&mut self) -> Result<Option<&'a [u8]>, OriginError> {
        match self.size()? {
            0 => Ok(None),
            n => self.blob(n).map(Some),
        }
    }

    /// Skips objects up to and including the end mark. Each object consumes
    /// at least five bytes, so the loop is bounded by the file size.
    pub(crate) fn skip_list(&mut self) -> Result<(), OriginError> {
        while self.object()?.is_some() {}
        Ok(())
    }
}

/// NUL-terminated text at `off`, at most `max` bytes, as Windows-1252.
pub(crate) fn cstr(bytes: &[u8], off: usize, max: usize) -> String {
    let Some(rest) = bytes.get(off..) else {
        return String::new();
    };
    let rest = rest.get(..max).unwrap_or(rest);
    let end = rest.iter().position(|&c| c == 0).unwrap_or(rest.len());
    rest[..end].iter().map(|&b| windows_1252(b)).collect()
}

pub(crate) fn le_u16(bytes: &[u8], off: usize) -> Option<u16> {
    let s = bytes.get(off..off.checked_add(2)?)?;
    Some(u16::from_le_bytes(s.try_into().ok()?))
}

pub(crate) fn le_i32(bytes: &[u8], off: usize) -> Option<i32> {
    let s = bytes.get(off..off.checked_add(4)?)?;
    Some(i32::from_le_bytes(s.try_into().ok()?))
}

pub(crate) fn le_f64(bytes: &[u8], off: usize) -> Option<f64> {
    let s = bytes.get(off..off.checked_add(8)?)?;
    Some(f64::from_le_bytes(s.try_into().ok()?))
}
