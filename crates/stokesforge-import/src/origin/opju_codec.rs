// SPDX-License-Identifier: GPL-3.0-or-later
//
// The FPC parameters, the segment grammar and the value tags used with this
// codec were first documented by the Apache-2.0 project quantized
// (https://github.com/pquarterman17/quantized, docs/origin_project_format.md,
// (c) 2026 Paige Quarterman). Facts only: no code or text of it is copied.
// FPC itself is a published algorithm (Burtscher and Ratanaworabhan, IEEE
// Transactions on Computers, 2009). See docs/license-and-extensions.md.

//! Byte-level primitives of the `.opju` container: a checked cursor, LEB128
//! varints, zigzag integers, the zero-run code of "packed" objects, LZ4
//! blocks and the FPC double codec.
//!
//! - **Varint**: LEB128, at most 10 bytes. **Zigzag**: `(v >> 1) ^ -(v & 1)`.
//! - **Legacy object**: `u32 LE size`, `0x0A`, payload, `0x0A` (nothing more
//!   after the first `0x0A` when the size is 0), the `.opj` framing.
//! - **Blob**: `LEB n`, `n` bytes, `0x0A` when `n > 0`. **Text line**:
//!   `LEB n`, `n` bytes, `0x0A` always.
//! - **Packed object**: `0x80`, `LEB L`, `L` bytes of zero-run code. Control
//!   byte `c < 0x80`: `c` literal bytes follow; `c >= 0x80`: a run of
//!   `(c & 0x3F) + 3` bytes of value 0, or of the next byte when `c & 0x40`.
//!   A packed object decodes to the bytes of the matching `.opj` header.
//! - **LZ4 value**: `0x81`, `LEB dsz`, `LEB csz`, `csz` bytes of an LZ4
//!   *block* (no frame) that decompress to exactly `dsz` bytes.
//! - **List end**: the six bytes `00 00 00 00 00 0A`.
//! - **FPC**: two 4096-entry tables, 4-bit codes packed two per control byte
//!   (low nibble first); bit 3 selects the DFCM prediction, bits 0-2 the
//!   residual byte count `0,1,2,3,5,6,7,8`.
//!
//! Robustness (invariant 10): every read is checked, a defect is an `Err`,
//! never a panic. Nothing is allocated from a declared size: the zero-run
//! output is capped at [`MAX_PACKED`], an LZ4 output never exceeds its
//! declared size (itself capped at [`MAX_LZ4`]) and grows as it is written,
//! and an FPC count larger than twice the remaining bytes is refused before
//! decoding. The FPC tables are reused between segments and "cleared" by a
//! generation counter, so a segment costs time proportional to its values.

use super::OriginError;
use crate::decode::windows_1252;

/// Largest decoded size of one packed object.
pub(super) const MAX_PACKED: usize = 1 << 20;
/// Largest decompressed size accepted for one LZ4 block.
pub(super) const MAX_LZ4: usize = 16 << 20;
/// Closes every list of the object stream.
pub(super) const LIST_END: [u8; 6] = [0, 0, 0, 0, 0, 0x0A];

/// A checked reader over the bytes of the file.
pub(super) struct Cursor<'a> {
    bytes: &'a [u8],
    pos: usize,
}

impl<'a> Cursor<'a> {
    pub(super) fn new(bytes: &'a [u8], pos: usize) -> Self {
        Self { bytes, pos }
    }

    pub(super) fn pos(&self) -> usize {
        self.pos
    }

    pub(super) fn set_pos(&mut self, pos: usize) {
        self.pos = pos;
    }

    /// The next `n` bytes without consuming them, if they are all there.
    pub(super) fn peek_bytes(&self, n: usize) -> Option<&'a [u8]> {
        self.bytes.get(self.pos..self.pos.checked_add(n)?)
    }

    /// The byte `ahead` bytes after the cursor.
    pub(super) fn peek_at(&self, ahead: usize) -> Option<u8> {
        self.bytes.get(self.pos.checked_add(ahead)?).copied()
    }

    pub(super) fn unsupported(&self, what: &'static str) -> OriginError {
        OriginError::OpjuUnsupported {
            offset: self.pos,
            what,
        }
    }

    pub(super) fn unsupported_at(offset: usize, what: &'static str) -> OriginError {
        OriginError::OpjuUnsupported { offset, what }
    }

    pub(super) fn truncated(&self) -> OriginError {
        OriginError::Truncated { offset: self.pos }
    }

    pub(super) fn u8(&mut self) -> Result<u8, OriginError> {
        let b = self.peek()?;
        self.pos += 1;
        Ok(b)
    }

    pub(super) fn peek(&self) -> Result<u8, OriginError> {
        self.bytes
            .get(self.pos)
            .copied()
            .ok_or_else(|| self.truncated())
    }

    pub(super) fn take(&mut self, n: usize) -> Result<&'a [u8], OriginError> {
        let end = self.pos.checked_add(n).ok_or_else(|| self.truncated())?;
        let data = self
            .bytes
            .get(self.pos..end)
            .ok_or_else(|| self.truncated())?;
        self.pos = end;
        Ok(data)
    }

    /// LEB128, at most 10 bytes.
    pub(super) fn leb(&mut self) -> Result<u64, OriginError> {
        let mut value = 0u64;
        for index in 0..10u32 {
            let b = self.u8()?;
            let low = u64::from(b & 0x7F);
            if index == 9 && low > 1 {
                return Err(self.unsupported("varint overflows 64 bits"));
            }
            value |= low << (7 * index);
            if b & 0x80 == 0 {
                return Ok(value);
            }
        }
        Err(self.unsupported("varint longer than 10 bytes"))
    }

    /// A varint used as a count or a size.
    pub(super) fn leb_usize(&mut self) -> Result<usize, OriginError> {
        let v = self.leb()?;
        usize::try_from(v).map_err(|_| self.unsupported("count does not fit in memory"))
    }

    pub(super) fn zigzag(&mut self) -> Result<i64, OriginError> {
        let v = self.leb()?;
        Ok(((v >> 1) as i64) ^ -((v & 1) as i64))
    }

    pub(super) fn newline(&mut self) -> Result<(), OriginError> {
        if self.u8()? != 0x0A {
            self.pos -= 1;
            return Err(self.unsupported("object delimiter expected"));
        }
        Ok(())
    }

    pub(super) fn expect(
        &mut self,
        expected: &[u8],
        what: &'static str,
    ) -> Result<(), OriginError> {
        let at = self.pos;
        if self.take(expected.len())? != expected {
            return Err(Self::unsupported_at(at, what));
        }
        Ok(())
    }

    pub(super) fn at_list_end(&self) -> bool {
        self.bytes
            .get(self.pos..)
            .is_some_and(|rest| rest.starts_with(&LIST_END))
    }

    pub(super) fn list_end(&mut self) -> Result<(), OriginError> {
        if !self.at_list_end() {
            return Err(self.unsupported("list end expected"));
        }
        self.pos += LIST_END.len();
        Ok(())
    }

    /// `u32 size`, `0x0A`, payload and `0x0A` (nothing more for size 0).
    pub(super) fn legacy(&mut self) -> Result<&'a [u8], OriginError> {
        let raw = self.take(4)?;
        let size = raw
            .try_into()
            .map(u32::from_le_bytes)
            .map_err(|_| self.truncated())?;
        self.newline()?;
        if size == 0 {
            return Ok(&[]);
        }
        let data = self.take(size as usize)?;
        self.newline()?;
        Ok(data)
    }

    /// `LEB n`, `n` bytes, `0x0A` when `n > 0`.
    pub(super) fn blob(&mut self) -> Result<&'a [u8], OriginError> {
        let n = self.leb_usize()?;
        let data = self.take(n)?;
        if n > 0 {
            self.newline()?;
        }
        Ok(data)
    }

    /// `LEB n`, `n` bytes, `0x0A` always.
    pub(super) fn text_line(&mut self) -> Result<&'a [u8], OriginError> {
        let n = self.leb_usize()?;
        let data = self.take(n)?;
        self.newline()?;
        Ok(data)
    }

    /// `0x80`, `LEB L`, `L` bytes of zero-run code; returns the decoded bytes.
    pub(super) fn packed(&mut self) -> Result<Vec<u8>, OriginError> {
        let at = self.pos;
        if self.u8()? != 0x80 {
            return Err(Self::unsupported_at(at, "packed object expected"));
        }
        let n = self.leb_usize()?;
        let code = self.take(n)?;
        unzero_run(code).map_err(|what| Self::unsupported_at(at, what))
    }

    /// `0x81`, `LEB dsz`, `LEB csz`, `csz` bytes of LZ4 block.
    pub(super) fn lz4_value(&mut self) -> Result<Vec<u8>, OriginError> {
        let at = self.pos;
        if self.u8()? != 0x81 {
            return Err(Self::unsupported_at(at, "LZ4 value expected"));
        }
        let dsz = self.leb_usize()?;
        let csz = self.leb_usize()?;
        let src = self.take(csz)?;
        lz4_block(src, dsz).map_err(|what| Self::unsupported_at(at, what))
    }

    /// Decodes `count` doubles of one FPC segment (as bit patterns).
    pub(super) fn fpc(
        &mut self,
        fpc: &mut Fpc,
        count: usize,
        emit: impl FnMut(u64),
    ) -> Result<(), OriginError> {
        let at = self.pos;
        fpc.decode(count, self.bytes, &mut self.pos, emit)
            .map_err(|what| Self::unsupported_at(at, what))
    }
}

/// Decodes the zero-run code of a packed object.
pub(super) fn unzero_run(src: &[u8]) -> Result<Vec<u8>, &'static str> {
    let mut out: Vec<u8> = Vec::new();
    let mut i = 0;
    while let Some(&control) = src.get(i) {
        i += 1;
        if control & 0x80 != 0 {
            let run = usize::from(control & 0x3F) + 3;
            let fill = if control & 0x40 != 0 {
                let f = *src.get(i).ok_or("zero-run fill byte missing")?;
                i += 1;
                f
            } else {
                0
            };
            if out.len() + run > MAX_PACKED {
                return Err("packed object too large");
            }
            out.resize(out.len() + run, fill);
        } else {
            let n = usize::from(control);
            let literal = src.get(i..i + n).ok_or("zero-run literal truncated")?;
            if out.len() + n > MAX_PACKED {
                return Err("packed object too large");
            }
            out.extend_from_slice(literal);
            i += n;
        }
    }
    Ok(out)
}

/// An LZ4 length extension: bytes of 255 until one below.
fn lz4_extension(src: &[u8], i: &mut usize) -> Result<usize, &'static str> {
    let mut total = 0usize;
    loop {
        let b = *src.get(*i).ok_or("LZ4 length truncated")?;
        *i += 1;
        total += usize::from(b);
        if b != 255 {
            return Ok(total);
        }
    }
}

/// Decompresses one LZ4 block to exactly `dsz` bytes.
pub(super) fn lz4_block(src: &[u8], dsz: usize) -> Result<Vec<u8>, &'static str> {
    if dsz > MAX_LZ4 {
        return Err("LZ4 block too large");
    }
    let mut out: Vec<u8> = Vec::new();
    let mut i = 0;
    loop {
        let token = *src.get(i).ok_or("LZ4 block truncated")?;
        i += 1;
        let mut literal = usize::from(token >> 4);
        if literal == 15 {
            literal += lz4_extension(src, &mut i)?;
        }
        let end = i.checked_add(literal).ok_or("LZ4 literal overflows")?;
        let bytes = src.get(i..end).ok_or("LZ4 literal truncated")?;
        if out.len() + literal > dsz {
            return Err("LZ4 output exceeds its declared size");
        }
        out.extend_from_slice(bytes);
        i = end;
        if i == src.len() {
            break;
        }
        let offset = src.get(i..i + 2).ok_or("LZ4 offset truncated")?;
        let offset = usize::from(u16::from_le_bytes([offset[0], offset[1]]));
        i += 2;
        let mut length = usize::from(token & 15);
        if length == 15 {
            length += lz4_extension(src, &mut i)?;
        }
        length += 4;
        if offset == 0 || offset > out.len() || out.len() + length > dsz {
            return Err("LZ4 match out of range");
        }
        let start = out.len() - offset;
        if offset >= length {
            out.extend_from_within(start..start + length);
        } else {
            for k in 0..length {
                let b = *out.get(start + k).ok_or("LZ4 match out of range")?;
                out.push(b);
            }
        }
    }
    if out.len() != dsz {
        return Err("LZ4 output shorter than declared");
    }
    Ok(out)
}

/// UTF-8 text of an `.opju` string, cut at the first NUL. The files seen
/// are ASCII; a string that is not valid UTF-8 is read as Windows-1252, the
/// code page the `.opj` format uses (not observed in an `.opju`).
pub(super) fn text(bytes: &[u8]) -> String {
    let end = bytes.iter().position(|&b| b == 0).unwrap_or(bytes.len());
    let bytes = bytes.get(..end).unwrap_or(bytes);
    match std::str::from_utf8(bytes) {
        Ok(s) => s.to_string(),
        Err(_) => bytes.iter().map(|&b| windows_1252(b)).collect(),
    }
}

const TABLE: usize = 4096;

/// FPC decoder state: two tables of `(generation, value)` slots. A slot of
/// an older generation reads as 0, so starting a segment is O(1).
pub(super) struct Fpc {
    fcm: Vec<(u32, u64)>,
    dfcm: Vec<(u32, u64)>,
    generation: u32,
}

impl Fpc {
    pub(super) fn new() -> Self {
        Self {
            fcm: vec![(0, 0); TABLE],
            dfcm: vec![(0, 0); TABLE],
            generation: 0,
        }
    }

    fn begin(&mut self) -> u32 {
        if self.generation == u32::MAX {
            self.fcm.fill((0, 0));
            self.dfcm.fill((0, 0));
            self.generation = 0;
        }
        self.generation += 1;
        self.generation
    }

    /// Decodes `count` values from `input` at `*pos`, with a fresh
    /// predictor state, and passes their bit patterns to `emit`.
    fn decode(
        &mut self,
        count: usize,
        input: &[u8],
        pos: &mut usize,
        mut emit: impl FnMut(u64),
    ) -> Result<(), &'static str> {
        let remaining = input.len().saturating_sub(*pos);
        if count > remaining.saturating_mul(2) {
            return Err("FPC value count exceeds the data");
        }
        let generation = self.begin();
        let read = |table: &[(u32, u64)], index: usize| match table.get(index) {
            Some(&(g, v)) if g == generation => v,
            _ => 0,
        };
        let (mut fh, mut dh, mut last) = (0usize, 0usize, 0u64);
        let mut done = 0;
        while done < count {
            let control = *input.get(*pos).ok_or("FPC data truncated")?;
            *pos += 1;
            for nibble in [control & 0x0F, control >> 4] {
                let code = usize::from(nibble & 7);
                let width = if code < 4 { code } else { code + 1 };
                let end = *pos + width;
                let bytes = input.get(*pos..end).ok_or("FPC data truncated")?;
                *pos = end;
                let mut buf = [0u8; 8];
                buf.get_mut(..width)
                    .ok_or("FPC residual width")?
                    .copy_from_slice(bytes);
                let residual = u64::from_le_bytes(buf);
                let predicted = if nibble & 8 != 0 {
                    last.wrapping_add(read(&self.dfcm, dh))
                } else {
                    read(&self.fcm, fh)
                };
                let value = predicted ^ residual;
                let stride = value.wrapping_sub(last);
                if let Some(slot) = self.fcm.get_mut(fh) {
                    *slot = (generation, value);
                }
                if let Some(slot) = self.dfcm.get_mut(dh) {
                    *slot = (generation, stride);
                }
                fh = ((fh << 6) ^ (value >> 48) as usize) & (TABLE - 1);
                dh = ((dh << 2) ^ (stride >> 40) as usize) & (TABLE - 1);
                last = value;
                emit(value);
                done += 1;
                if done == count {
                    break;
                }
            }
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn varints_and_zigzag() {
        let mut c = Cursor::new(&[0x00, 0x7F, 0x80, 0x01, 0xAC, 0x02], 0);
        assert_eq!(c.leb().unwrap(), 0);
        assert_eq!(c.leb().unwrap(), 127);
        assert_eq!(c.leb().unwrap(), 128);
        assert_eq!(c.leb().unwrap(), 300);
        let mut c = Cursor::new(&[0, 1, 2, 3, 4, 5], 0);
        let got: Vec<_> = (0..6).map(|_| c.zigzag().unwrap()).collect();
        assert_eq!(got, [0, -1, 1, -2, 2, -3]);
    }

    #[test]
    fn a_varint_of_eleven_bytes_or_overflowing_is_an_error() {
        let long = [0x80u8; 11];
        assert!(Cursor::new(&long, 0).leb().is_err());
        let mut max = vec![0xFFu8; 9];
        max.push(0x01);
        assert_eq!(Cursor::new(&max, 0).leb().unwrap(), u64::MAX);
        let mut over = vec![0xFFu8; 9];
        over.push(0x02);
        assert!(Cursor::new(&over, 0).leb().is_err());
    }

    #[test]
    fn zero_run_literals_zero_runs_and_fill_runs() {
        // 2 literals, 3 zeros, 5 times 0xFF, 1 literal.
        let code = [2, 7, 8, 0x80, 0xC2, 0xFF, 1, 9];
        let got = unzero_run(&code).unwrap();
        assert_eq!(got, [7, 8, 0, 0, 0, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 9]);
        // The longest run is 66 bytes.
        assert_eq!(unzero_run(&[0xBF]).unwrap().len(), 66);
        assert!(unzero_run(&[3, 1, 2]).is_err());
        assert!(unzero_run(&[0xC0]).is_err());
    }

    #[test]
    fn zero_run_output_is_capped() {
        let bomb = vec![0xBF; MAX_PACKED / 66 + 2];
        assert!(unzero_run(&bomb).is_err());
    }

    #[test]
    fn lz4_literal_block_and_overlapping_match() {
        // Literal-only block.
        assert_eq!(lz4_block(&[0x30, b'a', b'b', b'c'], 3).unwrap(), b"abc");
        // "abc" then a match of 6 at offset 3 (overlapping copy not needed),
        // then the closing empty literal sequence.
        let block = [0x32, b'a', b'b', b'c', 3, 0, 0x00];
        assert_eq!(lz4_block(&block, 9).unwrap(), b"abcabcabc");
        // "a" then a match of 7 at offset 1 overlaps itself.
        let block = [0x13, b'a', 1, 0, 0x00];
        assert_eq!(lz4_block(&block, 8).unwrap(), b"aaaaaaaa");
        // A literal length extension (15 + 5 = 20).
        let mut block = vec![0xF0, 5];
        block.extend_from_slice(&[7u8; 20]);
        assert_eq!(lz4_block(&block, 20).unwrap(), vec![7u8; 20]);
    }

    #[test]
    fn lz4_defects_are_errors() {
        let block = [0x32, b'a', b'b', b'c', 3, 0, 0x00];
        assert!(lz4_block(&block, 10).is_err(), "declared size too long");
        assert!(lz4_block(&block, 8).is_err(), "declared size too short");
        assert!(lz4_block(&[0x13, b'a', 0, 0, 0x00], 8).is_err(), "offset 0");
        assert!(
            lz4_block(&[0x13, b'a', 2, 0, 0x00], 8).is_err(),
            "offset beyond"
        );
        assert!(lz4_block(&[0x30, b'a'], 3).is_err(), "literal truncated");
        assert!(lz4_block(&[0x30, b'a', b'b', b'c'], MAX_LZ4 + 1).is_err());
        assert!(lz4_block(&[], 0).is_err());
    }

    #[test]
    fn fpc_residuals_of_every_width() {
        // One segment of three values. Codes: width 1 (code 1), width 2
        // (code 2), width 5 (code 4); control bytes 0x21 then 0x04. The FCM
        // prediction is the table slot of the previous value's hash, which is
        // slot 0 each time here: 0, then 0x07, then 0x1233.
        let data = [0x21, 0x07, 0x34, 0x12, 0x04, 1, 2, 3, 4, 5];
        let mut got = Vec::new();
        let mut pos = 0;
        Fpc::new()
            .decode(3, &data, &mut pos, |v| got.push(v))
            .unwrap();
        assert_eq!(
            got,
            [0x07, 0x07 ^ 0x1234, (0x07 ^ 0x1234) ^ 0x05_04_03_02_01]
        );
        assert_eq!(pos, data.len());
    }

    #[test]
    fn fpc_count_larger_than_the_data_is_refused_before_decoding() {
        let mut pos = 0;
        let r = Fpc::new().decode(100, &[0, 0], &mut pos, |_| panic!("emitted"));
        assert!(r.is_err());
    }

    #[test]
    fn fpc_state_is_fresh_for_each_segment() {
        let mut fpc = Fpc::new();
        for _ in 0..3 {
            let mut got = Vec::new();
            let mut pos = 0;
            fpc.decode(2, &[0x11, 9, 9], &mut pos, |v| got.push(v))
                .unwrap();
            assert_eq!(
                got,
                [9, 0],
                "a stale table would predict 9 for the first value"
            );
        }
    }

    #[test]
    fn text_is_utf8_then_windows_1252_and_stops_at_nul() {
        assert_eq!(text(b"ab\0cd"), "ab");
        assert_eq!(text("\u{B5}m".as_bytes()), "\u{B5}m");
        assert_eq!(text(&[0xB5, b'm']), "\u{B5}m");
    }
}
