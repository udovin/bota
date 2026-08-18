//! Ability engine and the Sylla kit.

use bota_proto::{AbilitySlot, EventKind, Order, OrderTarget, RejectReason, SlotId, Team, Vec2};

use super::fixtures::{hero_id, world};
use crate::sim::{Command, Event, Unit, World, rules};

fn cmd(slot: u8, order: Order) -> Command {
    Command {
        slot: SlotId(slot),
        order,
    }
}

fn cast(slot: u8, ability: u8, target: OrderTarget) -> Command {
    cmd(
        slot,
        Order::CastAbility {
            slot: AbilitySlot(ability),
            target,
        },
    )
}

fn step_n(w: &mut World, n: u32) -> Vec<Event> {
    let mut all = Vec::new();
    for _ in 0..n {
        all.extend(w.step(&[]));
    }
    all
}

#[test]
fn an_unlearned_ability_does_not_cast_and_a_passive_never_does() {
    let mut w = world();
    assert_eq!(
        w.validate(
            SlotId(0),
            &Order::CastAbility {
                slot: AbilitySlot(1),
                target: OrderTarget::None,
            }
        ),
        Err(RejectReason::EmptySlot),
        "level zero cannot be cast"
    );
    w.seats[0].abilities[0].level = 1;
    assert_eq!(
        w.validate(
            SlotId(0),
            &Order::CastAbility {
                slot: AbilitySlot(0),
                target: OrderTarget::None,
            }
        ),
        Err(RejectReason::WrongTargetKind),
        "the crit is a passive"
    );
}

#[test]
fn skill_points_and_caps_gate_levelling() {
    let mut w = world();
    // Level one: one point, spendable on a basic but not on the ultimate.
    assert_eq!(
        w.validate(
            SlotId(0),
            &Order::LevelUpAbility {
                slot: AbilitySlot(3)
            }
        ),
        Err(RejectReason::CannotLevelUp),
        "the ultimate waits for its level floor"
    );
    w.step(&[cmd(
        0,
        Order::LevelUpAbility {
            slot: AbilitySlot(1),
        },
    )]);
    let hero = hero_id(&w, 0);
    assert_eq!(w.seats[0].abilities[1].level, 1);
    // The point is spent; the next one needs another hero level.
    assert_eq!(
        w.validate(
            SlotId(0),
            &Order::LevelUpAbility {
                slot: AbilitySlot(0)
            }
        ),
        Err(RejectReason::CannotLevelUp)
    );
    // Ability level two needs hero level three, not two.
    w.seats[0].level = 2;
    w.units.get_mut(hero).unwrap().level = 2;
    assert_eq!(
        w.validate(
            SlotId(0),
            &Order::LevelUpAbility {
                slot: AbilitySlot(1)
            }
        ),
        Err(RejectReason::CannotLevelUp)
    );
    w.seats[0].level = 6;
    w.units.get_mut(hero).unwrap().level = 6;
    assert_eq!(
        w.validate(
            SlotId(0),
            &Order::LevelUpAbility {
                slot: AbilitySlot(3)
            }
        ),
        Ok(()),
        "level six opens the ultimate"
    );
}

#[test]
fn learned_abilities_and_their_cooldowns_survive_death() {
    let mut w = world();
    let hero = hero_id(&w, 0);
    w.seats[0].abilities[1].level = 1;
    w.step(&[cast(0, 1, OrderTarget::None)]);
    assert!(w.seats[0].abilities[1].cooldown > 0);
    // The hero dies; the seat keeps the kit and the cooldown keeps running.
    let cooldown_at_death = w.seats[0].abilities[1].cooldown;
    let mut events = Vec::new();
    let deaths = w.resolve_damage(
        vec![crate::sim::DamageInst {
            source: None,
            slot: None,
            team: Team::Dire,
            target: hero,
            amount: 1_000_000,
            kind: bota_proto::DamageKind::Pure,
            crit: false,
        }],
        &mut events,
    );
    w.process_deaths(deaths, &mut events);
    assert!(w.seats[0].unit.is_none(), "the hero is dead");
    step_n(&mut w, 30);
    assert_eq!(w.seats[0].abilities[1].level, 1, "the level survives");
    assert!(
        w.seats[0].abilities[1].cooldown < cooldown_at_death,
        "and the cooldown ticked while dead"
    );
    // Waiting out the respawn brings the same kit back.
    for _ in 0..3000 {
        w.step(&[]);
        if w.seats[0].unit.is_some() {
            break;
        }
    }
    assert!(w.seats[0].unit.is_some(), "respawned");
    assert_eq!(w.seats[0].abilities[1].level, 1);
}

#[test]
fn frenzy_hastens_attacks_for_its_duration_and_costs_its_price() {
    let mut w = world();
    let hero = hero_id(&w, 0);
    w.seats[0].abilities[1].level = 1;
    let mana_before = w.units.get(hero).unwrap().mana;
    let base = w.units.get(hero).unwrap().attack_interval;
    w.step(&[cast(0, 1, OrderTarget::None)]);
    let u = w.units.get(hero).unwrap();
    assert_eq!(u.mana, mana_before - rules::SYLLA_FRENZY_MANA[0]);
    // Casts run after the cooldown tick of the same step, so the fresh
    // cooldown still shows its full value.
    assert_eq!(
        w.seats[0].abilities[1].cooldown,
        rules::SYLLA_FRENZY_COOLDOWN[0]
    );
    assert!(
        u.current_attack_interval() < base,
        "the buffed interval is shorter: {} vs {base}",
        u.current_attack_interval()
    );
    assert_eq!(
        w.validate(
            SlotId(0),
            &Order::CastAbility {
                slot: AbilitySlot(1),
                target: OrderTarget::None,
            }
        ),
        Err(RejectReason::OnCooldown)
    );
    step_n(&mut w, rules::SYLLA_FRENZY_TICKS);
    assert_eq!(
        w.units.get(hero).unwrap().current_attack_interval(),
        base,
        "the buff wears off"
    );
}

#[test]
fn the_bounce_jumps_down_the_whole_chain() {
    let mut w = world();
    let hero = hero_id(&w, 0);
    w.units.get_mut(hero).unwrap().pos = Vec2::from_ints(8300, 8600);
    w.seats[0].abilities[2].level = 1;
    let mut dummies = Vec::new();
    for x in [8600, 8700, 8800] {
        let id = w
            .units
            .insert(Unit::melee_creep(Team::Dire, Vec2::from_ints(x, 8600)));
        w.units.get_mut(id).unwrap().move_speed = bota_proto::Fixed::ZERO;
        dummies.push(id);
    }
    w.step(&[cast(0, 2, OrderTarget::Unit { target: dummies[0] })]);
    step_n(&mut w, 60);
    for id in &dummies {
        let d = w.units.get(*id).unwrap();
        assert!(
            d.hp <= d.max_hp - rules::SYLLA_BOUNCE_DAMAGE[0],
            "every creep in the chain was hit, one is at {}/{}",
            d.hp,
            d.max_hp
        );
    }
}

#[test]
fn the_bounce_respects_its_cast_range() {
    let mut w = world();
    let hero = hero_id(&w, 0);
    w.units.get_mut(hero).unwrap().pos = Vec2::from_ints(8300, 8600);
    w.seats[0].abilities[2].level = 1;
    let far = w
        .units
        .insert(Unit::melee_creep(Team::Dire, Vec2::from_ints(9200, 8600)));
    assert_eq!(
        w.validate(
            SlotId(0),
            &Order::CastAbility {
                slot: AbilitySlot(2),
                target: OrderTarget::Unit { target: far },
            }
        ),
        Err(RejectReason::OutOfRange)
    );
}

#[test]
fn the_multishot_volleys_everything_around() {
    let mut w = world();
    let hero = hero_id(&w, 0);
    w.units.get_mut(hero).unwrap().pos = Vec2::from_ints(8300, 8600);
    w.seats[0].abilities[3].level = 1;
    let near = w
        .units
        .insert(Unit::melee_creep(Team::Dire, Vec2::from_ints(8700, 8600)));
    let far = w
        .units
        .insert(Unit::melee_creep(Team::Dire, Vec2::from_ints(10500, 8600)));
    let mana_before = w.units.get(hero).unwrap().mana;
    w.step(&[cast(0, 3, OrderTarget::None)]);
    assert_eq!(
        w.units.get(hero).unwrap().mana,
        mana_before - rules::SYLLA_MULTI_MANA[0]
    );
    step_n(&mut w, 60);
    let hit = w.units.get(near).unwrap();
    assert!(hit.hp < hit.max_hp, "the nearby creep was volleyed");
    let spared = w.units.get(far).unwrap();
    assert_eq!(spared.hp, spared.max_hp, "out of the radius means spared");
}

#[test]
fn the_crit_passive_lands_bigger_hits_now_and_then() {
    let mut w = world();
    let hero = hero_id(&w, 0);
    w.units.get_mut(hero).unwrap().pos = Vec2::from_ints(8300, 8600);
    w.seats[0].abilities[0].level = 4;
    w.units.get_mut(hero).unwrap().max_hp = 100_000;
    w.units.get_mut(hero).unwrap().hp = 100_000;
    let dummy = w
        .units
        .insert(Unit::melee_creep(Team::Dire, Vec2::from_ints(8600, 8600)));
    w.units.get_mut(dummy).unwrap().move_speed = bota_proto::Fixed::ZERO;
    w.units.get_mut(dummy).unwrap().max_hp = 100_000;
    w.units.get_mut(dummy).unwrap().hp = 100_000;
    w.step(&[cmd(0, Order::AttackUnit { target: dummy })]);
    let mut crits = 0;
    let mut plain = 0;
    let mut crit_amount = 0;
    let mut plain_amount = 0;
    for _ in 0..3000 {
        for e in w.step(&[]) {
            if let EventKind::Damaged {
                source: Some(s),
                target,
                amount,
                crit,
                ..
            } = e.kind
                && s == hero
                && target == dummy
            {
                if crit {
                    crits += 1;
                    crit_amount = amount;
                } else {
                    plain += 1;
                    plain_amount = amount;
                }
            }
        }
    }
    assert!(crits > 0, "the passive fires");
    assert!(plain > 0, "but not every time");
    assert!(
        crit_amount > plain_amount,
        "a crit hits harder: {crit_amount} vs {plain_amount}"
    );
}
