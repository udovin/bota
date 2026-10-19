//! Client logic that works without a window.

use bota_proto::{
    AbilityId, AbilitySlot, AbilityView, Aim, Angle, Attributes, EntityId, Fixed, HeroId, ItemId,
    ItemSlot, ItemView, Kit, Order, PlayerView, ShopEntry, SlotId, StatusFlags, Target, Team,
    UnitKind, UnitView, Vec2, WorldView,
};
use clap::Parser;

use crate::replay_play::ReplayPlayer;
use crate::slots::{Look, Press, Slot, ability_look, decide, item_look, press};
use crate::state::{
    App, Heard, Source, Tap, body_in, known_in, remember, tap_again, token_from_text, token_text,
};

use crate::Args;
use crate::camera::Camera;

#[test]
fn mechanics_catalog_mango_has_an_append_only_id_and_describes_consumption() {
    let mango = crate::catalog::item(42).expect("Mango catalog entry");
    assert_eq!(mango.id, 42);
    assert_eq!(mango.name(), "Mango");
    assert!(mango.blurb.contains("100 mana"));
    assert_eq!(crate::catalog::ITEMS.len(), 52);
}

#[test]
fn mechanics_catalog_mango_icon_rasterises_visible_fruit_inside_the_item_frame() {
    let mango = crate::catalog::item(42).expect("Mango catalog entry");
    let art = mango.icon.expect("Mango must have embedded item art");

    let (width, height, bytes) =
        crate::icons::pixels_at(art, crate::icons::ICON_SCALE).expect("Mango SVG must rasterise");

    assert_eq!((width, height), (192, 128));
    assert_eq!(bytes.len(), (width * height * 4) as usize);
    let pixels = bytes.as_chunks::<4>().0;
    let background = pixels[0];
    let fruit = pixels
        .iter()
        .filter(|pixel| pixel[3] > 0 && **pixel != background)
        .count();
    assert!(
        fruit > (width * height / 10) as usize,
        "Mango fruit must be visible against its frame: {fruit} pixels"
    );
}

#[test]
fn mechanics_catalog_shadowraze_effect_describes_the_counted_timer() {
    let effect = crate::catalog::effect(15).expect("Shadowraze debuff catalog entry");
    assert_eq!(effect.id, 15);
    assert_eq!(effect.name, "Razed");
    assert!(effect.blurb.contains("8 s"));
    assert!(effect.blurb.contains("same caster"));
    assert_eq!(crate::catalog::EFFECTS.len(), 18);
}

#[test]
fn mechanics_catalog_auras_and_shadowraze_keep_distinct_contiguous_effect_ids() {
    for (id, name) in [(13, "Guarded"), (14, "Inspired"), (15, "Razed")] {
        let effect = crate::catalog::effect(id).expect("merged effect catalog entry");
        assert_eq!(effect.id, id);
        assert_eq!(effect.name, name);
    }
    for (id, effect) in crate::catalog::EFFECTS.iter().enumerate() {
        assert_eq!(usize::from(effect.id), id);
    }
    assert!(crate::catalog::effect(18).is_none());
    assert!(crate::catalog::effect(u16::MAX).is_none());
}

#[test]
fn mechanics_catalog_all_razes_describe_exact_base_and_stacking_damage() {
    for id in 13..=15 {
        let raze = crate::catalog::ability(id).unwrap();
        assert!(raze.blurb.contains("90/160/230/300"));
        assert!(raze.blurb.contains("50/60/70/80"));
        assert!(raze.blurb.contains("8 s"));
    }
}

#[test]
fn camera_transforms_round_trip() {
    let cam = Camera {
        x: 4096.0,
        y: 4096.0,
        zoom: 0.45,
    };
    let (sw, sh) = (1280.0, 800.0);
    for (wx, wy) in [
        (0.0, 0.0),
        (4096.0, 4096.0),
        (8192.0, 1.0),
        (123.5, 7000.25),
    ] {
        let (sx, sy) = cam.world_to_screen(wx, wy, sw, sh);
        let (bx, by) = cam.screen_to_world(sx, sy, sw, sh);
        assert!((bx - wx).abs() < 0.01, "{bx} vs {wx}");
        assert!((by - wy).abs() < 0.01, "{by} vs {wy}");
    }
}

#[test]
fn the_world_y_axis_points_up_on_screen() {
    let cam = Camera {
        x: 4096.0,
        y: 4096.0,
        zoom: 0.45,
    };
    let (_, low) = cam.world_to_screen(4096.0, 0.0, 1280.0, 800.0);
    let (_, high) = cam.world_to_screen(4096.0, 8192.0, 1280.0, 800.0);
    assert!(high < low, "greater world Y must sit higher on the screen");
}

#[test]
fn zoom_stays_within_its_limits() {
    let mut cam = Camera::over(0.0, 0.0);
    for _ in 0..100 {
        cam.zoom_by(1.0);
    }
    assert!(cam.zoom <= crate::camera::ZOOM_MAX);
    for _ in 0..200 {
        cam.zoom_by(-1.0);
    }
    assert!(cam.zoom >= crate::camera::ZOOM_MIN);
}

#[test]
fn portraits_split_the_teams_around_the_clock() {
    use bota_proto::{HeroId, PlayerView, SlotId, Team};
    let player = |slot: u8, team: Team| PlayerView {
        slot: SlotId(slot),
        team,
        hero: HeroId(0),
        unit: None,
        level: 1,
        xp: 0,
        gold: None,
        stash: None,
        kit: None,
        kills: 0,
        deaths: 0,
        assists: 0,
        last_hits: 0,
        denies: 0,
        respawn_left: 0,
    };
    let players = [
        player(0, Team::Radiant),
        player(1, Team::Dire),
        player(2, Team::Radiant),
        player(3, Team::Dire),
    ];
    let sw = 1280.0;
    let rects = crate::hud::top_portraits(&players, sw);
    assert_eq!(rects.len(), 4);
    for (slot, rect) in &rects {
        let radiant = slot.0.is_multiple_of(2);
        if radiant {
            assert!(rect.x + rect.w < sw / 2.0, "radiant sits left of the clock");
        } else {
            assert!(rect.x > sw / 2.0, "dire sits right of the clock");
        }
        assert!(rect.contains(rect.x + 1.0, rect.y + 1.0));
        assert!(!rect.contains(rect.x - 1.0, rect.y));
    }
    // No two portraits overlap.
    for (i, (_, a)) in rects.iter().enumerate() {
        for (_, b) in &rects[i + 1..] {
            let apart = a.x + a.w <= b.x || b.x + b.w <= a.x;
            assert!(apart, "{a:?} vs {b:?}");
        }
    }
}

#[test]
fn the_bottom_panel_stays_on_screen() {
    let rect = crate::hud::bottom_panel(1280.0, 800.0);
    assert!(rect.x >= 0.0 && rect.y >= 0.0);
    assert!(rect.x + rect.w <= 1280.0);
    assert!(rect.y + rect.h <= 800.0);
    let narrow = crate::hud::bottom_panel(500.0, 400.0);
    assert!(narrow.w <= 500.0 - 40.0 + 0.5);
}

#[test]
fn a_session_token_is_kept_as_sixty_four_hex_digits() {
    let mut bytes = [0u8; 32];
    for (at, byte) in bytes.iter_mut().enumerate() {
        *byte = (at as u8).wrapping_mul(37) ^ 0xa5;
    }
    let token = bota_proto::Token(bytes);
    let text = token_text(&token);
    assert_eq!(text.len(), 64);
    assert!(text.chars().all(|c| c.is_ascii_hexdigit()));
    assert_eq!(token_from_text(&text), Some(token), "it reads back whole");
    assert_eq!(
        token_from_text(&format!("  {text}\n")),
        Some(token),
        "the line around it does not matter"
    );
    assert_eq!(token_from_text(&text[..62]), None, "too short");
    assert_eq!(token_from_text(&format!("{text}00")), None, "too long");
    assert_eq!(
        token_from_text(&format!("zz{}", &text[2..])),
        None,
        "not hexadecimal"
    );
    assert_eq!(
        token_from_text(&format!("\u{e9}{}", &text[2..])),
        None,
        "sixty-four bytes, but not ASCII"
    );
}

#[test]
fn arguments_parse_and_default() {
    let args = |list: &[&str]| {
        let mut whole = vec!["bota-client"];
        whole.extend_from_slice(list);
        Args::try_parse_from(whole)
    };
    let defaults = args(&[]).expect("no arguments is fine");
    assert_eq!(defaults.addr, "127.0.0.1:4455");
    assert_eq!(defaults.login, None);
    assert_eq!(defaults.guest, None);
    assert_eq!(defaults.replay, None);
    assert!(defaults.errand().is_none(), "the hall is where it starts");

    let full = args(&[
        "--addr",
        "10.0.0.2:5000",
        "--login",
        "alice",
        "--password",
        "pw",
        "--quick",
        "2",
    ])
    .expect("all flags are known");
    assert_eq!(full.addr, "10.0.0.2:5000");
    assert_eq!(full.login.as_deref(), Some("alice"));
    assert!(matches!(
        full.errand(),
        Some(bota_proto::ToHub::FindMatch {
            mode: bota_proto::ModeId(2),
            seats: 1
        })
    ));

    let watching = args(&["--replay", "m.brp"]).expect("a replay is a whole way to run");
    assert_eq!(watching.replay, Some(std::path::PathBuf::from("m.brp")));

    let seat = args(&["--match", "7"]).expect("one match by its number");
    assert!(matches!(
        seat.errand(),
        Some(bota_proto::ToHub::JoinMatch {
            id: bota_proto::MatchId(7),
            role: bota_proto::Role::Player { seats: 1 }
        })
    ));
    let eye = args(&["--watch", "7"]).expect("watching one match");
    assert!(matches!(
        eye.errand(),
        Some(bota_proto::ToHub::JoinMatch {
            role: bota_proto::Role::Spectator,
            ..
        })
    ));

    assert!(args(&["--what"]).is_err(), "an unknown flag is refused");
    assert!(args(&["--addr"]).is_err(), "a missing value is refused");
    assert!(
        args(&["--replay", "m.brp", "--login", "alice"]).is_err(),
        "a file to play and a server to sign in to are two different runs"
    );
    assert!(
        args(&["--register"]).is_err(),
        "registering needs the login to register"
    );
    assert!(
        args(&["--quick", "0", "--match", "7"]).is_err(),
        "one errand at a time"
    );
}

#[test]
fn every_item_drawing_rasterises_to_something_visible() {
    for face in crate::catalog::ITEMS {
        let name = face.name();
        let art = face.icon.expect("every item is drawn");
        let (w, h, bytes) = crate::icons::pixels_at(art, crate::icons::ICON_SCALE)
            .expect("the drawing is readable");
        assert_eq!(w, 192, "{name} is drawn to the frame");
        assert_eq!(h, 128, "{name} is drawn to the frame");
        assert_eq!(bytes.len(), (w * h * 4) as usize);
        let painted = bytes.chunks_exact(4).filter(|px| px[3] > 0).count();
        assert!(
            painted > (w * h / 2) as usize,
            "{name} covers its frame: {painted} of {}",
            w * h
        );
    }
}

#[test]
fn every_ability_box_has_a_key_to_press() {
    let panel = crate::hud::UiRect {
        x: 0.0,
        y: 0.0,
        w: 1600.0,
        h: 200.0,
    };
    let boxes = crate::hud::ability_boxes(&panel);
    assert_eq!(
        boxes.len(),
        usize::from(crate::hud::ABILITY_BOXES),
        "the panel draws as many boxes as it says it has"
    );
    for (slot, _) in boxes {
        assert!(
            crate::render::ability_key(usize::from(slot)).is_some(),
            "slot {slot} has a key"
        );
    }
}

#[test]
fn every_catalog_entry_answers_to_its_own_id() {
    for (index, face) in crate::catalog::ABILITIES.iter().enumerate() {
        assert_eq!(
            usize::from(face.id),
            index,
            "ability {} is in place",
            face.name
        );
        assert_eq!(
            crate::catalog::ability(face.id).map(|found| found.name),
            Some(face.name)
        );
    }
    for (index, face) in crate::catalog::ITEMS.iter().enumerate() {
        assert_eq!(
            usize::from(face.id),
            index,
            "item {} is in place",
            face.name()
        );
        assert_ne!(face.name(), "?", "item {index} has a name");
    }
    assert_eq!(crate::catalog::ITEMS.len(), bota_proto::ITEM_NAMES.len());
    for (index, face) in crate::catalog::EFFECTS.iter().enumerate() {
        assert_eq!(
            usize::from(face.id),
            index,
            "effect {} is in place",
            face.name
        );
        assert_eq!(
            crate::catalog::effect(face.id).map(|found| found.name),
            Some(face.name)
        );
    }
    for (index, face) in crate::catalog::HEROES.iter().enumerate() {
        assert_eq!(
            usize::from(face.id),
            index,
            "hero {} is in place",
            face.name
        );
    }
}

#[test]
fn every_catalog_entry_has_something_to_say() {
    for face in crate::catalog::ABILITIES {
        assert!(
            !face.name.is_empty() && !face.blurb.is_empty(),
            "{}",
            face.id
        );
    }
    for face in crate::catalog::ITEMS {
        assert!(
            !face.name().is_empty() && !face.blurb.is_empty() && !face.stats.is_empty(),
            "{}",
            face.id
        );
    }
    for face in crate::catalog::EFFECTS {
        assert!(
            !face.name.is_empty() && !face.blurb.is_empty(),
            "{}",
            face.id
        );
    }
    for face in crate::catalog::HEROES {
        assert!(!face.name.is_empty(), "{}", face.id);
    }
}

#[test]
fn a_part_in_hand_comes_off_what_the_shop_asks() {
    let shop = vec![
        ShopEntry {
            id: ItemId(0),
            cost: 500,
            components: Vec::new(),
        },
        ShopEntry {
            id: ItemId(19),
            cost: 450,
            components: Vec::new(),
        },
        ShopEntry {
            id: ItemId(13),
            cost: 450,
            components: Vec::new(),
        },
        ShopEntry {
            id: ItemId(29),
            cost: 1400,
            components: vec![ItemId(0), ItemId(19), ItemId(13)],
        },
    ];
    assert_eq!(
        crate::catalog::price_for(&shop, ItemId(29), &[]),
        1400,
        "with nothing in hand the whole is asked for"
    );
    assert_eq!(
        crate::catalog::price_for(&shop, ItemId(29), &[ItemId(0)]),
        900,
        "the boots already worn are not asked for twice"
    );
    assert_eq!(
        crate::catalog::price_for(&shop, ItemId(0), &[ItemId(0)]),
        500,
        "but a second of what is bought whole costs the whole of it"
    );
    assert_eq!(
        crate::catalog::price_for(&shop, ItemId(29), &[ItemId(29)]),
        1400,
        "and a second built one is built out of fresh parts"
    );
}

/// A hero body with every field named, so a new one in `UnitView` has to be
/// thought about here before these tests compile again.
pub fn a_unit() -> UnitView {
    UnitView {
        id: EntityId {
            idx: 3,
            generation: 1,
        },
        kind: UnitKind::Hero,
        team: Team::Radiant,
        pos: Vec2 {
            x: Fixed::from_int(5000),
            y: Fixed::from_int(5000),
        },
        facing: Angle::default(),
        hp: 600,
        max_hp: 620,
        mana: 300,
        max_mana: 300,
        move_speed: Fixed::from_int(300),
        attack_damage: 55,
        attack_range: Fixed::from_int(600),
        attack_time: 1700,
        attack_point: 500,
        attack_speed: 100,
        armor: Fixed::from_int(3),
        magic_resist: Fixed::from_ratio(25, 100),
        collision: Fixed::from_int(27),
        bound: Fixed::from_int(24),
        vision_radius: Fixed::from_int(1800),
        true_sight_radius: Fixed::ZERO,
        statuses: StatusFlags::default(),
        attributes: Attributes::ZERO,
        primary: None,
        hero: Some(HeroId(0)),
        owner: Some(SlotId(0)),
        level: 4,
        abilities: Vec::new(),
        items: vec![None; 9],
        effects: Vec::new(),
    }
}

/// A slot holding something aimed the given way.
fn holding(aim: Option<Aim>) -> Option<Option<Aim>> {
    Some(aim)
}

/// The one this seat commands, for the self-target case.
fn me() -> Option<EntityId> {
    Some(EntityId {
        idx: 3,
        generation: 1,
    })
}

#[test]
fn a_press_is_sent_whatever_state_the_slot_is_in() {
    // Nothing here looks at cooldown, mana or points: the server is what
    // turns a press down, and it has to be reached to do it.
    let slot = Slot::Ability(0);
    assert_eq!(
        decide(slot, false, holding(Some(Aim::Own)), None, me()),
        Press::Send(Order::Cast {
            slot: AbilitySlot(0),
            target: Target::None,
        }),
        "one that needs no aiming goes at once"
    );
    assert_eq!(
        decide(slot, false, holding(None), None, me()),
        Press::Send(Order::Cast {
            slot: AbilitySlot(0),
            target: Target::None,
        }),
        "and so does a passive, which the server answers for"
    );
    assert_eq!(
        decide(slot, false, holding(Some(Aim::Point)), None, me()),
        Press::Aim(slot),
        "one that is aimed waits for the click"
    );
}

#[test]
fn control_spends_a_point_instead_of_casting() {
    assert_eq!(
        decide(
            Slot::Ability(2),
            true,
            holding(Some(Aim::Point)),
            None,
            me()
        ),
        Press::Send(Order::Learn {
            slot: AbilitySlot(2),
        }),
        "held down, control levels whatever the slot is"
    );
    assert_eq!(
        decide(Slot::Item(1), true, holding(Some(Aim::Own)), None, me()),
        Press::Send(Order::Use {
            slot: ItemSlot(1),
            target: Target::None,
        }),
        "an item has no points to spend, so control is nothing to it"
    );
}

#[test]
fn an_empty_slot_and_a_pocket_answer_to_nothing() {
    assert_eq!(
        decide(Slot::Ability(0), false, None, None, me()),
        Press::Nothing,
        "nothing in the slot, nothing to press"
    );
    assert_eq!(
        decide(Slot::Item(7), false, holding(Some(Aim::Own)), None, me()),
        Press::Nothing,
        "the backpack is reached by dragging, not by pressing"
    );
    assert_eq!(
        decide(Slot::Item(11), false, holding(Some(Aim::Own)), None, me()),
        Press::Nothing,
        "and so is the stash"
    );
}

#[test]
fn reaching_twice_for_what_is_aimed_at_a_unit_aims_it_at_oneself() {
    let slot = Slot::Item(0);
    assert_eq!(
        decide(slot, false, holding(Some(Aim::Unit)), None, me()),
        Press::Aim(slot),
        "the first press takes it up"
    );
    assert_eq!(
        decide(slot, false, holding(Some(Aim::Unit)), Some(slot), me()),
        Press::Send(Order::Use {
            slot: ItemSlot(0),
            target: Target::Unit(me().expect("a commander")),
        }),
        "the second drinks it"
    );
    assert_eq!(
        decide(slot, false, holding(Some(Aim::Point)), Some(slot), me()),
        Press::Aim(slot),
        "one aimed at the ground has no such shortcut"
    );
}

/// An ability slot with the numbers given.
fn an_ability(level: u8, cooldown: u32, mana_cost: i32, passive: bool, on: bool) -> AbilityView {
    AbilityView {
        id: AbilityId(0),
        level,
        max_level: 4,
        cooldown_left: cooldown,
        mana_cost,
        range: 600,
        aim: Aim::Unit,
        passive,
        on,
        can_level: false,
    }
}

#[test]
fn what_keeps_an_ability_from_working_is_told_apart_from_what_it_is() {
    let ready = ability_look(Some(&an_ability(1, 0, 50, false, false)), 100);
    assert!(ready.filled && ready.learned && ready.usable, "ready");
    assert!(!ready.passive && !ready.toggled);

    let unlearned = ability_look(Some(&an_ability(0, 0, 50, false, false)), 100);
    assert!(unlearned.filled, "it is still in the slot");
    assert!(!unlearned.learned, "with no points in it");
    assert!(!unlearned.usable, "which is also why it cannot be used");

    let waiting = ability_look(Some(&an_ability(1, 45, 50, false, false)), 100);
    assert!(waiting.learned, "learned all the same");
    assert!(!waiting.usable);
    assert_eq!(waiting.cooldown_left, 45, "and the wait is worth showing");

    let poor = ability_look(Some(&an_ability(1, 0, 50, false, false)), 20);
    assert!(poor.learned && !poor.usable, "mana keeps it from working");
    assert_eq!(poor.cooldown_left, 0, "with nothing on the clock");

    let idle = ability_look(Some(&an_ability(1, 0, 0, true, false)), 100);
    assert!(idle.passive && !idle.usable, "a passive is never used");

    let running = ability_look(Some(&an_ability(1, 0, 0, false, true)), 100);
    assert!(running.toggled, "a toggle that is on says so");

    assert_eq!(
        ability_look(None, 100),
        Look::default(),
        "a slot this one does not carry is empty"
    );
}

/// An item view with the numbers given.
fn an_item(charges: Option<u8>, cooldown: u32, mana_cost: i32, aim: Option<Aim>) -> ItemView {
    ItemView {
        id: ItemId(0),
        charges,
        cooldown_left: cooldown,
        mute_left: 0,
        mode: None,
        mana_cost,
        range: 0,
        aim,
        for_sale: false,
    }
}

#[test]
fn an_item_with_no_charges_at_all_is_not_an_empty_one() {
    let boots = an_item(None, 0, 0, None);
    let worn = item_look(Some(&boots), 0, 0);
    assert!(worn.filled && worn.passive, "boots are worn, not used");
    assert!(!worn.usable, "and pressing them does nothing");

    let stick = an_item(Some(0), 0, 0, Some(Aim::Own));
    assert!(
        !item_look(Some(&stick), 0, 0).usable,
        "an empty stack cannot be spent"
    );
    let charged = an_item(Some(3), 0, 0, Some(Aim::Own));
    assert!(
        item_look(Some(&charged), 0, 0).usable,
        "one with charges can"
    );
    assert!(
        !item_look(Some(&charged), 7, 0).usable,
        "the same stack in the backpack cannot"
    );

    let dear = an_item(None, 0, 75, Some(Aim::Own));
    assert!(!item_look(Some(&dear), 0, 50).usable, "mana is short");
    assert!(item_look(Some(&dear), 0, 75).usable, "and now it is not");

    let waiting = an_item(None, 30, 0, Some(Aim::Own));
    let look = item_look(Some(&waiting), 0, 0);
    assert!(
        !look.usable && look.cooldown_left == 30,
        "still on the clock"
    );

    assert_eq!(item_look(None, 0, 0), Look::default(), "an empty slot");
}

#[test]
fn no_two_boxes_of_the_panel_sit_on_one_another() {
    let panel = crate::hud::bottom_panel(1280.0, 800.0);
    let mut boxes: Vec<(String, crate::hud::UiRect)> = Vec::new();
    for (slot, r) in crate::hud::ability_boxes(&panel) {
        boxes.push((format!("ability {slot}"), r));
    }
    for (slot, r) in crate::hud::item_boxes(&panel) {
        boxes.push((format!("item {slot}"), r));
    }
    for (slot, r) in crate::hud::stash_boxes(&panel) {
        boxes.push((format!("stash {slot}"), r));
    }
    for (one, a) in &boxes {
        for (other, b) in &boxes {
            if one == other {
                continue;
            }
            let apart =
                a.x + a.w <= b.x || b.x + b.w <= a.x || a.y + a.h <= b.y || b.y + b.h <= a.y;
            assert!(
                apart,
                "{one} and {other} overlap: a click would land on both"
            );
        }
    }
    // The panel's own boxes stay inside it. The stash strip is drawn above
    // the panel on purpose and is left out of this.
    for (which, r) in boxes.iter().filter(|(name, _)| !name.starts_with("stash")) {
        assert!(
            r.x >= panel.x
                && r.y >= panel.y
                && r.x + r.w <= panel.x + panel.w
                && r.y + r.h <= panel.y + panel.h,
            "{which} hangs off the panel"
        );
    }
}

#[test]
fn a_second_reach_for_the_same_thing_makes_a_pair() {
    let hero = Tap::Hero;
    let (again, kept) = tap_again(None, hero);
    assert!(!again, "the first press is only a press");
    assert!(kept.is_some(), "and it is remembered for a moment");

    let (again, kept) = tap_again(kept, hero);
    assert!(again, "the second makes the pair");
    assert!(
        kept.is_none(),
        "which is spent: a third press starts over rather than pairing again"
    );

    let (again, kept) = tap_again(kept, hero);
    assert!(!again, "so the third is a single press");
    let (again, _) = tap_again(kept, Tap::Courier);
    assert!(!again, "and reaching for something else never pairs");
}

#[test]
fn a_pair_of_reaches_for_two_different_things_is_no_pair() {
    let one = Tap::Unit(EntityId {
        idx: 4,
        generation: 1,
    });
    let other = Tap::Unit(EntityId {
        idx: 5,
        generation: 1,
    });
    let (_, kept) = tap_again(None, one);
    let (again, _) = tap_again(kept, other);
    assert!(!again, "two units clicked in turn are two picks");
}

#[test]
fn the_cursor_pushes_the_camera_only_at_the_very_edge() {
    let (sw, sh, pan) = (1280.0, 800.0, 9.0);
    assert_eq!(
        crate::input::edge_of(640.0, 400.0, sw, sh, pan),
        (0.0, 0.0),
        "the middle of the screen pushes nothing"
    );
    assert_eq!(
        crate::input::edge_of(1.0, 400.0, sw, sh, pan),
        (-pan, 0.0),
        "the left edge pushes left"
    );
    assert_eq!(
        crate::input::edge_of(sw - 1.0, 400.0, sw, sh, pan),
        (pan, 0.0),
        "and the right edge right"
    );
    assert_eq!(
        crate::input::edge_of(640.0, 1.0, sw, sh, pan),
        (0.0, -pan),
        "the top edge pushes up"
    );
    assert_eq!(
        crate::input::edge_of(1.0, sh - 1.0, sw, sh, pan),
        (-pan, pan),
        "a corner pushes both ways at once"
    );
    assert_eq!(
        crate::input::edge_of(-4.0, 400.0, sw, sh, pan),
        (0.0, 0.0),
        "a cursor gone out of the window pushes nothing"
    );
    assert_eq!(
        crate::input::edge_of(640.0, sh + 40.0, sw, sh, pan),
        (0.0, 0.0),
        "and neither does one below it"
    );
}

/// A seat with every field named, so a new one in `PlayerView` has to be
/// thought about here before these tests compile again.
pub fn a_player() -> PlayerView {
    PlayerView {
        slot: SlotId(0),
        team: Team::Radiant,
        hero: HeroId(0),
        unit: None,
        level: 1,
        xp: 0,
        gold: Some(600),
        stash: Some(vec![None; 6]),
        kit: None,
        kills: 0,
        deaths: 0,
        assists: 0,
        last_hits: 0,
        denies: 0,
        respawn_left: 0,
    }
}

#[test]
fn a_click_takes_the_ground_item_nearest_the_cursor_and_only_within_reach() {
    let handle = |idx: u32| EntityId { idx, generation: 1 };
    let lying_at = |idx: u32, x: i32, y: i32| bota_proto::LootView {
        id: handle(idx),
        pos: Vec2::from_ints(x, y),
        item: ItemId(0),
        charges: None,
    };
    let mut view = a_tick(0, Vec::new(), Vec::new());
    view.loot = vec![lying_at(21, 1000, 1000), lying_at(22, 1040, 1000)];
    assert_eq!(
        crate::input::loot_under_cursor(&view, 1010.0, 1000.0),
        Some(handle(21)),
        "the nearer of the two"
    );
    assert_eq!(
        crate::input::loot_under_cursor(&view, 1044.0, 1000.0),
        Some(handle(22)),
        "and the nearer from the other side"
    );
    assert_eq!(
        crate::input::loot_under_cursor(&view, 5000.0, 5000.0),
        None,
        "far from both, the click is for the ground"
    );
}

#[test]
fn an_attack_click_sticks_only_to_an_enemy_and_only_near() {
    let handle = |idx: u32| EntityId { idx, generation: 1 };
    let mut enemy = a_unit();
    enemy.id = handle(8);
    enemy.team = Team::Dire;
    enemy.pos = Vec2::from_ints(7000, 7000);
    let mut own = a_unit();
    own.id = handle(9);
    own.pos = Vec2::from_ints(7400, 7000);
    let view = a_tick(100, vec![enemy, own], Vec::new());
    let mine = Some(Team::Radiant);
    assert_eq!(
        crate::input::enemy_near_cursor(&view, 7040.0, 7000.0, mine),
        Some(handle(8)),
        "a click just off the enemy's edge sticks to it"
    );
    assert_eq!(
        crate::input::enemy_near_cursor(&view, 7060.0, 7000.0, mine),
        None,
        "one further off is for the ground"
    );
    assert_eq!(
        crate::input::enemy_near_cursor(&view, 7390.0, 7010.0, mine),
        None,
        "one of this side's own never sticks"
    );
    assert_eq!(
        crate::input::enemy_near_cursor(&view, 9000.0, 9000.0, mine),
        None,
        "far from everybody the click is for the ground"
    );
}

/// A tick holding these units and these seats.
fn a_tick(tick: u32, units: Vec<UnitView>, players: Vec<PlayerView>) -> WorldView {
    WorldView {
        tick,
        viewer: None,
        units,
        projectiles: Vec::new(),
        players,
        felled_trees: Vec::new(),
        planted_trees: Vec::new(),
        loot: Vec::new(),
    }
}

/// A seat with the unit the wire names for it, if any.
fn a_seat(slot: SlotId, unit: Option<EntityId>) -> PlayerView {
    let mut seat = a_player();
    seat.slot = slot;
    seat.unit = unit;
    seat
}

/// A unit of one kind belonging to one seat.
fn owned(idx: u32, kind: UnitKind, slot: Option<SlotId>) -> UnitView {
    let mut body = a_unit();
    body.id = EntityId { idx, generation: 1 };
    body.kind = kind;
    body.owner = slot;
    body
}

#[test]
fn only_what_a_seat_owns_is_worth_remembering() {
    let mut seen = Vec::new();
    let hero = owned(1, UnitKind::Hero, Some(SlotId(0)));
    let creep = owned(2, UnitKind::CreepMelee, None);
    remember(&mut seen, &a_tick(10, vec![hero, creep], Vec::new()));
    assert_eq!(seen.len(), 1, "the creep is not kept, the hero is");
    assert_eq!(seen[0].1.kind, UnitKind::Hero);
}

#[test]
fn a_unit_out_of_sight_is_known_as_it_was_and_how_long_ago() {
    let mut seen = Vec::new();
    let hero = owned(1, UnitKind::Hero, Some(SlotId(0)));
    let id = hero.id;
    let seat = a_seat(SlotId(0), Some(id));
    remember(&mut seen, &a_tick(10, vec![hero], vec![seat.clone()]));

    let live = a_tick(10, Vec::new(), vec![seat.clone()]);
    let (held, stale) = known_in(&seen, &live, id).expect("it is remembered");
    assert_eq!(held.id, id, "the body it was is still there to look at");
    assert_eq!(stale, 0, "and this tick it is as fresh as the memory");

    let later = a_tick(70, Vec::new(), vec![seat]);
    let (_, stale) = known_in(&seen, &later, id).expect("still remembered");
    assert_eq!(stale, 60, "sixty ticks later it is sixty ticks old");
}

#[test]
fn what_stands_now_is_known_before_what_was_remembered() {
    let mut seen = Vec::new();
    let was = owned(1, UnitKind::Hero, Some(SlotId(0)));
    let id = was.id;
    remember(&mut seen, &a_tick(10, vec![was], Vec::new()));
    let mut now = owned(1, UnitKind::Hero, Some(SlotId(0)));
    now.hp = 42;
    let view = a_tick(90, vec![now], Vec::new());
    let (held, stale) = known_in(&seen, &view, id).expect("it stands");
    assert_eq!(held.hp, 42, "the live body wins over the kept one");
    assert_eq!(stale, 0, "and nothing about it is old");
}

#[test]
fn a_seat_with_its_hero_down_is_still_picked_by_the_body_it_left() {
    let mut seen = Vec::new();
    let hero = owned(1, UnitKind::Hero, Some(SlotId(0)));
    let id = hero.id;
    remember(
        &mut seen,
        &a_tick(10, vec![hero], vec![a_seat(SlotId(0), Some(id))]),
    );
    let dead = a_tick(60, Vec::new(), vec![a_seat(SlotId(0), None)]);
    assert_eq!(
        body_in(&seen, &dead, SlotId(0)),
        Some(id),
        "the fallen body is the handle the seat is picked by"
    );
    assert_eq!(
        body_in(&seen, &dead, SlotId(1)),
        None,
        "a seat nothing is known of is picked by nothing"
    );
}

#[test]
fn a_hero_that_comes_back_leaves_only_one_body_behind() {
    let mut seen = Vec::new();
    let first = owned(1, UnitKind::Hero, Some(SlotId(0)));
    remember(&mut seen, &a_tick(10, vec![first], Vec::new()));
    let again = owned(2, UnitKind::Hero, Some(SlotId(0)));
    let fresh = again.id;
    remember(&mut seen, &a_tick(600, vec![again], Vec::new()));
    assert_eq!(seen.len(), 1, "the body it left is dropped for the new one");
    assert_eq!(seen[0].1.id, fresh);
    let view = a_tick(600, Vec::new(), vec![a_seat(SlotId(0), None)]);
    assert_eq!(
        body_in(&seen, &view, SlotId(0)),
        Some(fresh),
        "and the seat answers with the one it stood in last"
    );
}

/// A client with no window, playing one seat and looking at one tick.
fn an_app(mine: SlotId, view: WorldView) -> App {
    let player = ReplayPlayer::from_reader(std::io::Cursor::new(Vec::new()));
    let mut app = App::new(Source::Replay(Box::new(player)));
    app.my_slot = Some(mine);
    app.view = Some(view);
    app
}

#[test]
fn a_fallen_hero_picked_in_the_panel_still_shops_learns_and_drags() {
    let hero = EntityId {
        idx: 7,
        generation: 1,
    };
    let foe = EntityId {
        idx: 8,
        generation: 1,
    };
    let salve = an_item(Some(1), 0, 0, Some(Aim::Unit));
    let mut fallen = a_seat(SlotId(0), Some(hero));
    fallen.respawn_left = 300;
    fallen.kit = Some(Kit {
        abilities: vec![an_ability(1, 0, 0, false, false)],
        items: vec![Some(salve), None, None, None, None, None, None, None, None],
    });
    let mut other = a_seat(SlotId(1), Some(foe));
    other.team = Team::Dire;
    other.respawn_left = 300;
    let mut app = an_app(SlotId(0), a_tick(100, Vec::new(), vec![fallen, other]));
    app.selected = Some(hero);

    assert!(!app.hero_stands(), "nothing of it is on the field");
    assert!(app.controls_selection(), "it is still ours to command");
    assert_eq!(
        app.item_in(0),
        Some(salve),
        "its bag is the one it fell with"
    );
    assert_eq!(
        press(&app, Slot::Item(0), false),
        Press::Nothing,
        "nothing in that bag is pressed"
    );
    assert_eq!(
        press(&app, Slot::Ability(0), true),
        Press::Send(Order::Learn {
            slot: AbilitySlot(0)
        }),
        "a skill point is spent all the same"
    );
    let bought = app.send_order(Order::Buy { item: ItemId(1) });
    assert_eq!(
        bought.map(|given| given.unit),
        Some(hero),
        "a purchase names the fallen hero"
    );

    app.selected = Some(foe);
    assert!(
        !app.controls_selection(),
        "a fallen enemy is nobody's to command"
    );
    assert_eq!(app.item_in(0), None, "nor is its bag ours to reach into");
}

/// A match as the hall tells its members, with every field named.
fn a_room(id: u64) -> bota_proto::MatchState {
    bota_proto::MatchState {
        id: bota_proto::MatchId(id),
        name: "mid".to_string(),
        mode: bota_proto::ModeId(0),
        host: None,
        stage: bota_proto::Stage::Gathering,
        slots: Vec::new(),
        watchers: Vec::new(),
        yours: Vec::new(),
    }
}

/// A match as it begins, with every field named and nothing on its map.
fn a_match_info() -> bota_proto::MatchInfo {
    bota_proto::MatchInfo {
        match_id: bota_proto::MatchId(5),
        map: bota_proto::MapId(0),
        tick_rate: 30,
        pregame_ticks: 900,
        trees: Vec::new(),
        terrain_cells: 0,
        terrain_rle: Vec::new(),
        opaque_cells: Vec::new(),
        mode: bota_proto::TickMode::Realtime,
        picks: Vec::new(),
        shop: Vec::new(),
        rules: bota_proto::Rules {
            waves: bota_proto::Lanes::ALL,
            invulnerable_towers: bota_proto::Lanes::NONE,
            death_limit: 0,
            tower_ends_it: false,
            time_limit: 0,
            cheats: false,
        },
    }
}

#[test]
fn the_chat_keeps_its_newest_lines_and_leaves_the_hall_behind() {
    use crate::chat::shown;
    use bota_proto::{FromGame, FromHub};

    let mut app = an_app(SlotId(0), a_tick(0, Vec::new(), Vec::new()));
    app.handle(Heard::Hub(FromHub::Chat {
        from: "anna".to_string(),
        side: None,
        team: false,
        text: "hi".to_string(),
    }));
    app.handle(Heard::Hub(FromHub::Notice {
        text: "anna rolls 3 (1-6)".to_string(),
    }));
    assert_eq!(shown(&app.chat[0]), "anna: hi");
    assert_eq!(shown(&app.chat[1]), "anna rolls 3 (1-6)");
    for n in 0..crate::state::CHAT_KEPT + 5 {
        app.handle(Heard::Hub(FromHub::Notice {
            text: n.to_string(),
        }));
    }
    assert_eq!(app.chat.len(), crate::state::CHAT_KEPT, "the oldest go");
    assert_eq!(
        app.chat.last().map(|line| line.text.clone()),
        Some((crate::state::CHAT_KEPT + 4).to_string())
    );

    app.handle(Heard::Hub(FromHub::MatchState { state: a_room(5) }));
    assert!(app.chat.is_empty(), "the hall's lines stay in the hall");
    app.handle(Heard::Game(FromGame::Chat {
        from: "anna".to_string(),
        side: Some(Team::Dire),
        team: true,
        text: "push".to_string(),
    }));
    assert_eq!(
        shown(&app.chat[0]),
        "[team] anna: push",
        "the game server's chat lands in the same place"
    );
    app.handle(Heard::Hub(FromHub::MatchState { state: a_room(5) }));
    assert_eq!(app.chat.len(), 1, "news of the same match keeps its chat");
}

#[test]
fn a_refusal_from_the_game_server_is_said_and_shown() {
    let mut app = an_app(SlotId(0), a_tick(0, Vec::new(), Vec::new()));
    app.handle(Heard::Game(bota_proto::FromGame::Refused {
        reason: bota_proto::GameReason::UnknownTicket,
    }));
    assert!(app.game_lost, "the match is out of reach");
    assert_eq!(
        app.feed.last().map(|line| line.text.as_str()),
        Some("the game server does not know this ticket")
    );
    app.leave_room();
    assert!(!app.game_lost, "leaving the match forgets it");
}

#[test]
fn the_next_game_of_a_cup_starts_with_nothing_of_the_last() {
    use bota_proto::{FromGame, FromHub, MatchStats};

    let mut app = an_app(SlotId(0), a_tick(900, Vec::new(), Vec::new()));
    app.handle(Heard::Hub(FromHub::MatchState { state: a_room(5) }));
    app.handle(Heard::Game(FromGame::MatchOver {
        winner: Team::Radiant,
        stats: MatchStats {
            duration: 900,
            slots: Vec::new(),
        },
    }));
    assert!(app.over.is_some());

    // The hub brings the seat straight to the next game of the series.
    app.handle(Heard::Hub(FromHub::MatchState { state: a_room(6) }));
    assert!(app.over.is_none(), "the last game's end is not this one's");
    assert!(app.view.is_none(), "nor is its world");
    assert_eq!(app.room.as_ref().map(|room| room.id.0), Some(6));
    assert!(app.phase == crate::state::Phase::Lobby, "its lobby");
}

#[test]
fn every_hero_has_a_portrait_that_fills_its_frame_and_a_kit_the_catalog_names() {
    for face in crate::catalog::HEROES {
        let art = face.icon.expect("every hero is drawn");
        let (w, h, bytes) = crate::icons::pixels_at(art, crate::icons::PORTRAIT_SCALE)
            .expect("the drawing is readable");
        assert_eq!((w, h), (360, 240), "{} is drawn to the frame", face.name);
        let painted = bytes.chunks_exact(4).filter(|px| px[3] == 255).count();
        assert_eq!(painted, (w * h) as usize, "{} covers its frame", face.name);
        let corner = &bytes[..4];
        let figure = bytes.chunks_exact(4).filter(|px| *px != corner).count();
        assert!(
            figure > (w * h / 2) as usize,
            "{} stands out of its background: {figure} of {}",
            face.name,
            w * h
        );
        assert!(!face.abilities.is_empty(), "{} has a kit", face.name);
        for id in face.abilities {
            assert!(
                crate::catalog::ability(*id).is_some(),
                "{} carries ability {id}, which the catalog names",
                face.name
            );
        }
    }
}

#[test]
fn a_box_of_another_shape_shows_the_middle_of_a_portrait() {
    let narrow = crate::icons::middle_cut(360.0, 240.0, 1.0);
    assert_eq!(
        (narrow.x, narrow.y, narrow.w, narrow.h),
        (60.0, 0.0, 240.0, 240.0)
    );
    let wide = crate::icons::middle_cut(360.0, 240.0, 3.0);
    assert_eq!((wide.x, wide.y, wide.w, wide.h), (0.0, 60.0, 360.0, 120.0));
    let same = crate::icons::middle_cut(360.0, 240.0, 1.5);
    assert_eq!((same.x, same.y, same.w, same.h), (0.0, 0.0, 360.0, 240.0));
}

#[test]
fn hero_cards_sit_apart_between_the_seats_the_chat_and_the_buttons() {
    let (sw, sh) = (1280.0, 800.0);
    let chat = crate::hall::chat_panel(sw, sh);
    let ready = crate::lobby::action_button(sw, sh);
    let leave = crate::lobby::leave_button(sw, sh);
    assert!(leave.x + leave.w < ready.x, "the lobby's buttons sit apart");
    let random = crate::lobby::random_button(sw, sh);
    let walk_out = crate::lobby::picking_leave_button(sw, sh);
    assert!(
        walk_out.x + walk_out.w < random.x && random.x + random.w < ready.x,
        "the picking's buttons sit apart"
    );
    assert!(walk_out.x > crate::hall::LEFT + 260.0, "clear of the count");
    assert!(ready.x + ready.w <= chat.x, "and of the chat");
    for (heroes, per_side) in [(3, 1), (3, 5), (7, 1), (12, 2)] {
        let cards = crate::lobby::hero_cards(heroes, per_side, sw, sh);
        assert_eq!(cards.len(), heroes);
        let seats_end = crate::lobby::seat_row(0, per_side - 1, sw, sh);
        for (at, card) in cards.iter().enumerate() {
            assert!(
                card.x >= crate::hall::LEFT,
                "{heroes} heroes: card {at} starts on screen"
            );
            assert!(
                card.x + card.w <= chat.x,
                "{heroes} heroes: card {at} keeps off the chat"
            );
            assert!(
                card.y > seats_end.y + seats_end.h,
                "{heroes} heroes: card {at} is under the seats"
            );
            for other in &cards[at + 1..] {
                let apart = card.x + card.w <= other.x
                    || other.x + other.w <= card.x
                    || card.y + card.h <= other.y
                    || other.y + other.h <= card.y;
                assert!(apart, "{heroes} heroes: cards overlap");
            }
        }
        if heroes <= 3 {
            let last = cards.last().expect("cards");
            assert!(
                last.y + last.h <= ready.y,
                "{per_side} a side: a row of cards stays above the buttons"
            );
        }
    }
}

#[test]
fn heroes_are_picked_on_the_game_server_once_and_for_good() {
    use bota_proto::{FromGame, FromHub, PickReason, PickSeat, PickState};

    let picking = |mine: Option<HeroId>, set_on: Option<HeroId>| PickState {
        seats: vec![
            PickSeat {
                slot: SlotId(0),
                team: Team::Radiant,
                login: "anna".to_string(),
                hero: mine,
                selected: set_on,
            },
            PickSeat {
                slot: SlotId(1),
                team: Team::Dire,
                login: "boris".to_string(),
                hero: None,
                selected: None,
            },
        ],
        yours: vec![SlotId(0)],
        ms_left: 60_000,
    };
    let mut app = an_app(SlotId(0), a_tick(0, Vec::new(), Vec::new()));
    app.handle(Heard::Hub(FromHub::MatchState {
        state: bota_proto::MatchState {
            stage: bota_proto::Stage::Picking,
            yours: vec![SlotId(0)],
            ..a_room(5)
        },
    }));
    assert!(
        !app.may_pick(),
        "nothing is picked before the game server says so"
    );
    app.handle(Heard::Game(FromGame::Picking {
        state: picking(None, None),
    }));
    assert!(app.phase == crate::state::Phase::Picking);
    assert!(app.may_pick());
    assert_eq!(app.my_pick(), None);
    assert_eq!(app.my_selection(), None, "set on nothing yet");

    // A selection told back by the game server, as after coming back, and
    // one made here, which is the one that counts.
    app.handle(Heard::Game(FromGame::Picking {
        state: picking(None, Some(HeroId(1))),
    }));
    assert_eq!(app.my_selection(), Some(HeroId(1)));
    app.select_hero(HeroId(0));
    assert_eq!(app.my_selection(), Some(HeroId(0)));
    assert_eq!(app.my_pick(), None, "selecting picks nothing");

    app.handle(Heard::Game(FromGame::Picking {
        state: picking(Some(HeroId(2)), None),
    }));
    assert_eq!(app.my_pick(), Some(HeroId(2)));
    assert!(!app.may_pick(), "a pick is final");
    assert_eq!(app.my_selection(), None, "nothing is selected once picked");
    app.handle(Heard::Game(FromGame::PickRefused {
        seat: SlotId(0),
        hero: HeroId(1),
        reason: PickReason::AlreadyPicked,
    }));
    assert_eq!(
        app.reject.as_ref().map(|(text, _)| text.as_str()),
        Some("your hero is picked for good")
    );
    app.handle(Heard::Game(FromGame::MatchStart {
        info: a_match_info(),
    }));
    assert!(
        app.picking.is_none(),
        "the picking is over once the match begins"
    );
    assert!(app.phase == crate::state::Phase::Playing);
}

#[test]
fn a_lobby_is_ready_to_start_once_every_seat_is_held_by_somebody_here() {
    let seat = |login: &str, connected: bool| bota_proto::LobbySlot {
        slot: SlotId(0),
        team: Team::Radiant,
        login: login.to_string(),
        connected,
    };
    let room = |slots| bota_proto::MatchState { slots, ..a_room(5) };
    assert!(crate::lobby::all_here(&room(vec![
        seat("anna", true),
        seat("boris", true)
    ])));
    assert!(!crate::lobby::all_here(&room(vec![
        seat("anna", true),
        seat("", false)
    ])));
    assert!(!crate::lobby::all_here(&room(vec![
        seat("anna", true),
        seat("boris", false)
    ])));
}

#[test]
fn the_hall_offers_back_a_running_match_until_its_seat_is_given_up() {
    use bota_proto::{
        AccountId, AccountInfo, MapId, MatchId, MatchSummary, ModeId, SeatSummary, Stage,
    };

    let summary = |stage: Stage, abandoned: bool| MatchSummary {
        id: MatchId(9),
        name: "duel".to_string(),
        mode: ModeId(0),
        map: MapId(0),
        stage,
        host: None,
        seats: vec![
            SeatSummary {
                slot: SlotId(0),
                team: Team::Radiant,
                login: "anna".to_string(),
                hero: Some(HeroId(2)),
                abandoned,
            },
            SeatSummary {
                slot: SlotId(1),
                team: Team::Dire,
                login: "boris".to_string(),
                hero: Some(HeroId(1)),
                abandoned: false,
            },
        ],
        tournament: None,
        created_at: 0,
        duration: 0,
        winner: None,
        seed: None,
    };
    let player = ReplayPlayer::from_reader(std::io::Cursor::new(Vec::new()));
    let mut app = App::new(Source::Replay(Box::new(player)));
    app.account = Some(AccountInfo {
        id: AccountId(1),
        login: "anna".to_string(),
    });
    app.hall_matches = vec![summary(Stage::Gathering, false)];
    assert_eq!(
        app.match_to_rejoin().map(|m| m.id),
        Some(MatchId(9)),
        "a seat kept in a gathering match is the queue's or a cup's"
    );
    app.hall_matches = vec![summary(Stage::Playing, false)];
    assert_eq!(app.match_to_rejoin().map(|m| m.id), Some(MatchId(9)));
    app.hall_matches = vec![summary(Stage::Playing, true)];
    assert!(
        app.match_to_rejoin().is_none(),
        "a seat given up is not offered back"
    );
    app.hall_matches = vec![summary(Stage::Over, false)];
    assert!(app.match_to_rejoin().is_none(), "an ended match is not");
    app.account = Some(AccountInfo {
        id: AccountId(2),
        login: "carl".to_string(),
    });
    app.hall_matches = vec![summary(Stage::Playing, false)];
    assert!(
        app.match_to_rejoin().is_none(),
        "nor is one this account has no seat in"
    );
}

#[test]
fn the_panel_offering_a_match_back_sits_under_the_chat_with_its_buttons_apart() {
    for (sw, sh) in [(1280.0, 800.0), (1920.0, 1080.0), (1024.0, 640.0)] {
        let chat = crate::hall::chat_panel(sw, sh);
        let panel = crate::hall::rejoin_panel(sw, sh);
        let back = crate::hall::rejoin_button(sw, sh);
        let give_up = crate::hall::abandon_button(sw, sh);
        assert!(panel.y >= chat.y + chat.h, "under the chat at {sw}x{sh}");
        assert!(panel.y + panel.h <= sh, "on screen at {sw}x{sh}");
        for button in [&back, &give_up] {
            assert!(button.x >= panel.x && button.x + button.w <= panel.x + panel.w);
            assert!(button.y >= panel.y && button.y + button.h <= panel.y + panel.h);
        }
        assert!(back.x + back.w < give_up.x, "apart at {sw}x{sh}");
    }
}

#[test]
fn a_search_keeps_its_own_clock_and_a_found_match_the_time_it_first_gave() {
    use bota_proto::{FromHub, MatchId, ModeId, Search};

    let player = ReplayPlayer::from_reader(std::io::Cursor::new(Vec::new()));
    let mut app = App::new(Source::Replay(Box::new(player)));
    assert!(!app.searching());
    let queued = |waiting| {
        Heard::Hub(FromHub::Search {
            search: Search::Queued {
                mode: ModeId(1),
                waiting,
            },
        })
    };
    let found = |accepted, ms_left| {
        Heard::Hub(FromHub::Search {
            search: Search::Found {
                id: MatchId(7),
                mode: ModeId(1),
                accepted,
                players: 2,
                yours: accepted > 0,
                ms_left,
            },
        })
    };
    app.handle(queued(1));
    assert!(app.searching());
    app.tick_effects(5.0);
    app.handle(queued(2));
    assert_eq!(
        app.searched_for, 5.0,
        "news of who else waits does not restart the clock"
    );

    app.handle(found(0, 15_000));
    assert_eq!(app.found_span, 15.0);
    app.tick_effects(3.0);
    let Search::Found { ms_left, .. } = app.search else {
        panic!("a match is found");
    };
    assert_eq!(App::seconds_left(ms_left, app.search_age), 12.0);
    app.handle(found(1, 11_000));
    assert_eq!(app.found_span, 15.0, "the bar keeps its full length");
    assert_eq!(App::seconds_left(40, 2.0), 0.0, "never below zero");

    app.handle(Heard::Hub(FromHub::Search {
        search: Search::Idle,
    }));
    assert!(!app.searching());
    app.handle(queued(1));
    assert_eq!(app.searched_for, 0.0, "a new search starts its clock again");
}

#[test]
fn a_match_found_is_answered_from_buttons_inside_its_box() {
    for (sw, sh) in [(1280.0, 800.0), (1920.0, 1080.0), (1024.0, 640.0)] {
        let rect = crate::hall::found_box(sw, sh);
        let accept = crate::hall::found_accept(sw, sh);
        let decline = crate::hall::found_decline(sw, sh);
        for button in [&accept, &decline] {
            assert!(button.x >= rect.x && button.x + button.w <= rect.x + rect.w);
            assert!(button.y >= rect.y && button.y + button.h <= rect.y + rect.h);
        }
        assert!(decline.x + decline.w < accept.x, "apart at {sw}x{sh}");
    }
}
