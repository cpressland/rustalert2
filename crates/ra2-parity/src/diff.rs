//! Comparing two logs under a scenario's checks.

use std::fmt;

use crate::log::{Event, Frame, Log, Object, events};
use crate::scenario::{Check, EventKind, Filter, Scenario};

/// The outcome of one check.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Outcome {
    pub check: String,
    /// What the reference (the original game) shows.
    pub reference: String,
    /// What the candidate (our engine) shows.
    pub candidate: String,
    pub pass: bool,
}

impl fmt::Display for Outcome {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "{} {:<44} reference {:<14} candidate {}",
            if self.pass { "pass" } else { "FAIL" },
            self.check,
            self.reference,
            self.candidate
        )
    }
}

fn matches(f: &Filter, o: &Object) -> bool {
    f.type_id
        .as_ref()
        .is_none_or(|t| t.eq_ignore_ascii_case(&o.type_id))
        && f.owner.is_none_or(|w| w == o.owner)
}

fn label(f: &Filter) -> String {
    format!(
        "{}{}",
        f.type_id.as_deref().unwrap_or("*"),
        f.owner.map(|o| format!("@{o}")).unwrap_or_default()
    )
}

fn selected<'a>(frame: Option<&'a Frame>, f: &Filter) -> Vec<&'a Object> {
    frame
        .map(|fr| fr.objects.iter().filter(|o| matches(f, o)).collect())
        .unwrap_or_default()
}

fn show(v: Option<i64>) -> String {
    v.map_or("-".into(), |v| v.to_string())
}

fn within(a: Option<i64>, b: Option<i64>, tol: u32) -> bool {
    matches!((a, b), (Some(a), Some(b)) if a.abs_diff(b) <= u64::from(tol))
}

/// Pair each object with its nearest unpaired counterpart; return the worst
/// distance in leptons, or `None` if the counts differ.
fn worst_pairing(a: &[&Object], b: &[&Object]) -> Option<u64> {
    if a.len() != b.len() {
        return None;
    }
    let mut free: Vec<&Object> = b.to_vec();
    let mut worst = 0u64;
    for o in a {
        let (i, d) = free
            .iter()
            .enumerate()
            .map(|(i, p)| {
                let (dx, dy, dz) = (
                    i64::from(o.x - p.x),
                    i64::from(o.y - p.y),
                    i64::from(o.z - p.z),
                );
                (i, (dx * dx + dy * dy + dz * dz).unsigned_abs().isqrt())
            })
            .min_by_key(|&(_, d)| d)?;
        worst = worst.max(d);
        free.swap_remove(i);
    }
    Some(worst)
}

fn first_damage(log: &Log, f: &Filter) -> Option<i64> {
    let total = |fr: &Frame| -> i64 { selected(Some(fr), f).iter().map(|o| i64::from(o.hp)).sum() };
    let start = total(log.frames.first()?);
    log.frames
        .iter()
        .find(|fr| total(fr) < start)
        .map(|fr| i64::from(fr.frame))
}

fn first_event(evts: &[Event], f: &Filter, kind: EventKind) -> Option<i64> {
    evts.iter()
        .find_map(|e| {
            let (frame, type_id, owner) = match (e, kind) {
                (
                    Event::Created {
                        frame,
                        type_id,
                        owner,
                        ..
                    },
                    EventKind::Created,
                )
                | (
                    Event::Removed {
                        frame,
                        type_id,
                        owner,
                        ..
                    },
                    EventKind::Removed,
                ) => (*frame, type_id, *owner),
                _ => return None,
            };
            let hit = f
                .type_id
                .as_ref()
                .is_none_or(|t| t.eq_ignore_ascii_case(type_id))
                && f.owner.is_none_or(|w| w == owner);
            hit.then_some(frame)
        })
        .map(i64::from)
}

/// Evaluate every check of a scenario. The candidate may be missing (our
/// engine can't run it yet): then each check reports the reference alone
/// and fails.
#[allow(clippy::too_many_lines)] // one arm per check kind
pub fn compare(s: &Scenario, reference: &Log, candidate: Option<&Log>) -> Vec<Outcome> {
    let empty = Log::default();
    let cand = candidate.unwrap_or(&empty);
    let (ref_events, cand_events) = (events(reference), events(cand));
    s.checks
        .iter()
        .map(|c| match c {
            Check::Count {
                filter,
                at,
                tolerance,
            } => {
                let count = |l: &Log| {
                    l.at(*at)
                        .map(|f| i64::try_from(selected(Some(f), filter).len()).unwrap_or(i64::MAX))
                };
                let (r, k) = (count(reference), candidate.and_then(|_| count(cand)));
                Outcome {
                    check: format!("count {} at {at} (±{tolerance})", label(filter)),
                    reference: show(r),
                    candidate: show(k),
                    pass: within(r, k, *tolerance),
                }
            }
            Check::Credits {
                house,
                at,
                tolerance,
            } => {
                let credits = |l: &Log| {
                    l.at(*at)
                        .and_then(|f| f.houses.iter().find(|h| h.index == *house))
                        .map(|h| i64::from(h.credits))
                };
                let (r, k) = (credits(reference), candidate.and_then(|_| credits(cand)));
                Outcome {
                    check: format!("credits of house {house} at {at} (±{tolerance})"),
                    reference: show(r),
                    candidate: show(k),
                    pass: within(r, k, *tolerance),
                }
            }
            Check::Hp {
                filter,
                at,
                tolerance,
            } => {
                let hp = |l: &Log| {
                    l.at(*at).map(|f| {
                        selected(Some(f), filter)
                            .iter()
                            .map(|o| i64::from(o.hp))
                            .sum()
                    })
                };
                let (r, k) = (hp(reference), candidate.and_then(|_| hp(cand)));
                Outcome {
                    check: format!("hp of {} at {at} (±{tolerance})", label(filter)),
                    reference: show(r),
                    candidate: show(k),
                    pass: within(r, k, *tolerance),
                }
            }
            Check::Position {
                filter,
                at,
                tolerance,
            } => {
                let (r, k) = (
                    selected(reference.at(*at), filter),
                    selected(cand.at(*at), filter),
                );
                let worst = candidate.and_then(|_| worst_pairing(&r, &k));
                Outcome {
                    check: format!(
                        "position of {} at {at} (±{tolerance} leptons)",
                        label(filter)
                    ),
                    reference: format!("{} objects", r.len()),
                    candidate: worst
                        .map_or_else(|| format!("{} objects", k.len()), |w| format!("worst {w}")),
                    pass: worst.is_some_and(|w| w <= u64::from(*tolerance)),
                }
            }
            Check::FirstDamage { filter, window } => {
                let (r, k) = (
                    first_damage(reference, filter),
                    candidate.and_then(|c| first_damage(c, filter)),
                );
                Outcome {
                    check: format!("first damage to {} (±{window} frames)", label(filter)),
                    reference: show(r),
                    candidate: show(k),
                    pass: within(r, k, *window),
                }
            }
            Check::Event {
                filter,
                event,
                window,
            } => {
                let (r, k) = (
                    first_event(&ref_events, filter, *event),
                    candidate.and_then(|_| first_event(&cand_events, filter, *event)),
                );
                Outcome {
                    check: format!("first {event:?} of {} (±{window} frames)", label(filter))
                        .to_lowercase(),
                    reference: show(r),
                    candidate: show(k),
                    pass: within(r, k, *window),
                }
            }
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fmt::Write as _;

    fn obj(id: u32, t: &str, x: i32, hp: i32) -> String {
        format!(
            r#"{{"id":{id},"kind":"Unit","type":"{t}","owner":0,"x":{x},"y":0,"z":0,"hp":{hp}}}"#
        )
    }

    fn log(frames: &[(u32, Vec<String>, i32)]) -> Log {
        let mut text = String::new();
        for (f, objs, credits) in frames {
            writeln!(
                text,
                "{{\"frame\":{f},\"houses\":[{{\"index\":0,\"credits\":{credits}}}],\"objects\":[{}]}}",
                objs.join(",")
            )
            .unwrap();
        }
        Log::read(text.as_bytes()).unwrap()
    }

    #[test]
    fn checks() {
        let s = Scenario::parse(
            r#"
name = "t"
map = "m"
end_frame = 10
[[check]]
what = "count"
type = "MTNK"
at = 5
[[check]]
what = "credits"
house = 0
at = 5
tolerance = 10
[[check]]
what = "position"
type = "MTNK"
at = 5
tolerance = 20
[[check]]
what = "event"
event = "removed"
type = "HTNK"
window = 2
[[check]]
what = "hp"
type = "MTNK"
at = 5
"#,
        )
        .unwrap();
        let reference = log(&[
            (
                1,
                vec![obj(1, "MTNK", 0, 300), obj(2, "HTNK", 900, 600)],
                1000,
            ),
            (5, vec![obj(1, "MTNK", 100, 250)], 990),
        ]);
        let candidate = log(&[
            (
                1,
                vec![obj(5, "MTNK", 0, 300), obj(6, "HTNK", 900, 600)],
                1000,
            ),
            (4, vec![obj(5, "MTNK", 110, 250)], 985),
            (6, vec![], 985),
        ]);
        let out = compare(&s, &reference, Some(&candidate));
        let passes: Vec<bool> = out.iter().map(|o| o.pass).collect();
        assert_eq!(passes, [true, true, true, true, true], "{out:#?}");
        // Missing candidate: everything fails, reference still shown.
        let alone = compare(&s, &reference, None);
        assert!(alone.iter().all(|o| !o.pass));
        assert_eq!(alone[1].reference, "990");
        // A tighter position tolerance fails.
        let mut tight = s.clone();
        tight.checks[2] = Check::Position {
            filter: Filter {
                type_id: Some("MTNK".into()),
                owner: None,
            },
            at: 5,
            tolerance: 5,
        };
        assert!(!compare(&tight, &reference, Some(&candidate))[2].pass);
    }
}
