//! Bounds-checked access to blob bytes. Callers check blob lengths up front, so failures here are bugs.

use std::ops::Range;

use crate::error::{Error, Result};

const OOB: Error = Error::Internal("blob offset out of range");

pub(crate) fn get(d: &[u8], i: usize) -> Result<u8> {
    d.get(i).copied().ok_or(OOB)
}

pub(crate) fn set(d: &mut [u8], i: usize, v: u8) -> Result<()> {
    *d.get_mut(i).ok_or(OOB)? = v;
    Ok(())
}

pub(crate) fn sub(d: &[u8], r: Range<usize>) -> Result<&[u8]> {
    d.get(r).ok_or(OOB)
}

pub(crate) fn sub_mut(d: &mut [u8], r: Range<usize>) -> Result<&mut [u8]> {
    d.get_mut(r).ok_or(OOB)
}

pub(crate) fn arr<const N: usize>(d: &[u8], off: usize) -> Result<[u8; N]> {
    d.get(off..).and_then(|t| t.get(..N)).and_then(|t| t.try_into().ok()).ok_or(OOB)
}

pub(crate) fn put(d: &mut [u8], off: usize, src: &[u8]) -> Result<()> {
    let end = off.checked_add(src.len()).ok_or(OOB)?;
    sub_mut(d, off..end)?.copy_from_slice(src);
    Ok(())
}

pub(crate) fn u16_le(d: &[u8], off: usize) -> Result<u16> {
    arr(d, off).map(u16::from_le_bytes)
}

/// `base + stride * i`
pub(crate) fn offset(base: usize, stride: usize, i: usize) -> Result<usize> {
    stride.checked_mul(i).and_then(|o| o.checked_add(base)).ok_or(OOB)
}

pub(crate) fn hex(b: &[u8]) -> String {
    b.iter().map(|x| format!("{x:02x}")).collect::<Vec<_>>().join(" ")
}
