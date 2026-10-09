// SPDX-License-Identifier: MIT OR Apache-2.0

mod reader;
mod writer;

pub use reader::from_fcpxml;
pub use writer::to_fcpxml;

use crate::{Error, FrameRate, MAX_FRAMES, Result};

/// Metadata key on asset-clips and placeholder gaps.
pub const SHOT_ID_KEY: &str = "shot_list.id";

fn gcd(mut a: u128, mut b: u128) -> u128 {
    while b != 0 {
        (a, b) = (b, a % b);
    }
    a
}

fn time(frames: u64, rate: FrameRate) -> String {
    let n = u128::from(frames) * u128::from(rate.den);
    let d = u128::from(rate.num);
    let factor = gcd(n, d);
    if d / factor == 1 {
        format!("{}s", n / factor)
    } else {
        format!("{}/{}s", n / factor, d / factor)
    }
}

fn rational(text: &str, at: &str) -> Result<(u64, u32)> {
    let text = text.strip_suffix('s').ok_or_else(|| Error::invalid(at))?;
    let (n, d) = text.split_once('/').unwrap_or((text, "1"));
    if n.is_empty()
        || d.is_empty()
        || !n.bytes().all(|b| b.is_ascii_digit())
        || !d.bytes().all(|b| b.is_ascii_digit())
    {
        return Err(Error::invalid(at));
    }
    let n: u64 = n.parse().map_err(|_| Error::invalid(at))?;
    let d: u32 = d.parse().map_err(|_| Error::invalid(at))?;
    if n > i64::MAX as u64 || d == 0 {
        return Err(Error::invalid(at));
    }
    Ok((n, d))
}

fn frames(text: &str, rate: FrameRate, at: &str) -> Result<u64> {
    let (n, d) = rational(text, at)?;
    let n = u128::from(n) * u128::from(rate.num);
    let d = u128::from(d) * u128::from(rate.den);
    if n % d != 0 || n / d > u128::from(MAX_FRAMES) {
        return Err(Error::unsupported(at));
    }
    Ok((n / d) as u64)
}
