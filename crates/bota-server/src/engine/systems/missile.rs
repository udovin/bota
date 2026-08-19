//! Missiles in the air, and what they leave behind when they arrive.

use bota_proto::{Fixed, Team};

use crate::engine::{EntityAllocator, Health, Hit, Projectile, Table, Transform, Visibility};
use crate::sim::{facing_towards, move_towards, per_tick};

/// What flight reads and writes.
pub struct MissileCx<'a> {
    /// Which entities exist, where a missile is given up and a blow is made.
    pub entities: &'a mut EntityAllocator,
    /// The missiles themselves.
    pub projectile: &'a mut Table<Projectile>,
    /// Where everything stands, missiles included.
    pub transform: &'a mut Table<Transform>,
    /// Which side each entity is on.
    pub team: &'a mut Table<Team>,
    /// Who sees what, so a new blow carries nothing to see.
    pub visibility: &'a mut Table<Visibility>,
    /// Health, for telling whether a missile still has anybody to reach.
    pub health: &'a Table<Health>,
    /// Where an arriving missile leaves its blow.
    pub hit: &'a mut Table<Hit>,
}

/// Moves every missile along and turns the ones that arrive into blows.
///
/// A missile whose target has fallen is given up in the air: nothing it was
/// aimed at is left to reach.
pub fn missile_system(cx: MissileCx<'_>) {
    let MissileCx {
        entities,
        projectile,
        transform,
        team,
        visibility,
        health,
        hit,
    } = cx;
    for missile in entities.iter().collect::<Vec<_>>() {
        let Some(shot) = projectile.get(missile).cloned() else {
            continue;
        };
        let standing = health
            .get(shot.target)
            .is_some_and(|health| health.hp > Fixed::ZERO);
        let to = transform.get(shot.target).map(|t| t.pos);
        let (true, Some(to)) = (standing, to) else {
            give_up(missile, entities, projectile, transform, team, visibility);
            continue;
        };
        let Some(at) = transform.get_mut(missile) else {
            continue;
        };
        let next = move_towards(at.pos, to, per_tick(shot.speed));
        at.pos = next;
        at.facing = facing_towards(next, to);
        if next != to {
            continue;
        }
        let blow = entities.alloc();
        hit.insert(
            blow,
            Hit {
                source: shot.source,
                target: shot.target,
                amount: shot.damage,
                kind: shot.kind,
                crit: shot.crit,
            },
        );
        give_up(missile, entities, projectile, transform, team, visibility);
    }
}

/// Takes a missile out of the air and out of the world.
fn give_up(
    missile: crate::engine::Entity,
    entities: &mut EntityAllocator,
    projectile: &mut Table<Projectile>,
    transform: &mut Table<Transform>,
    team: &mut Table<Team>,
    visibility: &mut Table<Visibility>,
) {
    projectile.remove(missile);
    transform.remove(missile);
    team.remove(missile);
    visibility.remove(missile);
    entities.free(missile);
}
