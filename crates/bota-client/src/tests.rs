//! Client logic that works without a window.

use crate::Args;
use crate::camera::Camera;
use clap::Parser;

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
fn arguments_parse_and_default() {
    let args = |list: &[&str]| {
        let mut whole = vec!["bota-client"];
        whole.extend_from_slice(list);
        Args::try_parse_from(whole)
    };
    let defaults = args(&[]).expect("no arguments is fine");
    assert_eq!(defaults.addr, "127.0.0.1:4455");
    assert_eq!(defaults.name, "player");
    assert!(!defaults.spectate);
    assert_eq!(defaults.replay, None);

    let full = args(&["--addr", "10.0.0.2:5000", "--name", "alice", "--spectate"])
        .expect("all flags are known");
    assert_eq!(full.addr, "10.0.0.2:5000");
    assert_eq!(full.name, "alice");
    assert!(full.spectate);

    let watching = args(&["--replay", "m.brp"]).expect("a replay is a whole way to run");
    assert_eq!(watching.replay, Some(std::path::PathBuf::from("m.brp")));

    assert!(args(&["--what"]).is_err(), "an unknown flag is refused");
    assert!(args(&["--addr"]).is_err(), "a missing value is refused");
    assert!(
        args(&["--replay", "m.brp", "--spectate"]).is_err(),
        "a file to play and a match to watch are two different runs"
    );
}

#[test]
fn every_item_drawing_rasterises_to_something_visible() {
    for face in crate::catalog::ITEMS {
        let name = face.name;
        let art = face.icon.expect("every item is drawn");
        let (w, h, bytes) = crate::icons::pixels(art).expect("the drawing is readable");
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
        assert!(face.max_level > 0, "{} can be levelled", face.name);
    }
    for (index, face) in crate::catalog::ITEMS.iter().enumerate() {
        assert_eq!(
            usize::from(face.id),
            index,
            "item {} is in place",
            face.name
        );
        assert_eq!(
            crate::catalog::item(face.id).map(|found| found.name),
            Some(face.name)
        );
        assert!(face.cost > 0, "{} has a price", face.name);
        assert!(
            !face.at_a_tree || face.aim == crate::catalog::Aim::Point,
            "{} is aimed at the ground to reach a tree",
            face.name
        );
    }
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
    assert_eq!(
        crate::catalog::item(crate::catalog::TOWN_PORTAL_SCROLL).map(|face| face.name),
        Some("TP"),
        "the named scroll id is the scroll"
    );
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
            !face.name.is_empty() && !face.blurb.is_empty() && !face.stats.is_empty(),
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
