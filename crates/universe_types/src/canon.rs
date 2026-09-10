//! Canonical little-endian binary encoding (ch. 33.4).

use crate::error::NumericFault;

pub const MAGIC_RULES: [u8; 4] = *b"EUR1";
pub const MAGIC_STATE: [u8; 4] = *b"EUS1";
pub const SCHEMA_VERSION: u32 = 1;

#[derive(Clone, Default)]
pub struct CanonWriter {
    buf: Vec<u8>,
}

impl CanonWriter {
    pub fn new() -> Self {
        Self { buf: Vec::new() }
    }

    pub fn with_magic_rules() -> Self {
        let mut w = Self::new();
        w.raw(&MAGIC_RULES);
        w.u32(SCHEMA_VERSION);
        w
    }

    pub fn with_magic_state() -> Self {
        let mut w = Self::new();
        w.raw(&MAGIC_STATE);
        w.u32(SCHEMA_VERSION);
        w
    }

    pub fn raw(&mut self, bytes: &[u8]) {
        self.buf.extend_from_slice(bytes);
    }

    pub fn u8(&mut self, v: u8) {
        self.buf.push(v);
    }

    pub fn bool(&mut self, v: bool) {
        self.buf.push(u8::from(v));
    }

    pub fn u16(&mut self, v: u16) {
        self.buf.extend_from_slice(&v.to_le_bytes());
    }

    pub fn u32(&mut self, v: u32) {
        self.buf.extend_from_slice(&v.to_le_bytes());
    }

    pub fn i32(&mut self, v: i32) {
        self.buf.extend_from_slice(&v.to_le_bytes());
    }

    pub fn i8(&mut self, v: i8) {
        self.buf.push(v as u8);
    }

    pub fn u64(&mut self, v: u64) {
        self.buf.extend_from_slice(&v.to_le_bytes());
    }

    pub fn u128(&mut self, v: u128) {
        self.buf.extend_from_slice(&v.to_le_bytes());
    }

    pub fn bytes32(&mut self, v: &[u8; 32]) {
        self.buf.extend_from_slice(v);
    }

    /// Presence byte then payload if `Some`.
    pub fn option_u64(&mut self, v: Option<u64>) {
        match v {
            None => self.u8(0),
            Some(x) => {
                self.u8(1);
                self.u64(x);
            }
        }
    }

    pub fn f64_canonical(&mut self, v: f64) -> Result<(), NumericFault> {
        let bits = canonicalize_f64(v)?;
        self.buf.extend_from_slice(&bits.to_le_bytes());
        Ok(())
    }

    pub fn f64_slice(&mut self, xs: &[f64]) -> Result<(), NumericFault> {
        self.u64(xs.len() as u64);
        for &x in xs {
            self.f64_canonical(x)?;
        }
        Ok(())
    }

    pub fn u64_slice(&mut self, xs: &[u64]) {
        self.u64(xs.len() as u64);
        for &x in xs {
            self.u64(x);
        }
    }

    pub fn u16_slice(&mut self, xs: &[u16]) {
        self.u64(xs.len() as u64);
        for &x in xs {
            self.u16(x);
        }
    }

    pub fn i32_slice(&mut self, xs: &[i32]) {
        self.u64(xs.len() as u64);
        for &x in xs {
            self.i32(x);
        }
    }

    pub fn into_inner(self) -> Vec<u8> {
        self.buf
    }

    pub fn as_slice(&self) -> &[u8] {
        &self.buf
    }
}

/// Reject non-finite values; map -0.0 to +0.0.
pub fn canonicalize_f64(v: f64) -> Result<u64, NumericFault> {
    if !v.is_finite() {
        return Err(NumericFault::NonFinite);
    }
    let bits = v.to_bits();
    if bits == (-0.0f64).to_bits() {
        return Ok(0.0f64.to_bits());
    }
    Ok(bits)
}

pub fn f64_from_canonical_bits(bits: u64) -> Result<f64, NumericFault> {
    let v = f64::from_bits(bits);
    if !v.is_finite() {
        return Err(NumericFault::NonFinite);
    }
    if bits == (-0.0f64).to_bits() {
        return Ok(0.0);
    }
    Ok(v)
}

#[derive(Clone)]
pub struct CanonReader<'a> {
    buf: &'a [u8],
    pos: usize,
}

impl<'a> CanonReader<'a> {
    pub fn new(buf: &'a [u8]) -> Self {
        Self { buf, pos: 0 }
    }

    fn take(&mut self, n: usize) -> Result<&'a [u8], NumericFault> {
        if self.pos + n > self.buf.len() {
            return Err(NumericFault::Overflow("canonical_read"));
        }
        let s = &self.buf[self.pos..self.pos + n];
        self.pos += n;
        Ok(s)
    }

    pub fn expect_magic(&mut self, magic: &[u8; 4]) -> Result<(), NumericFault> {
        let got = self.take(4)?;
        if got != magic {
            return Err(NumericFault::Overflow("bad_magic"));
        }
        let ver = self.u32()?;
        if ver != SCHEMA_VERSION {
            return Err(NumericFault::Overflow("schema_version"));
        }
        Ok(())
    }

    pub fn u8(&mut self) -> Result<u8, NumericFault> {
        Ok(self.take(1)?[0])
    }

    pub fn bool(&mut self) -> Result<bool, NumericFault> {
        Ok(self.u8()? != 0)
    }

    pub fn u16(&mut self) -> Result<u16, NumericFault> {
        let b = self.take(2)?;
        Ok(u16::from_le_bytes([b[0], b[1]]))
    }

    pub fn u32(&mut self) -> Result<u32, NumericFault> {
        let b = self.take(4)?;
        Ok(u32::from_le_bytes(b.try_into().unwrap()))
    }

    pub fn i32(&mut self) -> Result<i32, NumericFault> {
        Ok(self.u32()? as i32)
    }

    pub fn i8(&mut self) -> Result<i8, NumericFault> {
        Ok(self.u8()? as i8)
    }

    pub fn u64(&mut self) -> Result<u64, NumericFault> {
        let b = self.take(8)?;
        Ok(u64::from_le_bytes(b.try_into().unwrap()))
    }

    pub fn u128(&mut self) -> Result<u128, NumericFault> {
        let b = self.take(16)?;
        Ok(u128::from_le_bytes(b.try_into().unwrap()))
    }

    pub fn bytes32(&mut self) -> Result<[u8; 32], NumericFault> {
        let b = self.take(32)?;
        let mut a = [0u8; 32];
        a.copy_from_slice(b);
        Ok(a)
    }

    pub fn f64_canonical(&mut self) -> Result<f64, NumericFault> {
        f64_from_canonical_bits(self.u64()?)
    }

    pub fn option_u64(&mut self) -> Result<Option<u64>, NumericFault> {
        match self.u8()? {
            0 => Ok(None),
            1 => Ok(Some(self.u64()?)),
            _ => Err(NumericFault::Overflow("option_tag")),
        }
    }

    pub fn remaining(&self) -> usize {
        self.buf.len().saturating_sub(self.pos)
    }

    pub fn finish(self) -> Result<(), NumericFault> {
        if self.pos != self.buf.len() {
            return Err(NumericFault::Overflow("trailing_bytes"));
        }
        Ok(())
    }
}
