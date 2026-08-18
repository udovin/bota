//! Client logic that works without a window.

use crate::camera::Camera;
use crate::parse_args;

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
    let args = |list: &[&str]| parse_args(&list.iter().map(|s| s.to_string()).collect::<Vec<_>>());
    let defaults = args(&[]).expect("no arguments is fine");
    assert_eq!(defaults.addr, "127.0.0.1:4455");
    assert!(!defaults.spectate);
    assert_eq!(defaults.replay, None);

    let full = args(&[
        "--addr",
        "10.0.0.2:5000",
        "--name",
        "alice",
        "--spectate",
        "--replay",
        "m.brp",
    ])
    .expect("all flags are known");
    assert_eq!(full.addr, "10.0.0.2:5000");
    assert_eq!(full.name, "alice");
    assert!(full.spectate);
    assert_eq!(full.replay, Some(std::path::PathBuf::from("m.brp")));

    assert_eq!(args(&["--what"]), None, "an unknown flag asks for usage");
    assert_eq!(args(&["--addr"]), None, "a missing value asks for usage");
}
