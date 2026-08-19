//! Taking blows off the health they landed on.

use bota_proto::{DamageKind, Fixed, Team, Vec2};

use crate::engine::{Entity, EntityAllocator, Health, Hit, Stats, Table, Transform};
use crate::sim::rules;

/// One blow once it has been felt.
///
/// What the world does with it afterwards — the event it sends, the bounty it
/// pays — is not this system's business.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Landed {
    /// Who dealt it, while that one still stands.
    pub source: Option<Entity>,
    /// Who took it.
    pub target: Entity,
    /// After armor and resistance.
    pub amount: i32,
    /// Which reduction applied.
    pub kind: DamageKind,
    /// Where it happened.
    pub at: Vec2,
    /// The side that took it.
    pub side: Team,
    /// Whether it brought the target down.
    pub fatal: bool,
}

/// What resolving blows reads and writes.
pub struct HitCx<'a> {
    /// Which entities exist, and where a spent blow is given up.
    pub entities: &'a mut EntityAllocator,
    /// The blows waiting to be felt.
    pub hit: &'a mut Table<Hit>,
    /// Where each entity stands.
    pub transform: &'a Table<Transform>,
    /// Which side each entity is on.
    pub team: &'a Table<Team>,
    /// Armor and resistance.
    pub stats: &'a Table<Stats>,
    /// What the blow comes off.
    pub health: &'a mut Table<Health>,
}

/// Takes every waiting blow off the health it landed on.
///
/// A blow at something already down, or at something damage passes by, is
/// given up unfelt. Every blow is given up either way: none survives the tick
/// that resolves it.
pub fn hitting_system(cx: HitCx<'_>) -> Vec<Landed> {
    let HitCx {
        entities,
        hit,
        transform,
        team,
        stats,
        health,
    } = cx;
    let mut felt = Vec::new();
    for entity in entities.iter().collect::<Vec<_>>() {
        let Some(blow) = hit.remove(entity) else {
            continue;
        };
        entities.free(entity);
        let standing = health
            .get(blow.target)
            .is_some_and(|health| health.hp > Fixed::ZERO);
        let Some(stat) = stats.get(blow.target).copied() else {
            continue;
        };
        if !standing || stat.invulnerable {
            continue;
        }
        let taken = mitigate(blow.amount, blow.kind, stat.armor, stat.magic_resist_pct);
        let Some(pool) = health.get_mut(blow.target) else {
            continue;
        };
        let applied = taken.min(pool.hp.to_int().max(0) + 1);
        pool.hp -= Fixed::from_int(applied);
        let fatal = pool.hp <= Fixed::ZERO;
        felt.push(Landed {
            source: blow.source,
            target: blow.target,
            amount: applied,
            kind: blow.kind,
            at: transform.get(blow.target).map_or(Vec2::ZERO, |t| t.pos),
            side: team.get(blow.target).copied().unwrap_or(Team::Neutral),
            fatal,
        });
    }
    felt
}

/// Damage after armor or magic resistance.
fn mitigate(amount: i32, kind: DamageKind, armor: i32, magic_resist_pct: i32) -> i32 {
    match kind {
        DamageKind::Physical => {
            let den = 100 + rules::ARMOR_SCALE * armor.max(0);
            (i64::from(amount) * 100 / i64::from(den)) as i32
        }
        DamageKind::Magical => {
            let kept = (100 - magic_resist_pct).clamp(0, 100);
            (i64::from(amount) * i64::from(kept) / 100) as i32
        }
        DamageKind::Pure => amount,
    }
}
