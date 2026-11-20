//! `ra2tool audio`: decode sounds, music and the sound bank.

use std::io;
use std::path::{Path, PathBuf};

use ra2_formats::audio::{Sound, aud, bag_index, bag_sound, wav};
use ra2_vfs::install::Game;

use crate::pal::{read, vfs};

/// How loud and how clipped a sound is: a cheap check that a decode is sane.
fn stats(s: &Sound) -> (f64, usize) {
    #[allow(clippy::cast_precision_loss)]
    let rms = if s.samples.is_empty() {
        0.0
    } else {
        (s.samples
            .iter()
            .map(|&v| f64::from(v) * f64::from(v))
            .sum::<f64>()
            / s.samples.len() as f64)
            .sqrt()
    };
    (
        rms,
        s.samples
            .iter()
            .filter(|&&v| v == i16::MAX || v == i16::MIN)
            .count(),
    )
}

/// Decode every WAV and AUD in the mounted game, and every sound in each
/// `audio.bag`.
pub(crate) fn check(game: Game, game_dir: Option<PathBuf>) -> io::Result<bool> {
    let vfs = vfs(game, game_dir)?;
    let (mut wavs, mut auds, mut bags, mut seconds) = (0usize, 0usize, 0usize, 0f64);
    let (mut samples, mut clipped) = (0usize, 0usize);
    let mut failures = Vec::new();
    let mut add = |s: &Sound| {
        #[allow(clippy::cast_precision_loss)]
        {
            seconds += s.frames() as f64 / f64::from(s.rate);
        }
        samples += s.samples.len();
        clipped += stats(s).1;
    };
    for m in vfs.mounts() {
        let mut idx = None;
        let mut bag = None;
        for e in &m.archive.index().entries {
            let Ok(data) = m.archive.read(*e) else {
                continue;
            };
            if data.starts_with(b"RIFF") {
                match wav(&data) {
                    Ok(s) => {
                        wavs += 1;
                        add(&s);
                    }
                    Err(err) => failures.push(format!("{} {:08X}: {err}", m.name, e.id)),
                }
            } else if data.get(16..20) == Some(&[0xaf, 0xde, 0, 0][..]) {
                match aud(&data) {
                    Ok(s) => {
                        auds += 1;
                        add(&s);
                    }
                    Err(err) => failures.push(format!("{} {:08X}: {err}", m.name, e.id)),
                }
            } else if data.starts_with(b"GABA") {
                idx = Some(data);
            } else if e.id == ra2_formats::mix::mix_id("audio.bag") {
                bag = Some(data);
            }
        }
        if let (Some(idx), Some(bag)) = (idx, bag) {
            let entries = bag_index(&idx)
                .map_err(|e| io::Error::new(io::ErrorKind::InvalidData, e.to_string()))?;
            for e in &entries {
                match bag_sound(&bag, e) {
                    Ok(s) => {
                        bags += 1;
                        add(&s);
                    }
                    Err(err) => failures.push(format!("{} bag {}: {err}", m.name, e.name)),
                }
            }
        }
    }
    #[allow(clippy::cast_precision_loss)]
    let clip_pct = 100.0 * clipped as f64 / samples.max(1) as f64;
    println!(
        "{wavs} WAVs, {auds} AUDs and {bags} bank sounds decode: {:.0} minutes, {clip_pct:.4}% of samples at full scale; {} failed",
        seconds / 60.0,
        failures.len()
    );
    for f in failures.iter().take(20) {
        println!("  {f}");
    }
    Ok(failures.is_empty())
}

/// Write a sound as a 16-bit PCM WAV: a file by name, or a bank sound by
/// its `audio.idx` name.
pub(crate) fn export(
    game: Game,
    game_dir: Option<PathBuf>,
    name: &str,
    out: &Path,
) -> io::Result<bool> {
    let vfs = vfs(game, game_dir)?;
    let bad =
        |e: ra2_formats::audio::Error| io::Error::new(io::ErrorKind::InvalidData, e.to_string());
    let sound = if let Some(data) = vfs.read(name)? {
        if data.starts_with(b"RIFF") {
            wav(&data).map_err(bad)?
        } else {
            aud(&data).map_err(bad)?
        }
    } else {
        let entries = bag_index(&read(&vfs, "audio.idx")?).map_err(bad)?;
        let e = entries
            .iter()
            .find(|e| e.name.eq_ignore_ascii_case(name))
            .ok_or_else(|| {
                io::Error::new(
                    io::ErrorKind::NotFound,
                    format!("{name}: no such file or bank sound"),
                )
            })?;
        bag_sound(&read(&vfs, "audio.bag")?, e).map_err(bad)?
    };
    let bytes = u32::try_from(sound.samples.len() * 2).map_err(io::Error::other)?;
    let mut w = Vec::with_capacity(44 + sound.samples.len() * 2);
    w.extend_from_slice(b"RIFF");
    w.extend_from_slice(&(36 + bytes).to_le_bytes());
    w.extend_from_slice(b"WAVEfmt ");
    w.extend_from_slice(&16u32.to_le_bytes());
    w.extend_from_slice(&1u16.to_le_bytes());
    w.extend_from_slice(&sound.channels.to_le_bytes());
    w.extend_from_slice(&sound.rate.to_le_bytes());
    w.extend_from_slice(&(sound.rate * u32::from(sound.channels) * 2).to_le_bytes());
    w.extend_from_slice(&(sound.channels * 2).to_le_bytes());
    w.extend_from_slice(&16u16.to_le_bytes());
    w.extend_from_slice(b"data");
    w.extend_from_slice(&bytes.to_le_bytes());
    for s in &sound.samples {
        w.extend_from_slice(&s.to_le_bytes());
    }
    std::fs::write(out, w)?;
    let (rms, clipped) = stats(&sound);
    #[allow(clippy::cast_precision_loss)]
    let secs = sound.frames() as f64 / f64::from(sound.rate);
    println!(
        "{name}: {} Hz, {} ch, {secs:.2} s, RMS {rms:.0}, {clipped} clipped; wrote {}",
        sound.rate,
        sound.channels,
        out.display()
    );
    Ok(true)
}
