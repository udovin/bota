//! Projection of the world onto the wire.

use bota_proto::{PlayerView, ProjectileView, StatusFlags, Team, UnitView, WorldView};

use crate::sim::{Unit, World};

impl World {
    /// The world through one team's fog of war.
    pub fn view(&self, team: Team) -> WorldView {
        self.project(Some(team))
    }

    /// The whole world, for spectators and the replay.
    pub fn view_full(&self) -> WorldView {
        self.project(None)
    }

    fn project(&self, viewer: Option<Team>) -> WorldView {
        let units = self
            .units
            .iter()
            .filter(|(_, u)| match viewer {
                None => true,
                Some(team) => u.team == team || self.can_see_point(team, u.pos),
            })
            .map(|(id, u)| {
                let carried_slots =
                    crate::sim::rules::INVENTORY_SLOTS + crate::sim::rules::BACKPACK_SLOTS;
                let seat = u.owner.and_then(|slot| self.seat(slot));
                let carried = seat
                    .map(|s| crate::sim::item_views(&s.items[..carried_slots]))
                    .unwrap_or_default();
                let abilities = seat
                    .map(|s| crate::sim::ability_views(&s.abilities))
                    .unwrap_or_default();
                project_unit(id, u, carried, abilities)
            })
            .collect();
        let projectiles = self
            .projectiles
            .iter()
            .filter(|(_, p)| match viewer {
                None => true,
                Some(team) => p.team == team || self.can_see_point(team, p.pos),
            })
            .map(|(id, p)| ProjectileView {
                id,
                pos: p.pos,
                facing: p.facing,
                team: p.team,
                ability: p.ability,
            })
            .collect();
        let players = self
            .seats
            .iter()
            .map(|s| PlayerView {
                slot: s.slot,
                team: s.team,
                hero: s.hero,
                unit: s.unit,
                level: s.level,
                xp: s.xp,
                gold: match viewer {
                    None => Some(s.gold),
                    Some(team) if team == s.team => Some(s.gold),
                    Some(_) => None,
                },
                stash: match viewer {
                    Some(team) if team != s.team => None,
                    _ => Some(crate::sim::item_views(
                        &s.items[crate::sim::rules::INVENTORY_SLOTS
                            + crate::sim::rules::BACKPACK_SLOTS..],
                    )),
                },
                kills: s.kills,
                deaths: s.deaths,
                assists: s.assists,
                last_hits: s.last_hits,
                denies: s.denies,
                respawn_left: s.respawn_left,
            })
            .collect();
        WorldView {
            tick: self.tick,
            viewer,
            units,
            projectiles,
            players,
        }
    }
}

fn project_unit(
    id: bota_proto::EntityId,
    u: &Unit,
    items: Vec<Option<bota_proto::ItemView>>,
    abilities: Vec<bota_proto::AbilityView>,
) -> UnitView {
    UnitView {
        id,
        kind: u.kind,
        team: u.team,
        pos: u.pos,
        facing: u.facing,
        hp: u.hp,
        max_hp: u.max_hp,
        mana: u.mana,
        max_mana: u.max_mana,
        move_speed: u.move_speed,
        attack_damage: u.attack_damage,
        attack_range: u.attack_range,
        attack_interval: u.current_attack_interval(),
        armor: bota_proto::Fixed::from_int(u.armor),
        magic_resist: bota_proto::Fixed::from_ratio(u.magic_resist_pct, 100),
        radius: u.radius,
        vision_radius: u.vision_radius,
        statuses: StatusFlags::default(),
        hero: u.hero,
        owner: u.owner,
        level: u.level,
        abilities,
        items,
        effects: unit_effects(u),
    }
}

/// The timed effects of a unit, in a fixed order: frenzy, salve, clarity.
fn unit_effects(u: &Unit) -> Vec<bota_proto::EffectView> {
    let mut out = Vec::new();
    for (id, ticks) in [
        (0u16, u.frenzy_ticks),
        (1, u.salve_ticks),
        (2, u.clarity_ticks),
    ] {
        if ticks > 0 {
            out.push(bota_proto::EffectView {
                id: bota_proto::EffectId(id),
                ticks_left: ticks,
            });
        }
    }
    out
}
