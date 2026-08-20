//! What a match was worth to the bot that played it.
//!
//! A match ends far more rarely than it is played: two even bots farm for an
//! hour and neither Ancient falls. So the number a search climbs is mostly
//! made of what the seat did — creeps taken, creeps denied, levels, gold —
//! with the win itself worth more than any of it.

use crate::Outcome;

/// What each thing a seat did is worth.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Worth {
    /// Winning the match.
    pub win: f32,
    /// One enemy creep taken.
    pub last_hit: f32,
    /// One of its own put out.
    pub deny: f32,
    /// One enemy hero killed.
    pub kill: f32,
    /// One death of its own.
    pub death: f32,
    /// One hero level.
    pub level: f32,
    /// One gold earned, spent or not.
    pub gold: f32,
    /// One order the server would not take.
    pub reject: f32,
    /// One point of damage dealt to an enemy hero.
    pub hero_damage: f32,
    /// One point of damage dealt to what the other side has built.
    pub structure_damage: f32,
    /// One point of damage taken.
    pub damage_taken: f32,
}

impl Default for Worth {
    fn default() -> Worth {
        Worth {
            win: 200.0,
            last_hit: 1.0,
            deny: 1.0,
            kill: 20.0,
            death: -15.0,
            level: 5.0,
            gold: 0.002,
            reject: -0.01,
            hero_damage: 0.05,
            structure_damage: 0.05,
            damage_taken: -0.02,
        }
    }
}

/// What one match came to, by those weights.
///
/// A bot that never got a seat scores nothing at all, so a run that failed to
/// join is never mistaken for a run that played badly.
pub fn score(out: &Outcome, worth: &Worth) -> f32 {
    let Some(mine) = out.mine.as_ref() else {
        return f32::MIN;
    };
    let won = match (out.winner, out.team) {
        (Some(winner), Some(team)) if winner == team => worth.win,
        (Some(_), Some(_)) => -worth.win,
        _ => 0.0,
    };
    won + f32::from(mine.last_hits) * worth.last_hit
        + f32::from(mine.denies) * worth.deny
        + f32::from(mine.kills) * worth.kill
        + f32::from(mine.deaths) * worth.death
        + f32::from(mine.level) * worth.level
        + earned(out) * worth.gold
        + out.rejected as f32 * worth.reject
        + out.hero_damage as f32 * worth.hero_damage
        + out.structure_damage as f32 * worth.structure_damage
        + out.damage_taken as f32 * worth.damage_taken
}

/// Gold the seat earned over the match: what the final numbers say when the
/// match ran to its end, and what is left unspent when it did not.
fn earned(out: &Outcome) -> f32 {
    let counted = out.stats.as_ref().zip(out.slot).and_then(|(stats, slot)| {
        stats
            .slots
            .iter()
            .find(|row| row.slot == slot)
            .map(|row| row.net_worth)
    });
    match counted {
        Some(net) => net as f32,
        None => out.mine.as_ref().and_then(|mine| mine.gold).unwrap_or(0) as f32,
    }
}
