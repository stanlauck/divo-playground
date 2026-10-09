// SPDX-License-Identifier: MIT OR Apache-2.0

// A local oracle-facing probe; output is synthetic, not media metadata.
use edl_exchange::{FrameRate, TimecodeMode, format_timecode};
fn main() -> Result<(), edl_exchange::Error> {
    let rates = [
        (FrameRate::new(24_000, 1001), "23.976"),
        (FrameRate::new(24, 1), "24"),
        (FrameRate::new(25, 1), "25"),
        (FrameRate::new(30, 1), "30"),
        (FrameRate::new(30_000, 1001), "29.97"),
    ];
    let mut count = 0;
    for (rate, label) in rates {
        for mode in [TimecodeMode::NonDrop, TimecodeMode::Drop] {
            if mode == TimecodeMode::Drop && label != "29.97" {
                continue;
            }
            let base = rate.timecode_base()?;
            let day = if mode == TimecodeMode::Drop {
                2_589_408
            } else {
                base * 86_400
            };
            let mut frames = vec![
                0,
                1,
                2,
                899,
                1798,
                1799,
                1800,
                3597,
                3598,
                17_981,
                17_982,
                107_891,
                107_892,
                day - 1,
            ];
            // Both sides of every minute, including tenth-minute and hour edges.
            for minute in 1..1440u64 {
                let nominal = minute * 60 * base;
                let edge = if mode == TimecodeMode::Drop {
                    nominal - 2 * (minute - minute / 10)
                } else {
                    nominal
                };
                for delta in -3i64..=3 {
                    frames.push((edge as i64 + delta) as u64);
                }
            }
            let mut seed = 0x6a09_e667_f3bc_c908u64;
            for _ in 0..2000 {
                seed = seed.wrapping_mul(6364136223846793005).wrapping_add(1);
                frames.push(seed % day);
            }
            frames.sort_unstable();
            frames.dedup();
            for frame in frames {
                let tc = format_timecode(frame, rate, mode)?;
                println!(
                    "{label}\t{}\t{frame}\t{tc}",
                    if mode == TimecodeMode::Drop {
                        "drop"
                    } else {
                        "non_drop"
                    }
                );
                count += 1;
            }
        }
    }
    eprintln!("Synthetic timecode vectors: {count}");
    Ok(())
}
