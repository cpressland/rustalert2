//! Observation logs: what a game holds at each frame, from either engine.
//!
//! A log is JSON lines, one per observed frame. The recomp's observer
//! (`parity/recomp/observe.c`) writes it from the original game's memory;
//! `ra2-sim` will write the same shape. See `docs/spec/parity.md`.

use std::collections::BTreeMap;
use std::io::{self, BufRead};

use serde::{Deserialize, Serialize};

/// One house (player or AI or neutral) at one frame.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct House {
    /// Its index in the game's house list.
    pub index: u32,
    /// Its country, e.g. `Americans`.
    #[serde(default)]
    pub country: String,
    pub credits: i32,
    #[serde(default)]
    pub power_output: i32,
    #[serde(default)]
    pub power_drain: i32,
    #[serde(default)]
    pub defeated: bool,
    /// Superweapons it has: type index, charged (0/1), on hold (0/1),
    /// frames left to charge.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub supers: Vec<[i32; 4]>,
}

/// One unit, infantry, aircraft or building at one frame.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Object {
    /// An identity stable for the object's life within one log (the original
    /// uses its address); not comparable across engines.
    pub id: u32,
    /// `Unit`, `Infantry`, `Aircraft` or `Building`.
    pub kind: String,
    /// The rules ID, e.g. `MTNK`.
    #[serde(rename = "type")]
    pub type_id: String,
    /// The owning house's index.
    pub owner: u32,
    /// Position in leptons (256 per cell).
    pub x: i32,
    pub y: i32,
    pub z: i32,
    /// Hit points.
    pub hp: i32,
    /// Bails of ore or gems carried, when any.
    #[serde(default, skip_serializing_if = "is_zero")]
    pub cargo: i32,
}

#[allow(clippy::trivially_copy_pass_by_ref)]
fn is_zero(v: &i32) -> bool {
    *v == 0
}

/// Everything observed at one frame.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Frame {
    pub frame: u32,
    #[serde(default)]
    pub houses: Vec<House>,
    #[serde(default)]
    pub objects: Vec<Object>,
    /// Every cell holding ore or gems: x, y, overlay, and the overlay's
    /// data (bails left, less one).
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub ore: Vec<[i32; 4]>,
}

/// A whole log, frames in order.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Log {
    pub frames: Vec<Frame>,
}

impl Log {
    /// Read JSON lines. Blank lines and lines starting with `#` are skipped.
    pub fn read(r: impl BufRead) -> io::Result<Log> {
        let mut frames = Vec::new();
        for (n, line) in r.lines().enumerate() {
            let line = line?;
            let t = line.trim();
            if t.is_empty() || t.starts_with('#') {
                continue;
            }
            let f: Frame = serde_json::from_str(t).map_err(|e| {
                io::Error::new(io::ErrorKind::InvalidData, format!("line {}: {e}", n + 1))
            })?;
            frames.push(f);
        }
        Ok(Log { frames })
    }

    /// Write JSON lines, as [`Log::read`] reads them.
    ///
    /// # Errors
    /// If writing fails.
    pub fn write(&self, mut w: impl io::Write) -> io::Result<()> {
        for f in &self.frames {
            serde_json::to_writer(&mut w, f).map_err(io::Error::other)?;
            w.write_all(b"\n")?;
        }
        Ok(())
    }

    /// The last observed frame at or before `frame`.
    pub fn at(&self, frame: u32) -> Option<&Frame> {
        match self.frames.binary_search_by_key(&frame, |f| f.frame) {
            Ok(i) => Some(&self.frames[i]),
            Err(0) => None,
            Err(i) => Some(&self.frames[i - 1]),
        }
    }

    /// The first frame at which a predicate holds.
    pub fn first(&self, pred: impl Fn(&Frame) -> bool) -> Option<u32> {
        self.frames.iter().find(|f| pred(f)).map(|f| f.frame)
    }
}

/// What changed between two consecutive observations.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Event {
    /// An object appeared (built, trained, unloaded, created).
    Created {
        frame: u32,
        kind: String,
        type_id: String,
        owner: u32,
    },
    /// An object went away (destroyed, sold, entered something).
    Removed {
        frame: u32,
        kind: String,
        type_id: String,
        owner: u32,
        last_hp: i32,
    },
}

/// Derive creation and removal events from a log's snapshots.
pub fn events(log: &Log) -> Vec<Event> {
    let mut out = Vec::new();
    let mut prev: BTreeMap<u32, &Object> = BTreeMap::new();
    for (i, f) in log.frames.iter().enumerate() {
        let now: BTreeMap<u32, &Object> = f.objects.iter().map(|o| (o.id, o)).collect();
        if i > 0 {
            for (id, o) in &now {
                if !prev.contains_key(id) {
                    out.push(Event::Created {
                        frame: f.frame,
                        kind: o.kind.clone(),
                        type_id: o.type_id.clone(),
                        owner: o.owner,
                    });
                }
            }
            for (id, o) in &prev {
                if !now.contains_key(id) {
                    out.push(Event::Removed {
                        frame: f.frame,
                        kind: o.kind.clone(),
                        type_id: o.type_id.clone(),
                        owner: o.owner,
                        last_hp: o.hp,
                    });
                }
            }
        }
        prev = now;
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    const LOG: &str = r#"
# a comment
{"frame":1,"houses":[{"index":0,"country":"Americans","credits":10000}],"objects":[{"id":7,"kind":"Unit","type":"MTNK","owner":0,"x":256,"y":512,"z":0,"hp":300}]}
{"frame":3,"objects":[{"id":7,"kind":"Unit","type":"MTNK","owner":0,"x":300,"y":512,"z":0,"hp":250},{"id":9,"kind":"Infantry","type":"E1","owner":0,"x":0,"y":0,"z":0,"hp":125}]}
{"frame":5,"objects":[{"id":9,"kind":"Infantry","type":"E1","owner":0,"x":0,"y":0,"z":0,"hp":125}]}
"#;

    #[test]
    fn read_and_query() {
        let log = Log::read(LOG.as_bytes()).unwrap();
        assert_eq!(log.frames.len(), 3);
        assert_eq!(log.at(2).unwrap().frame, 1);
        assert_eq!(log.at(3).unwrap().frame, 3);
        assert!(log.at(0).is_none());
        assert_eq!(log.frames[0].houses[0].credits, 10000);
        assert_eq!(log.first(|f| f.objects.len() == 2), Some(3));
    }

    #[test]
    fn derived_events() {
        let log = Log::read(LOG.as_bytes()).unwrap();
        let e = events(&log);
        assert_eq!(
            e,
            vec![
                Event::Created {
                    frame: 3,
                    kind: "Infantry".into(),
                    type_id: "E1".into(),
                    owner: 0
                },
                Event::Removed {
                    frame: 5,
                    kind: "Unit".into(),
                    type_id: "MTNK".into(),
                    owner: 0,
                    last_hp: 250
                },
            ]
        );
    }

    #[test]
    fn bad_line_is_reported() {
        let err = Log::read("{\"frame\":1}\nnot json\n".as_bytes()).unwrap_err();
        assert!(err.to_string().contains("line 2"));
    }
}
