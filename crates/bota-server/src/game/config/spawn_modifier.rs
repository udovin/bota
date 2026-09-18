//! Modifiers trusted match setup puts on units as they are stood up.

use bota_proto::{EntityId, MAX_MODIFIER_TICKS, ModifierSpec, Team, UnitKind};

use crate::game::is_structure;

/// The most rules one match setup may carry.
pub const MAX_SPAWN_MODIFIERS: usize = 64;

/// One rule: what to put on which spawns, and for how long.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SpawnModifier {
    /// Which spawns it lands on.
    pub select: SpawnSelector,
    /// What it changes.
    pub spec: ModifierSpec,
    /// How long each application lasts.
    pub duration: ModifierDuration,
}

impl SpawnModifier {
    /// Whether the rule is one a match setup may carry.
    pub fn is_valid(&self) -> bool {
        check(self).is_ok()
    }
}

/// Which units one spawn modifier lands on.
///
/// An absent field takes everything that field could name; the fields narrow
/// one another.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct SpawnSelector {
    /// Which side it takes. `None` takes either side.
    pub team: Option<Team>,
    /// Which kinds it takes. Empty takes every kind.
    pub targets: Vec<SpawnTarget>,
    /// One exact unit handle. When present it is the only unit taken, and it
    /// does not follow a body to a respawn.
    pub unit: Option<EntityId>,
}

impl SpawnSelector {
    /// Whether a standing unit is one this selector takes.
    pub fn takes(&self, kind: UnitKind, team: Option<Team>, unit: EntityId) -> bool {
        if let Some(named) = self.unit
            && named != unit
        {
            return false;
        }
        if let Some(wanted) = self.team
            && team != Some(wanted)
        {
            return false;
        }
        self.targets.is_empty() || self.targets.iter().any(|target| target.takes(kind))
    }
}

/// One kind or category of unit a spawn modifier can name.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SpawnTarget {
    /// Every unit of one exact kind.
    Kind(UnitKind),
    /// Every unit in a category.
    Category(SpawnCategory),
}

impl SpawnTarget {
    /// Whether a kind is the one this target names.
    pub fn takes(&self, kind: UnitKind) -> bool {
        match self {
            SpawnTarget::Kind(exact) => *exact == kind,
            SpawnTarget::Category(category) => category.takes(kind),
        }
    }
}

/// A group of unit kinds a spawn modifier can name at once.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SpawnCategory {
    /// Every hero.
    Hero,
    /// Lane creeps: melee, flagbearer, ranged and siege.
    LaneCreep,
    /// Neutral camp creeps.
    NeutralCreep,
    /// Buildings: towers, ancients, barracks and fountains.
    Structure,
    /// Observer and sentry wards.
    Ward,
    /// Couriers.
    Courier,
}

impl SpawnCategory {
    /// Whether a kind belongs to this category.
    pub fn takes(&self, kind: UnitKind) -> bool {
        match self {
            SpawnCategory::Hero => kind == UnitKind::Hero,
            SpawnCategory::LaneCreep => matches!(
                kind,
                UnitKind::CreepMelee
                    | UnitKind::CreepFlagbearer
                    | UnitKind::CreepRanged
                    | UnitKind::CreepSiege
            ),
            SpawnCategory::NeutralCreep => kind == UnitKind::CreepNeutral,
            SpawnCategory::Structure => is_structure(kind),
            SpawnCategory::Ward => kind == UnitKind::Ward,
            SpawnCategory::Courier => kind == UnitKind::Courier,
        }
    }
}

/// How long one application lasts.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ModifierDuration {
    /// Until the body falls. Every spawn gets it afresh.
    MatchLong,
    /// A number of ticks, counted from each application.
    Ticks(u32),
}

impl ModifierDuration {
    /// What each application starts its countdown with. Absent runs until
    /// the body falls.
    pub fn ticks_left(self) -> Option<u32> {
        match self {
            ModifierDuration::MatchLong => None,
            ModifierDuration::Ticks(ticks) => Some(ticks),
        }
    }

    /// Whether the policy is one a match setup may carry.
    pub fn is_valid(self) -> bool {
        match self {
            ModifierDuration::MatchLong => true,
            ModifierDuration::Ticks(ticks) => ticks > 0 && ticks <= MAX_MODIFIER_TICKS,
        }
    }
}

/// Why one spawn modifier was refused.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SpawnModifierError {
    /// The spec does not lie within its bounds.
    UnboundedSpec,
    /// The duration is not one a match may carry.
    BadDuration,
}

impl core::fmt::Display for SpawnModifierError {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        match self {
            SpawnModifierError::UnboundedSpec => write!(f, "the modifier spec is out of bounds"),
            SpawnModifierError::BadDuration => write!(f, "the duration is out of range"),
        }
    }
}

impl std::error::Error for SpawnModifierError {}

/// Why a rule is not one a match setup may carry, or nothing when it is.
pub fn check(rule: &SpawnModifier) -> Result<(), SpawnModifierError> {
    if !rule.spec.is_bounded() {
        return Err(SpawnModifierError::UnboundedSpec);
    }
    if !rule.duration.is_valid() {
        return Err(SpawnModifierError::BadDuration);
    }
    Ok(())
}
