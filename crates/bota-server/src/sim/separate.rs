//! Bodies that ended a tick inside one another are eased apart.

use bota_proto::{EntityId, Fixed, Vec2};

use crate::sim::{Arena, PassGrid, Unit, clamp_to_map, isqrt64, rules};

/// Eases apart every pair of bodies whose hulls overlap.
///
/// A body is moved at most [`rules::SEPARATION_STEP`] units in a tick and
/// never into a closed cell. A structure never moves: the whole correction
/// falls on whatever walked into it. Two bodies at the same point part along
/// the x axis.
pub fn push_apart(units: &mut Arena<Unit>, grid: &PassGrid) {
    let bodies: Vec<(EntityId, Vec2, i64, bool)> = units
        .iter()
        .map(|(id, u)| (id, u.pos, i64::from(u.radius.raw), u.is_structure()))
        .collect();
    let cap = i64::from(rules::units(rules::SEPARATION_STEP).raw);
    let mut push = vec![(0i64, 0i64); bodies.len()];
    for i in 0..bodies.len() {
        for j in i + 1..bodies.len() {
            let (a, b) = (&bodies[i], &bodies[j]);
            if a.3 && b.3 {
                continue;
            }
            let dx = i64::from(b.1.x.raw) - i64::from(a.1.x.raw);
            let dy = i64::from(b.1.y.raw) - i64::from(a.1.y.raw);
            let min = a.2 + b.2;
            let d2 = dx * dx + dy * dy;
            if d2 >= min * min {
                continue;
            }
            let d = isqrt64(d2);
            let (ux, uy, len) = if d == 0 { (1, 0, 1) } else { (dx, dy, d) };
            let gap = min - d;
            let (share_a, share_b) = match (a.3, b.3) {
                (true, _) => (0, gap),
                (_, true) => (gap, 0),
                _ => (gap / 2, gap / 2),
            };
            push[i].0 -= ux * share_a.min(cap) / len;
            push[i].1 -= uy * share_a.min(cap) / len;
            push[j].0 += ux * share_b.min(cap) / len;
            push[j].1 += uy * share_b.min(cap) / len;
        }
    }
    for (i, (id, pos, _, structure)) in bodies.iter().enumerate() {
        let (mut dx, mut dy) = push[i];
        if *structure || (dx == 0 && dy == 0) {
            continue;
        }
        // However many bodies pressed on it, one tick moves it one step.
        let len = isqrt64(dx * dx + dy * dy);
        if len > cap {
            dx = dx * cap / len;
            dy = dy * cap / len;
        }
        let next = clamp_to_map(Vec2 {
            x: Fixed {
                raw: pos.x.raw.saturating_add(dx as i32),
            },
            y: Fixed {
                raw: pos.y.raw.saturating_add(dy as i32),
            },
        });
        if !grid.walkable(next) && grid.walkable(*pos) {
            continue;
        }
        if let Some(unit) = units.get_mut(*id) {
            unit.pos = next;
        }
    }
}
