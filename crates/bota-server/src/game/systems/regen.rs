//! Mending health and mana over time.

use bota_proto::Fixed;

use crate::game::{EntityAllocator, Health, Mana, Stats, Table};

/// Adds what each entity mends to its health and mana, up to its maximum.
///
/// Both pools are held finer than a whole point, so mending of less than one a
/// tick adds up rather than falling away. An entity already dead mends
/// neither.
pub fn regenerate(
    entities: &EntityAllocator,
    stats: &Table<Stats>,
    health: &mut Table<Health>,
    mana: &mut Table<Mana>,
) {
    for entity in entities.iter() {
        let Some(stat) = stats.get(entity) else {
            continue;
        };
        // Anything with a health pool is dead at zero; what has none is
        // never dead.
        let standing = match health.get_mut(entity) {
            Some(hp) if hp.hp > Fixed::ZERO => {
                hp.hp = (hp.hp + stat.hp_regen).min(stat.max_hp);
                true
            }
            Some(_) => false,
            None => true,
        };
        if standing && let Some(mp) = mana.get_mut(entity) {
            mp.mana = (mp.mana + stat.mana_regen).min(stat.max_mana);
        }
    }
}
