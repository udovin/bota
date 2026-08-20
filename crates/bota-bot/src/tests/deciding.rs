//! What the bot decides, measured against views built by hand.

use bota_proto::{
    AbilityView, Angle, EffectView, EntityId, Fixed, HeroId, ItemId, ItemSlot, ItemView, MapId,
    MatchInfo, Order, OrderTarget, Pick, PlayerView, SlotId, StatusFlags, Team, TickMode, UnitKind,
    UnitView, Vec2, WorldView,
};

use crate::{BURST, Brain, DELIVER, GO_HOME, Params, SALVE, TAKE_STASH, TANGO};

/// Where each fountain stands on the map both sides play.
const RADIANT_HOME: (i32, i32) = (1760, 2278);
/// The other one.
const DIRE_HOME: (i32, i32) = (16624, 16064);

/// A handle by its number alone.
fn id(idx: u32) -> EntityId {
    EntityId { idx, generation: 1 }
}

/// One unit standing somewhere, with everything named.
fn unit(idx: u32, kind: UnitKind, team: Team, at: (i32, i32), hp: i32) -> UnitView {
    UnitView {
        id: id(idx),
        kind,
        team,
        pos: Vec2::from_ints(at.0, at.1),
        facing: Angle::default(),
        hp,
        max_hp: 600,
        mana: 300,
        max_mana: 300,
        move_speed: Fixed::from_int(300),
        attack_damage: 50,
        attack_range: Fixed::from_int(150),
        attack_interval: 51,
        armor: Fixed::from_int(2),
        magic_resist: Fixed::from_ratio(25, 100),
        radius: Fixed::from_int(24),
        vision_radius: Fixed::from_int(1800),
        true_sight_radius: Fixed::ZERO,
        statuses: StatusFlags::default(),
        hero: (kind == UnitKind::Hero).then_some(HeroId(0)),
        owner: (kind == UnitKind::Hero).then_some(SlotId(0)),
        level: 1,
        abilities: Vec::new(),
        items: vec![None; 9],
        effects: Vec::new(),
    }
}

/// A building of a side, which both sides always see.
fn building(idx: u32, kind: UnitKind, team: Team, at: (i32, i32)) -> UnitView {
    let mut built = unit(idx, kind, team, at, 2000);
    built.max_hp = 2000;
    built.attack_damage = if kind == UnitKind::Tower { 110 } else { 0 };
    built.attack_range = Fixed::from_int(if kind == UnitKind::Tower { 700 } else { 0 });
    built.attack_interval = 29;
    built.radius = Fixed::from_int(144);
    built.move_speed = Fixed::ZERO;
    built
}

/// This seat's courier, standing somewhere with the errands it carries.
fn courier(idx: u32, at: (i32, i32)) -> UnitView {
    let mut bird = unit(idx, UnitKind::Courier, Team::Radiant, at, 250);
    bird.max_hp = 250;
    bird.attack_damage = 0;
    bird.attack_range = Fixed::ZERO;
    bird.owner = Some(SlotId(0));
    bird.hero = None;
    bird.items = vec![None; 6];
    bird.abilities = [BURST, TAKE_STASH, DELIVER, GO_HOME]
        .into_iter()
        .map(|id| AbilityView {
            id: bota_proto::AbilityId(id),
            level: 1,
            cooldown_left: 0,
            mana_cost: 0,
        })
        .collect();
    bird
}

/// A stash holding that many of one thing.
fn stash_of(count: usize) -> Vec<Option<ItemView>> {
    let mut slots = vec![None; 6];
    for slot in slots.iter_mut().take(count) {
        *slot = Some(ItemView {
            id: ItemId(TANGO),
            charges: 3,
            cooldown_left: 0,
        });
    }
    slots
}

/// A seat holding one body, with the gold it has.
fn player(unit: Option<EntityId>, gold: i32) -> PlayerView {
    PlayerView {
        slot: SlotId(0),
        team: Team::Radiant,
        hero: HeroId(0),
        unit,
        level: 1,
        xp: 0,
        gold: Some(gold),
        stash: Some(vec![None; 6]),
        kills: 0,
        deaths: 0,
        assists: 0,
        last_hits: 0,
        denies: 0,
        respawn_left: 0,
    }
}

/// Where the first tower of each side stands on the middle lane.
const RADIANT_TOWER: (i32, i32) = (7672, 7808);
/// The other one.
const DIRE_TOWER: (i32, i32) = (9740, 9868);

/// A view of one tick: the units given, both fountains, and one seat.
fn view(mut units: Vec<UnitView>, gold: i32) -> WorldView {
    let me = units
        .iter()
        .find(|unit| unit.kind == UnitKind::Hero && unit.team == Team::Radiant)
        .map(|unit| unit.id);
    units.push(building(
        90,
        UnitKind::Fountain,
        Team::Radiant,
        RADIANT_HOME,
    ));
    units.push(building(91, UnitKind::Fountain, Team::Dire, DIRE_HOME));
    units.push(building(92, UnitKind::Tower, Team::Radiant, RADIANT_TOWER));
    units.push(building(93, UnitKind::Tower, Team::Dire, DIRE_TOWER));
    WorldView {
        tick: 1,
        viewer: Some(Team::Radiant),
        units,
        projectiles: Vec::new(),
        players: vec![player(me, gold)],
        felled_trees: Vec::new(),
        planted_trees: Vec::new(),
    }
}

/// The same view a tick later, so that a want may be said again.
fn a_tick_later(world: &WorldView, by: u32) -> WorldView {
    let mut next = world.clone();
    next.tick += by;
    next
}

/// What the bot orders its own hero to do this tick.
///
/// Every test here is about the hero, so an order for anything else would be a
/// surprise worth failing on.
fn decide(brain: &mut Brain, world: &WorldView) -> Option<Order> {
    let ask = brain.decide(world)?;
    assert_eq!(ask.unit, None, "this order was not for the hero");
    Some(ask.order)
}

/// A mind that knows which seat it drives.
///
/// It plays by the plain numbers, not by the ones training left: what is being
/// measured here is the policy, and a trained set is data that moves under it.
fn seated() -> Brain {
    let mut brain = Brain::with(Params::default());
    brain.slot = Some(SlotId(0));
    brain
}

/// A hero standing in its own lane, well away from home and from any tower.
///
/// Clear of a tower on purpose: one covering the fixture would be shooting the
/// creeps in it, which is the very thing the swing is timed against.
fn me_in_the_lane(hp: i32) -> UnitView {
    unit(0, UnitKind::Hero, Team::Radiant, (6000, 6200), hp)
}

/// The terms of a match with the trees given.
fn match_info(trees: Vec<Vec2>) -> MatchInfo {
    MatchInfo {
        match_id: 1,
        map: MapId(0),
        tick_rate: 30,
        pregame_ticks: 900,
        trees,
        terrain_cells: 0,
        terrain_rle: Vec::new(),
        opaque_cells: Vec::new(),
        mode: TickMode::Lockstep,
        picks: vec![Pick {
            slot: SlotId(0),
            team: Team::Radiant,
            hero: HeroId(0),
        }],
    }
}

#[test]
fn a_bot_with_no_body_wants_nothing() {
    let mut brain = seated();
    let world = view(Vec::new(), 600);
    assert_eq!(
        decide(&mut brain, &world),
        None,
        "with nothing to drive it waits"
    );
}

#[test]
fn a_bot_buys_wherever_it_stands_and_lets_the_courier_carry_it() {
    let mut brain = seated();
    let home = unit(0, UnitKind::Hero, Team::Radiant, RADIANT_HOME, 600);
    assert_eq!(
        decide(&mut brain, &view(vec![home], 600)),
        Some(Order::BuyItem {
            item: ItemId(TANGO)
        }),
        "standing in the shop it buys the first thing on the list"
    );
    let mut brain = seated();
    let bird = courier(5, RADIANT_HOME);
    assert_eq!(
        decide(&mut brain, &view(vec![me_in_the_lane(600), bird], 600)),
        Some(Order::BuyItem {
            item: ItemId(TANGO)
        }),
        "and out in the lane it buys just the same, for the courier to fetch"
    );
    let mut brain = seated();
    assert!(
        !matches!(
            decide(&mut brain, &view(vec![me_in_the_lane(600)], 600)),
            Some(Order::BuyItem { .. })
        ),
        "but not with no courier standing: nothing would ever come for it"
    );
}

#[test]
fn a_bot_stops_buying_when_the_stash_has_no_room_for_it() {
    let mut brain = seated();
    let mut world = view(vec![me_in_the_lane(600)], 600);
    world.players[0].stash = Some(stash_of(6));
    assert!(
        !matches!(decide(&mut brain, &world), Some(Order::BuyItem { .. })),
        "nothing is bought that has nowhere to land"
    );
}

#[test]
fn a_bot_takes_what_the_stash_holds_before_it_buys_more() {
    let mut brain = seated();
    let home = unit(0, UnitKind::Hero, Team::Radiant, RADIANT_HOME, 600);
    let mut world = view(vec![home], 600);
    world.players[0].stash = Some(
        [
            Some(ItemView {
                id: ItemId(TANGO),
                charges: 3,
                cooldown_left: 0,
            }),
            None,
            None,
            None,
            None,
            None,
        ]
        .to_vec(),
    );
    assert_eq!(
        decide(&mut brain, &world),
        Some(Order::MoveItem {
            from: ItemSlot(9),
            to: ItemSlot(0),
        }),
        "what the stash holds is taken out first"
    );
}

#[test]
fn a_bot_takes_the_swing_that_brings_a_creep_down() {
    let mut brain = seated();
    let me = me_in_the_lane(600);
    // One enemy creep in reach and nearly down, another full and further off.
    let dying = unit(1, UnitKind::CreepMelee, Team::Dire, (6100, 6200), 20);
    let whole = unit(2, UnitKind::CreepMelee, Team::Dire, (6120, 6200), 500);
    assert_eq!(
        decide(&mut brain, &view(vec![me, dying, whole], 0)),
        Some(Order::AttackUnit { target: id(1) }),
        "it swings at what one hit would take"
    );
}

#[test]
fn a_bot_puts_out_its_own_creep_when_it_may() {
    let mut brain = seated();
    let me = me_in_the_lane(600);
    let mine = unit(1, UnitKind::CreepMelee, Team::Radiant, (6100, 6200), 20);
    let theirs = unit(2, UnitKind::CreepMelee, Team::Dire, (6160, 6200), 500);
    assert_eq!(
        decide(&mut brain, &view(vec![me, mine, theirs], 0)),
        Some(Order::AttackUnit { target: id(1) }),
        "one of its own worn far enough down is denied"
    );
}

#[test]
fn a_bot_leaves_a_creep_that_is_not_worth_the_swing() {
    let mut brain = seated();
    let me = me_in_the_lane(600);
    let whole = unit(1, UnitKind::CreepMelee, Team::Dire, (6100, 6200), 500);
    let mine = unit(2, UnitKind::CreepMelee, Team::Radiant, (6000, 6100), 500);
    assert!(
        !matches!(
            decide(&mut brain, &view(vec![me, whole, mine], 0)),
            Some(Order::AttackUnit { .. })
        ),
        "a creep a swing will not take is left alone"
    );
}

#[test]
fn a_bot_stands_behind_the_wave_rather_than_in_it() {
    let mut brain = seated();
    let me = unit(0, UnitKind::Hero, Team::Radiant, (4000, 4000), 600);
    let mine = unit(1, UnitKind::CreepMelee, Team::Radiant, (7000, 7200), 500);
    let theirs = unit(2, UnitKind::CreepMelee, Team::Dire, (7600, 7800), 500);
    let world = view(vec![me, mine.clone(), theirs], 0);
    let Some(Order::Move { pos }) = decide(&mut brain, &world) else {
        panic!("with the lane held it walks up to it");
    };
    let to_the_wave = crate::span(pos, mine.pos);
    assert!(
        to_the_wave > 0.0,
        "it does not walk into the creep it is following"
    );
    assert!(
        crate::span(pos, Vec2::from_ints(4000, 4000)) > 1000.0,
        "and it does not stay where it stands"
    );
}

#[test]
fn a_bot_walks_the_lane_forward_when_nothing_of_theirs_holds_it() {
    let mut brain = seated();
    let me = unit(0, UnitKind::Hero, Team::Radiant, (4000, 4000), 600);
    let mine = unit(1, UnitKind::CreepMelee, Team::Radiant, (7000, 7200), 500);
    assert!(
        matches!(
            decide(&mut brain, &view(vec![me, mine], 0)),
            Some(Order::AttackMove { .. })
        ),
        "with the lane theirs to lose it pushes instead of waiting"
    );
}

#[test]
fn a_bot_drinks_before_it_walks_home() {
    let mut brain = seated();
    brain.match_started(&match_info(vec![Vec2::from_ints(6050, 6200)]));
    let mut hurt = me_in_the_lane(200);
    hurt.items[0] = Some(ItemView {
        id: ItemId(TANGO),
        charges: 3,
        cooldown_left: 0,
    });
    assert_eq!(
        decide(&mut brain, &view(vec![hurt], 0)),
        Some(Order::UseItem {
            slot: ItemSlot(0),
            target: OrderTarget::Point {
                pos: Vec2::from_ints(6050, 6200)
            },
        }),
        "a tree in reach is worth more than the walk home"
    );
}

#[test]
fn a_bot_that_is_already_drinking_does_not_drink_again() {
    let mut brain = seated();
    let mut hurt = me_in_the_lane(200);
    hurt.items[0] = Some(ItemView {
        id: ItemId(SALVE),
        charges: 1,
        cooldown_left: 0,
    });
    hurt.effects.push(EffectView {
        id: bota_proto::EffectId(1),
        ticks_left: 200,
    });
    assert!(
        !matches!(
            decide(&mut brain, &view(vec![hurt], 0)),
            Some(Order::UseItem { .. })
        ),
        "one salve at a time"
    );
}

#[test]
fn a_bot_worn_down_leaves_and_comes_back_healed() {
    let mut brain = seated();
    let hurt = me_in_the_lane(100);
    let prey = unit(1, UnitKind::CreepMelee, Team::Dire, (6100, 6200), 10);
    let leaving = decide(&mut brain, &view(vec![hurt.clone(), prey.clone()], 0));
    let Some(Order::Move { pos }) = leaving else {
        panic!("worn down it leaves, whatever is standing in front of it");
    };
    assert!(
        crate::span(pos, hurt.pos) > 500.0,
        "it goes back down the lane, not to where it already stands"
    );
    // Half healed it is still on its way out.
    let mut half = hurt.clone();
    half.hp = 300;
    let world = a_tick_later(&view(vec![half, prey.clone()], 0), 30);
    assert!(
        matches!(decide(&mut brain, &world), Some(Order::Move { .. }) | None),
        "half healed it keeps going"
    );
    // Nearly whole, it turns round.
    let mut healed = hurt;
    healed.hp = 550;
    let world = a_tick_later(&view(vec![healed, prey], 0), 60);
    assert_eq!(
        decide(&mut brain, &world),
        Some(Order::AttackUnit { target: id(1) }),
        "healed it goes back to work"
    );
}

#[test]
fn a_bot_that_gets_nowhere_tries_another_way_round() {
    let mut brain = seated();
    brain.params = Params {
        wedge_ticks: 3.0,
        resend_ticks: 1.0,
        ..Params::default()
    };
    let me = unit(0, UnitKind::Hero, Team::Radiant, (4000, 4000), 600);
    let mine = unit(1, UnitKind::CreepMelee, Team::Radiant, (7000, 7200), 500);
    let theirs = unit(2, UnitKind::CreepMelee, Team::Dire, (7600, 7800), 500);
    let world = view(vec![me.clone(), mine, theirs], 0);
    let Some(Order::Move { pos: wanted }) = decide(&mut brain, &world) else {
        panic!("it means to walk up the lane");
    };
    // The same spot tick after tick: the body is not getting anywhere.
    let mut answer = None;
    for tick in 1..12 {
        answer = decide(&mut brain, &a_tick_later(&world, tick));
    }
    let Some(Order::Move { pos: instead }) = answer else {
        panic!("stuck long enough, it tries something else");
    };
    assert!(
        crate::span(wanted, instead) > 100.0,
        "and what it tries is not the way that was not working"
    );
}

#[test]
fn a_bot_does_not_say_the_same_thing_every_tick() {
    let mut brain = seated();
    let me = me_in_the_lane(600);
    let dying = unit(1, UnitKind::CreepMelee, Team::Dire, (6100, 6200), 20);
    let world = view(vec![me, dying], 0);
    assert_eq!(
        decide(&mut brain, &world),
        Some(Order::AttackUnit { target: id(1) }),
        "the swing is asked for once"
    );
    assert_eq!(
        decide(&mut brain, &a_tick_later(&world, 1)),
        None,
        "and not asked for again while it still stands"
    );
}

#[test]
fn a_bot_spends_its_skill_points() {
    let mut brain = seated();
    let mut me = me_in_the_lane(600);
    me.level = 2;
    me.abilities = vec![
        AbilityView {
            id: bota_proto::AbilityId(0),
            level: 1,
            cooldown_left: 0,
            mana_cost: 0,
        },
        AbilityView {
            id: bota_proto::AbilityId(1),
            level: 0,
            cooldown_left: 0,
            mana_cost: 100,
        },
        AbilityView {
            id: bota_proto::AbilityId(2),
            level: 0,
            cooldown_left: 0,
            mana_cost: 100,
        },
        AbilityView {
            id: bota_proto::AbilityId(3),
            level: 0,
            cooldown_left: 0,
            mana_cost: 100,
        },
    ];
    assert!(
        matches!(
            decide(&mut brain, &view(vec![me], 0)),
            Some(Order::LevelUpAbility { .. })
        ),
        "a point in hand is a point spent"
    );
}

/// How far up the middle lane a spot lies, as the bot measures it.
fn along_the_middle(at: Vec2) -> f32 {
    crate::onto(
        Vec2::from_ints(RADIANT_HOME.0, RADIANT_HOME.1),
        Vec2::from_ints(DIRE_HOME.0, DIRE_HOME.1),
        at,
    )
    .0
}

#[test]
fn a_bot_with_no_wave_in_sight_walks_to_where_the_lane_will_meet() {
    let mut brain = seated();
    let me = unit(0, UnitKind::Hero, Team::Radiant, (5000, 5000), 600);
    // Its own wave has just spawned and is behind it; the other side's is
    // still in the fog.
    let mine = unit(1, UnitKind::CreepMelee, Team::Radiant, (3000, 3200), 500);
    let order = decide(&mut brain, &view(vec![me.clone(), mine], 0));
    let pos = match order {
        Some(Order::AttackMove { pos }) | Some(Order::Move { pos }) => pos,
        other => panic!("it walks up the lane, not {other:?}"),
    };
    assert!(
        along_the_middle(pos) > along_the_middle(me.pos),
        "forward, towards where the towers say the waves will meet"
    );
    let between = along_the_middle(Vec2::from_ints(
        (RADIANT_TOWER.0 + DIRE_TOWER.0) / 2,
        (RADIANT_TOWER.1 + DIRE_TOWER.1) / 2,
    ));
    assert!(
        along_the_middle(pos) <= between + 0.02,
        "and no further up it than that"
    );
}

#[test]
fn a_bot_keeps_to_the_swing_it_has_begun() {
    let mut brain = seated();
    brain.params = Params {
        resend_ticks: 1.0,
        ..Params::default()
    };
    let me = me_in_the_lane(600);
    let dying = unit(1, UnitKind::CreepMelee, Team::Dire, (6100, 6200), 20);
    assert_eq!(
        decide(&mut brain, &view(vec![me.clone(), dying.clone()], 0)),
        Some(Order::AttackUnit { target: id(1) }),
        "the swing begins"
    );
    // The creep is mended back to full before the blow lands: the swing is
    // still in the air and giving it up would waste the whole cycle.
    let mut mended = dying;
    mended.hp = 600;
    let later = a_tick_later(&view(vec![me, mended], 0), 2);
    assert_eq!(
        decide(&mut brain, &later),
        Some(Order::AttackUnit { target: id(1) }),
        "and is carried through"
    );
}

#[test]
fn a_bot_keeps_the_lane_it_is_holding() {
    let mut brain = seated();
    let me = unit(0, UnitKind::Hero, Team::Radiant, (7000, 7200), 600);
    let mine = unit(1, UnitKind::CreepMelee, Team::Radiant, (7400, 7600), 500);
    let held = decide(&mut brain, &view(vec![me.clone(), mine.clone()], 0));
    let Some(Order::AttackMove { pos: first }) = held else {
        panic!("it holds the lane it is on");
    };
    // A wave appears on another lane. The one in hand still has a wave of its
    // own, so it is not worth half a minute of walking.
    let elsewhere: Vec<UnitView> = (0..3)
        .map(|at| {
            unit(
                10 + at as u32,
                UnitKind::CreepMelee,
                Team::Radiant,
                (2000, 8000 + at * 200),
                500,
            )
        })
        .collect();
    let mut units = vec![me, mine];
    units.extend(elsewhere);
    let world = a_tick_later(&view(units, 0), 30);
    let after = decide(&mut brain, &world);
    if let Some(Order::AttackMove { pos }) = after {
        assert!(
            crate::span(first, pos) < 500.0,
            "it keeps walking the lane it was on"
        );
    }
}

/// Which errand the bot sent the courier on, by the ability it names.
///
/// Answered as an ability rather than a slot: which slot an errand sits in is
/// the server's business, and a test that spelled out a number would be
/// testing the order the book happens to be filled in.
fn errand(brain: &mut Brain, world: &WorldView, courier: &UnitView) -> Option<u16> {
    let ask = brain.decide(world)?;
    assert_eq!(
        ask.unit,
        Some(courier.id),
        "the errand was not for the courier"
    );
    match ask.order {
        Order::CastAbility { slot, target } => {
            assert_eq!(target, OrderTarget::None, "an errand works on the courier");
            Some(
                courier
                    .abilities
                    .get(usize::from(slot.0))
                    .expect("the slot is one the courier has")
                    .id
                    .0,
            )
        }
        other => panic!("an errand is a cast, not {other:?}"),
    }
}

#[test]
fn a_bot_sends_the_courier_once_the_shopping_has_piled_up() {
    let mut brain = seated();
    let bird = courier(5, RADIANT_HOME);
    let mut world = view(vec![me_in_the_lane(600), bird.clone()], 0);
    world.players[0].stash = Some(stash_of(2));
    assert_eq!(
        errand(&mut brain, &world, &bird),
        Some(TAKE_STASH),
        "two things waiting are worth the trip"
    );
}

#[test]
fn a_bot_does_not_send_the_courier_for_one_thing() {
    let mut brain = seated();
    let bird = courier(5, RADIANT_HOME);
    let mut world = view(vec![me_in_the_lane(600), bird], 0);
    world.players[0].stash = Some(stash_of(1));
    assert_eq!(
        brain.decide(&world).map(|ask| ask.unit),
        Some(None),
        "one thing waits for company"
    );
}

#[test]
fn a_bot_sends_for_one_thing_that_has_waited_long_enough() {
    let mut brain = seated();
    brain.params = Params {
        courier_patience: 10.0,
        ..Params::default()
    };
    let bird = courier(5, RADIANT_HOME);
    let mut world = view(vec![me_in_the_lane(600), bird.clone()], 0);
    world.players[0].stash = Some(stash_of(1));
    decide(&mut brain, &world);
    let later = a_tick_later(&world, 30);
    assert_eq!(
        errand(&mut brain, &later, &bird),
        Some(TAKE_STASH),
        "waiting long enough is as good as company"
    );
}

#[test]
fn a_bot_has_the_courier_bring_what_it_carries() {
    let mut brain = seated();
    let mut bird = courier(5, (6200, 6400));
    bird.items[0] = Some(ItemView {
        id: ItemId(TANGO),
        charges: 3,
        cooldown_left: 0,
    });
    let world = view(vec![me_in_the_lane(600), bird.clone()], 0);
    assert_eq!(
        errand(&mut brain, &world, &bird),
        Some(DELIVER),
        "what it holds goes to its owner"
    );
}

#[test]
fn a_bot_hurries_a_courier_that_is_still_a_long_way_off() {
    let mut brain = seated();
    let mut bird = courier(5, RADIANT_HOME);
    bird.items[0] = Some(ItemView {
        id: ItemId(TANGO),
        charges: 3,
        cooldown_left: 0,
    });
    let world = view(vec![me_in_the_lane(600), bird.clone()], 0);
    assert_eq!(
        errand(&mut brain, &world, &bird),
        Some(BURST),
        "a long way off, the turn of speed comes first"
    );
}

#[test]
fn a_bot_keeps_the_courier_away_while_it_is_being_shot_at() {
    let mut brain = seated();
    let mut bird = courier(5, (6200, 6400));
    bird.items[0] = Some(ItemView {
        id: ItemId(TANGO),
        charges: 3,
        cooldown_left: 0,
    });
    // A creep of the other side standing in reach of the bot is a creep in
    // reach of whatever walks up to it.
    let shooting = unit(1, UnitKind::CreepMelee, Team::Dire, (6100, 6200), 500);
    let world = view(vec![me_in_the_lane(600), bird, shooting], 0);
    assert!(
        brain.decide(&world).is_none_or(|ask| ask.unit.is_none()),
        "nothing is sent into what is already shooting"
    );
}
