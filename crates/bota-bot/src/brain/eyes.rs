//! Reading one snapshot: who is who, how far, and how much of it is aimed at
//! the bot.
//!
//! A [`Sight`] is a snapshot with the bot's own seat and body already picked
//! out of it. Everything the policy asks about the world it asks here, so no
//! decision has to walk the unit list itself.

use bota_proto::{EntityId, PlayerView, SlotId, Team, UnitKind, UnitView, Vec2, WorldView};

use crate::{Params, span};

/// One tick of the world, with the bot's own place in it found.
pub struct Sight<'a> {
    /// The snapshot itself.
    pub view: &'a WorldView,
    /// The bot's row of the scoreboard.
    pub seat: &'a PlayerView,
    /// The body it drives.
    pub me: &'a UnitView,
    /// The side it plays for.
    pub team: Team,
    /// Ticks in a second, as the match runs.
    pub tick_rate: f32,
    /// Ticks before the bot may begin another swing.
    ///
    /// The attack cycle is not on the wire; this is what the bot has worked
    /// out from the blows it has seen itself land.
    pub wait: f32,
}

impl<'a> Sight<'a> {
    /// The world through one seat's eyes, or nothing while that seat has no
    /// body standing.
    pub fn new(view: &'a WorldView, slot: SlotId, tick_rate: f32) -> Option<Sight<'a>> {
        let seat = view.players.iter().find(|player| player.slot == slot)?;
        let id = seat.unit?;
        let me = view.units.iter().find(|unit| unit.id == id)?;
        Some(Sight {
            view,
            seat,
            me,
            team: me.team,
            tick_rate,
            wait: 0.0,
        })
    }

    /// The unit behind a handle, while the snapshot still holds it.
    pub fn unit(&self, id: EntityId) -> Option<&'a UnitView> {
        self.view.units.iter().find(|unit| unit.id == id)
    }

    /// Everything standing that the bot can see, itself apart.
    pub fn others(&self) -> impl Iterator<Item = &'a UnitView> {
        let mine = self.me.id;
        self.view
            .units
            .iter()
            .filter(move |unit| unit.id != mine && unit.hp > 0)
    }

    /// Everything of the other side: their units, and the jungle.
    pub fn foes(&self) -> impl Iterator<Item = &'a UnitView> {
        let mine = self.team;
        self.others().filter(move |unit| unit.team != mine)
    }

    /// Everything of the other side that a swing is worth aiming at.
    pub fn enemies(&self) -> impl Iterator<Item = &'a UnitView> {
        self.foes()
            .filter(|unit| unit.team != Team::Neutral && unit.kind != UnitKind::Ward)
    }

    /// Everything of its own side, itself apart.
    pub fn allies(&self) -> impl Iterator<Item = &'a UnitView> {
        let mine = self.team;
        self.others().filter(move |unit| unit.team == mine)
    }

    /// The enemy heroes it can see.
    pub fn enemy_heroes(&self) -> impl Iterator<Item = &'a UnitView> {
        self.enemies().filter(|unit| unit.kind == UnitKind::Hero)
    }

    /// The creeps of the other side's waves.
    pub fn enemy_creeps(&self) -> impl Iterator<Item = &'a UnitView> {
        self.enemies().filter(|unit| is_wave_creep(unit.kind))
    }

    /// The creeps of its own waves.
    pub fn own_creeps(&self) -> impl Iterator<Item = &'a UnitView> {
        self.allies().filter(|unit| is_wave_creep(unit.kind))
    }

    /// Every building of a side. Both sides see them all, always.
    pub fn buildings(&self, team: Team) -> impl Iterator<Item = &'a UnitView> {
        self.view
            .units
            .iter()
            .filter(move |unit| unit.team == team && is_building(unit.kind) && unit.hp > 0)
    }

    /// The towers of a side that are still standing.
    pub fn towers(&self, team: Team) -> impl Iterator<Item = &'a UnitView> {
        self.buildings(team)
            .filter(|unit| unit.kind == UnitKind::Tower)
    }

    /// Where a side's fountain stands.
    pub fn fountain(&self, team: Team) -> Option<Vec2> {
        self.buildings(team)
            .find(|unit| unit.kind == UnitKind::Fountain)
            .map(|unit| unit.pos)
    }

    /// Where a side's Ancient stands.
    pub fn ancient(&self, team: Team) -> Option<Vec2> {
        self.buildings(team)
            .find(|unit| unit.kind == UnitKind::Ancient)
            .map(|unit| unit.pos)
    }

    /// The side that is not its own.
    pub fn other_side(&self) -> Team {
        match self.team {
            Team::Radiant => Team::Dire,
            Team::Dire => Team::Radiant,
            Team::Neutral => Team::Neutral,
        }
    }

    /// Health left, as a part of its whole.
    pub fn hp_part(&self) -> f32 {
        part(self.me.hp, self.me.max_hp)
    }

    /// Mana left, as a part of its whole.
    pub fn mana_part(&self) -> f32 {
        part(self.me.mana, self.me.max_mana)
    }

    /// How far a spot is from where the bot stands.
    pub fn how_far(&self, to: Vec2) -> f32 {
        span(self.me.pos, to)
    }

    /// The ground between two bodies, edge to edge.
    pub fn gap_to(&self, other: &UnitView) -> f32 {
        span(self.me.pos, other.pos) - self.me.radius.to_f32() - other.radius.to_f32()
    }

    /// How far the bot has to swing to touch a body, edge to edge.
    pub fn reach(&self) -> f32 {
        self.me.attack_range.to_f32()
    }

    /// Whether a body stands within a swing.
    pub fn in_reach(&self, other: &UnitView) -> bool {
        self.gap_to(other) <= self.reach()
    }

    /// Damage a tick that the bot is standing in right now.
    ///
    /// Everything of another side whose own reach covers where the bot stands
    /// counts, after the armor the bot carries.
    pub fn under_fire(&self, params: &Params) -> f32 {
        self.foes()
            .filter(|unit| unit.attack_damage > 0 && unit.attack_interval > 0)
            .filter(|unit| self.gap_to(unit) <= unit.attack_range.to_f32())
            .map(|unit| {
                crate::after_armor(unit.attack_damage as f32, self.me.armor.to_f32(), params)
                    / unit.attack_interval as f32
            })
            .sum()
    }
}

/// Whether a kind is one of the creeps a lane wave is made of.
pub fn is_wave_creep(kind: UnitKind) -> bool {
    matches!(
        kind,
        UnitKind::CreepMelee
            | UnitKind::CreepFlagbearer
            | UnitKind::CreepRanged
            | UnitKind::CreepSiege
    )
}

/// Whether a kind is something that is built rather than walks.
pub fn is_building(kind: UnitKind) -> bool {
    matches!(
        kind,
        UnitKind::Tower | UnitKind::Ancient | UnitKind::Fountain
    )
}

/// One number as a part of another, and zero when there is no whole.
pub fn part(some: i32, whole: i32) -> f32 {
    if whole <= 0 {
        return 0.0;
    }
    some.max(0) as f32 / whole as f32
}
