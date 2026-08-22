//! Drawing the lobby, the world and the HUD.

use bota_proto::{Team, UnitKind, UnitView, WorldView};
use macroquad::prelude::*;

use crate::state::{App, Phase, Selection, Source};

const BACKGROUND: Color = Color::new(0.07, 0.08, 0.10, 1.0);
const GROUND: Color = Color::new(0.12, 0.14, 0.13, 1.0);
const LANE: Color = Color::new(0.20, 0.22, 0.18, 1.0);
const RADIANT: Color = Color::new(0.30, 0.80, 0.35, 1.0);
const DIRE: Color = Color::new(0.90, 0.30, 0.25, 1.0);
const HP_BACK: Color = Color::new(0.0, 0.0, 0.0, 0.7);
const MANA: Color = Color::new(0.25, 0.45, 0.95, 1.0);

fn team_color(team: Team) -> Color {
    match team {
        Team::Radiant => RADIANT,
        Team::Dire => DIRE,
        Team::Neutral => Color::new(0.65, 0.60, 0.45, 1.0),
    }
}

/// Draws one frame.
pub fn draw(app: &App) {
    clear_background(BACKGROUND);
    match app.phase {
        Phase::Lobby => draw_lobby(app),
        Phase::Playing => {
            if let Some(view) = &app.view {
                draw_world(app, view);
                draw_hud(app, view);
            } else {
                center_text("waiting for the first snapshot...", 24.0, WHITE);
            }
            if let Some((winner, _)) = &app.over {
                draw_over(app, *winner);
            }
        }
    }
}

fn draw_lobby(app: &App) {
    let x = 60.0;
    let mut y = 80.0;
    draw_text("bota", x, y, 48.0, WHITE);
    y += 50.0;
    let hint = if app.my_slot.is_some() {
        if app.ready {
            "ready - waiting for the others (R to cancel)"
        } else {
            "press R when ready"
        }
    } else if app.me.is_some() {
        "spectating - waiting for the match to start"
    } else {
        "connecting..."
    };
    draw_text(hint, x, y, 24.0, GRAY);
    y += 40.0;
    if app.my_slot.is_some() {
        let roster = crate::catalog::HEROES
            .iter()
            .enumerate()
            .map(|(id, hero)| format!("{}: {}", id + 1, hero.name))
            .collect::<Vec<_>>()
            .join("   ");
        draw_text(format!("pick a hero   {roster}"), x, y, 22.0, GOLD);
        y += 36.0;
    }
    for slot in &app.lobby {
        let holder = if slot.name.is_empty() {
            "open".to_string()
        } else {
            let ready = if slot.ready { " [ready]" } else { "" };
            format!("{}{ready}", slot.name)
        };
        let hero = slot
            .hero
            .and_then(|hero| crate::catalog::hero(hero.0))
            .map_or(String::new(), |hero| format!(" as {}", hero.name));
        let line = format!("{:?} seat {}: {holder}{hero}", slot.team, slot.slot.0);
        let mine = app.my_slot == Some(slot.slot);
        let color = if mine { WHITE } else { team_color(slot.team) };
        draw_text(&line, x, y, 26.0, color);
        y += 32.0;
    }
}

const TREE: Color = Color::new(0.16, 0.30, 0.16, 1.0);

/// The world spans this many units per axis; the server's `MAP_SIZE`.
const MAP_SIZE: f32 = 18432.0;

/// The lane polylines the waves march, through every tower of each lane.
const MAP_LANES: [&[(f32, f32)]; 3] = [
    &[
        (3296.0, 3864.0),
        (4576.0, 5072.0),
        (6026.0, 6290.0),
        (7672.0, 7808.0),
        (9740.0, 9868.0),
        (11712.0, 11328.0),
        (13488.0, 12975.0),
        (14744.0, 14216.0),
    ],
    &[
        (3296.0, 3864.0),
        (2624.0, 5808.0),
        (2715.0, 8344.0),
        (2880.0, 11072.0),
        (3050.0, 15150.0),
        (3941.0, 15252.0),
        (9088.0, 15232.0),
        (12768.0, 14992.0),
        (14744.0, 14216.0),
    ],
    &[
        (3296.0, 3864.0),
        (5264.0, 3104.0),
        (8856.0, 2960.0),
        (14076.0, 2837.0),
        (15400.0, 2900.0),
        (15485.0, 6976.0),
        (15616.0, 9600.0),
        (15552.0, 12248.0),
        (14744.0, 14216.0),
    ],
];

/// The terrain texture, baked from the MatchStart cells on first use.
fn terrain_texture(app: &App) -> Option<Texture2D> {
    thread_local! {
        static BAKED: std::cell::RefCell<Option<Texture2D>> = const { std::cell::RefCell::new(None) };
    }
    BAKED.with(|slot| {
        let mut slot = slot.borrow_mut();
        if slot.is_none() && app.terrain_cells > 0 {
            let n = app.terrain_cells;
            let mut img = Image::gen_image_color(n as u16, n as u16, BLACK);
            for cy in 0..n {
                for cx in 0..n {
                    let b = app.terrain[cy * n + cx];
                    let tier = b & 0x1f;
                    // Cliffs win over water, or the pit walls standing in
                    // the river would drown in blue.
                    let color = if b & 0x80 == 0 {
                        Color::new(0.07, 0.09, 0.08, 1.0) // cliffs and holes
                    } else if b & 0x40 != 0 {
                        Color::new(0.13, 0.22, 0.29, 1.0) // river water
                    } else {
                        let l = 0.10 + 0.035 * f32::from(tier);
                        Color::new(l, l + 0.025, l - 0.01, 1.0)
                    };
                    img.set_pixel(cx as u32, (n - 1 - cy) as u32, color);
                }
            }
            let tex = Texture2D::from_image(&img);
            tex.set_filter(FilterMode::Nearest);
            *slot = Some(tex);
        }
        slot.clone()
    })
}

/// One viewer of the client's own fog: position, vision radius, ground tier.
struct FogViewer {
    x: f32,
    y: f32,
    radius: f32,
    tier: u8,
}

/// Everything the client-side fog needs for one frame.
struct Fog<'a> {
    n: usize,
    terrain: &'a [u8],
    opaque: &'a [u64],
    viewers: Vec<FogViewer>,
}

impl Fog<'_> {
    fn gather<'a>(app: &'a App, view: &WorldView) -> Option<Fog<'a>> {
        let team = app.fog_team()?;
        if app.terrain_cells == 0 {
            return None;
        }
        let viewers = view
            .units
            .iter()
            .filter(|u| u.team == team && u.vision_radius.to_f32() > 0.0)
            .map(|u| {
                let (x, y) = (u.pos.x.to_f32(), u.pos.y.to_f32());
                FogViewer {
                    x,
                    y,
                    radius: u.vision_radius.to_f32(),
                    tier: tier_at(app, x, y),
                }
            })
            .collect();
        Some(Fog {
            n: app.terrain_cells,
            terrain: &app.terrain,
            opaque: &app.opaque,
            viewers,
        })
    }

    fn opaque_at(&self, cx: usize, cy: usize) -> bool {
        let idx = cy * self.n + cx;
        self.opaque[idx / 64] & (1 << (idx % 64)) != 0
    }

    /// Whether any viewer lights a terrain cell: within radius, on ground
    /// at least as high, with no opaque cell across the sight line.
    fn lit(&self, cx: usize, cy: usize) -> bool {
        let px = cx as f32 * 64.0 + 32.0;
        let py = cy as f32 * 64.0 + 32.0;
        let cell_tier = self.terrain[cy * self.n + cx] & 0x1f;
        'viewers: for v in &self.viewers {
            let (dx, dy) = (px - v.x, py - v.y);
            if dx * dx + dy * dy > v.radius * v.radius || cell_tier > v.tier {
                continue;
            }
            let len = dx.abs().max(dy.abs());
            let steps = ((len / 32.0) as i32 + 1).max(1);
            let from_cell = ((v.x / 64.0) as usize, (v.y / 64.0) as usize);
            for i in 1..steps {
                let sx = v.x + dx * i as f32 / steps as f32;
                let sy = v.y + dy * i as f32 / steps as f32;
                let sc = ((sx / 64.0) as usize, (sy / 64.0) as usize);
                if sc == from_cell || sc == (cx, cy) {
                    continue;
                }
                if sc.0 >= self.n
                    || sc.1 >= self.n
                    || self.opaque_at(sc.0, sc.1)
                    || self.terrain[sc.1 * self.n + sc.0] & 0x1f > v.tier
                {
                    continue 'viewers;
                }
            }
            return true;
        }
        false
    }
}

/// The ground tier under a world position.
fn tier_at(app: &App, x: f32, y: f32) -> u8 {
    let n = app.terrain_cells;
    let cx = (x / 64.0) as usize;
    let cy = (y / 64.0) as usize;
    if n == 0 || cx >= n || cy >= n {
        return 0;
    }
    app.terrain[cy * n + cx] & 0x1f
}

const FOG_SHADE: Color = Color::new(0.0, 0.0, 0.02, 0.52);

/// Shades the unseen ground of the minimap. The mask is rebuilt every few
/// frames; between rebuilds the last texture is reused.
fn draw_minimap_fog(app: &App, view: &WorldView, r: &crate::hud::UiRect) {
    thread_local! {
        static CACHE: std::cell::RefCell<Option<(u32, Texture2D)>> =
            const { std::cell::RefCell::new(None) };
    }
    let Some(fog) = Fog::gather(app, view) else {
        return;
    };
    CACHE.with(|slot| {
        let mut slot = slot.borrow_mut();
        let stamp = view.tick / 15;
        let stale = match &*slot {
            Some((at, _)) => *at != stamp,
            None => true,
        };
        if stale {
            let n = fog.n;
            let mut img =
                Image::gen_image_color(n as u16, n as u16, Color::new(0.0, 0.0, 0.0, 0.0));
            for cy in 0..n {
                for cx in 0..n {
                    if !fog.lit(cx, cy) {
                        img.set_pixel(cx as u32, (n - 1 - cy) as u32, FOG_SHADE);
                    }
                }
            }
            let tex = Texture2D::from_image(&img);
            tex.set_filter(FilterMode::Nearest);
            *slot = Some((stamp, tex));
        }
        if let Some((_, tex)) = &*slot {
            draw_texture_ex(
                tex,
                r.x,
                r.y,
                WHITE,
                DrawTextureParams {
                    dest_size: Some(vec2(r.w, r.h)),
                    ..Default::default()
                },
            );
        }
    });
}

/// Shades the unseen ground of the visible part of the world.
fn draw_world_fog(app: &App, view: &WorldView, sw: f32, sh: f32) {
    let Some(fog) = Fog::gather(app, view) else {
        return;
    };
    let (wx0, wy1) = app.camera.screen_to_world(0.0, 0.0, sw, sh);
    let (wx1, wy0) = app.camera.screen_to_world(sw, sh, sw, sh);
    let c0 = ((wx0 / 64.0).floor().max(0.0)) as usize;
    let c1 = (((wx1 / 64.0).ceil()) as usize).min(fog.n - 1);
    let r0 = ((wy0 / 64.0).floor().max(0.0)) as usize;
    let r1 = (((wy1 / 64.0).ceil()) as usize).min(fog.n - 1);
    let cell_px = 64.0 * app.camera.zoom;
    for cy in r0..=r1 {
        for cx in c0..=c1 {
            if !fog.lit(cx, cy) {
                let (x, y) =
                    app.camera
                        .world_to_screen(cx as f32 * 64.0, cy as f32 * 64.0 + 64.0, sw, sh);
                draw_rectangle(x, y, cell_px + 1.0, cell_px + 1.0, FOG_SHADE);
            }
        }
    }
}

fn draw_world(app: &App, view: &WorldView) {
    let (sw, sh) = (screen_width(), screen_height());
    let to_screen = |wx: f32, wy: f32| app.camera.world_to_screen(wx, wy, sw, sh);

    // The ground and the lanes.
    let (x0, y0) = to_screen(0.0, MAP_SIZE);
    let (x1, y1) = to_screen(MAP_SIZE, 0.0);
    match terrain_texture(app) {
        Some(tex) => draw_texture_ex(
            &tex,
            x0,
            y0,
            WHITE,
            DrawTextureParams {
                dest_size: Some(vec2(x1 - x0, y1 - y0)),
                ..Default::default()
            },
        ),
        None => draw_rectangle(x0, y0, x1 - x0, y1 - y0, GROUND),
    }
    for lane in MAP_LANES {
        for seg in lane.windows(2) {
            let (ax, ay) = to_screen(seg[0].0, seg[0].1);
            let (bx, by) = to_screen(seg[1].0, seg[1].1);
            draw_line(ax, ay, bx, by, 24.0 * app.camera.zoom.max(0.2), LANE);
        }
    }
    draw_rectangle_lines(x0, y0, x1 - x0, y1 - y0, 2.0, GRAY);
    for (index, &(wx, wy)) in app.trees.iter().enumerate() {
        if view.felled_trees.contains(&(index as u32)) {
            continue;
        }
        let (x, y) = to_screen(wx, wy);
        draw_circle(x, y, (48.0 * app.camera.zoom).max(2.0), TREE);
    }
    for at in &view.planted_trees {
        let (x, y) = to_screen(at.x.to_f32(), at.y.to_f32());
        let r = (48.0 * app.camera.zoom).max(2.0);
        draw_circle(x, y, r, TREE);
        draw_circle_lines(x, y, r, 1.0, Color::new(0.6, 0.9, 0.5, 0.7));
    }

    draw_world_fog(app, view, sw, sh);

    draw_teleport_spots(app, view, to_screen);
    draw_tree_pick(app, view, to_screen);

    let me = app.my_hero();
    for u in &view.units {
        draw_unit(app, u, me == Some(u.id), to_screen);
    }
    for p in &view.projectiles {
        let (x, y) = to_screen(p.pos.x.to_f32(), p.pos.y.to_f32());
        draw_circle(x, y, 3.0, GOLD);
    }
    for f in &app.floaters {
        let (x, y) = to_screen(f.world.0, f.world.1);
        let rise = f.age * 40.0;
        let alpha = (1.2 - f.age).clamp(0.0, 1.0);
        draw_text(
            &f.text,
            x + 10.0,
            y - 14.0 - rise,
            18.0,
            Color::new(1.0, 0.9, 0.6, alpha),
        );
    }
}

/// The ground a held scroll may be aimed at: a ring round every building of
/// one's own side that still stands.
fn draw_teleport_spots(app: &App, view: &WorldView, to_screen: impl Fn(f32, f32) -> (f32, f32)) {
    let Some(slot) = app.pending_item else {
        return;
    };
    if app.item_id_at(slot).map(|id| id.0) != Some(crate::catalog::TOWN_PORTAL_SCROLL) {
        return;
    }
    let Some(side) = app.my_team() else {
        return;
    };
    let radius = TELEPORT_RANGE * app.camera.zoom;
    for u in &view.units {
        if u.team != side
            || !matches!(
                u.kind,
                UnitKind::Tower | UnitKind::Ancient | UnitKind::Fountain
            )
        {
            continue;
        }
        let (x, y) = to_screen(u.pos.x.to_f32(), u.pos.y.to_f32());
        draw_circle(x, y, radius, Color::new(0.35, 0.75, 1.0, 0.12));
        draw_circle_lines(x, y, radius, 2.0, Color::new(0.45, 0.85, 1.0, 0.8));
    }
}

/// The tree a held item is pointed at right now, ringed so it is plain which
/// one would go.
fn draw_tree_pick(app: &App, view: &WorldView, to_screen: impl Fn(f32, f32) -> (f32, f32)) {
    let Some(slot) = app.pending_item else {
        return;
    };
    let Some(id) = app.item_id_at(slot) else {
        return;
    };
    if !crate::catalog::item(id.0).is_some_and(|face| face.at_a_tree) {
        return;
    }
    let (mx, my) = mouse_position();
    let (wx, wy) = app
        .camera
        .screen_to_world(mx, my, screen_width(), screen_height());
    let standing = app
        .trees
        .iter()
        .enumerate()
        .filter(|(index, _)| !view.felled_trees.contains(&(*index as u32)))
        .map(|(_, at)| *at)
        .chain(
            view.planted_trees
                .iter()
                .map(|at| (at.x.to_f32(), at.y.to_f32())),
        );
    let mut best: Option<(f32, (f32, f32))> = None;
    for (tx, ty) in standing {
        let far = (tx - wx) * (tx - wx) + (ty - wy) * (ty - wy);
        if far <= TREE_RADIUS * TREE_RADIUS && best.is_none_or(|(b, _)| far < b) {
            best = Some((far, (tx, ty)));
        }
    }
    if let Some((_, (tx, ty))) = best {
        let (x, y) = to_screen(tx, ty);
        let r = (TREE_RADIUS * app.camera.zoom).max(4.0);
        draw_circle_lines(x, y, r, 2.0, Color::new(1.0, 0.85, 0.35, 0.9));
    }
}

fn draw_unit(app: &App, u: &UnitView, mine: bool, to_screen: impl Fn(f32, f32) -> (f32, f32)) {
    let (x, y) = to_screen(u.pos.x.to_f32(), u.pos.y.to_f32());
    let r = (u.radius.to_f32() * app.camera.zoom).max(4.0);
    let color = team_color(u.team);
    match u.kind {
        UnitKind::Tower => draw_rectangle(x - r, y - r, r * 2.0, r * 2.0, color),
        UnitKind::Ancient => {
            draw_poly(x, y, 4, r * 1.3, 45.0, color);
        }
        UnitKind::Fountain => {
            draw_circle_lines(x, y, r, 3.0, color);
        }
        UnitKind::Courier => {
            let wing = (14.0 * app.camera.zoom).max(4.0);
            draw_poly(x, y, 3, wing, 30.0, color);
            draw_circle_lines(x, y, wing + 2.0, 1.0, Color::new(1.0, 1.0, 1.0, 0.35));
        }
        // A ward takes no room on the ground, so it is drawn to a size of its
        // own rather than to its hull. The one that reveals is drawn hollow
        // and square, the one that watches solid and pointed.
        UnitKind::Ward => {
            let eye = (10.0 * app.camera.zoom).max(4.0);
            if u.true_sight_radius > bota_proto::Fixed::ZERO {
                draw_poly_lines(x, y, 4, eye, 45.0, 2.0, color);
                if mine {
                    let reach = u.true_sight_radius.to_f32() * app.camera.zoom;
                    draw_circle_lines(x, y, reach, 1.0, Color::new(0.9, 0.8, 1.0, 0.35));
                }
            } else {
                draw_poly(x, y, 3, eye, 90.0, color);
            }
            if u.statuses.bits & bota_proto::StatusFlags::INVISIBLE != 0 {
                draw_circle_lines(x, y, eye + 3.0, 1.0, Color::new(0.8, 0.8, 1.0, 0.5));
            }
        }
        _ => draw_circle(x, y, r, color),
    }
    if !matches!(
        u.kind,
        UnitKind::Tower | UnitKind::Ancient | UnitKind::Fountain
    ) {
        let theta = f32::from(u.facing.brads) / 65536.0 * std::f32::consts::TAU;
        let (fx, fy) = (theta.cos(), -theta.sin());
        draw_line(
            x,
            y,
            x + fx * (r + 6.0),
            y + fy * (r + 6.0),
            2.0,
            Color::new(1.0, 1.0, 1.0, 0.65),
        );
    }
    if mine {
        draw_circle_lines(x, y, r + 4.0, 2.0, WHITE);
        if app.attack_move_armed {
            let range = u.attack_range.to_f32() * app.camera.zoom;
            draw_circle_lines(x, y, range, 1.0, Color::new(1.0, 0.6, 0.2, 0.5));
        }
    }
    let picked = match app.selection {
        Selection::Unit(id) => id == u.id,
        Selection::Seat(slot) => u.owner == Some(slot),
        Selection::Own => false,
    };
    if picked && !mine {
        draw_circle_lines(x, y, r + 4.0, 2.0, GOLD);
    }
    if u.kind == UnitKind::Hero {
        draw_text(format!("{}", u.level), x - 4.0, y + 5.0, 16.0, BLACK);
        if let Some(owner) = u.owner {
            let name = app.seat_name(owner);
            let width = measure_text(&name, None, 16, 1.0).width;
            draw_text(&name, x - width / 2.0, y + r + 18.0, 16.0, WHITE);
        }
    }
    // Health, and mana for heroes.
    let bar_w = (r * 2.2).max(26.0);
    let bar_x = x - bar_w / 2.0;
    let bar_y = y - r - 10.0;
    let frac = (u.hp.max(0) as f32 / u.max_hp.max(1) as f32).clamp(0.0, 1.0);
    draw_rectangle(bar_x, bar_y, bar_w, 5.0, HP_BACK);
    draw_rectangle(bar_x, bar_y, bar_w * frac, 5.0, color);
    if u.max_mana > 0 {
        let frac = (u.mana.max(0) as f32 / u.max_mana as f32).clamp(0.0, 1.0);
        draw_rectangle(bar_x, bar_y + 6.0, bar_w, 3.0, HP_BACK);
        draw_rectangle(bar_x, bar_y + 6.0, bar_w * frac, 3.0, MANA);
    }
}

fn draw_hud(app: &App, view: &WorldView) {
    let ticks = i64::from(view.tick) - i64::from(app.pregame_ticks);
    let seconds = ticks.div_euclid(i64::from(app.tick_rate.max(1)));
    let clock = format!(
        "{}{}:{:02}",
        if seconds < 0 { "-" } else { "" },
        seconds.abs() / 60,
        seconds.abs() % 60
    );
    let clock_w = measure_text(&clock, None, 28, 1.0).width;
    draw_text(&clock, (screen_width() - clock_w) / 2.0, 40.0, 28.0, WHITE);

    draw_top_panel(app, view);
    draw_bottom_panel(app, view);
    draw_minimap(app, view);
    if app.controls_selection() && app.my_slot.is_some() {
        draw_stash(app, view);
        draw_shop_button(app);
        if app.shop_open {
            draw_shop(app, view);
        }
    }
    draw_tooltips(app, view);

    let mut fy = 30.0;
    for line in &app.feed {
        let alpha = ((8.0 - line.age) / 2.0).clamp(0.0, 1.0);
        let width = measure_text(&line.text, None, 20, 1.0).width;
        draw_text(
            &line.text,
            screen_width() - width - 16.0,
            fy,
            20.0,
            Color::new(0.9, 0.9, 0.9, alpha),
        );
        fy += 24.0;
    }

    if let Some((reason, _)) = &app.reject {
        center_text_at(
            &format!("order rejected: {reason}"),
            screen_height() - 90.0,
            22.0,
            ORANGE,
        );
    }
    if app.attack_move_armed {
        center_text_at(
            "attack-move: click a target point",
            screen_height() - 60.0,
            22.0,
            ORANGE,
        );
    }
    if let Some(slot) = app.my_slot
        && let Some(p) = view.players.iter().find(|p| p.slot == slot)
        && p.unit.is_none()
        && app.over.is_none()
    {
        let secs = p.respawn_left / u32::from(app.tick_rate.max(1)) + 1;
        center_text(&format!("respawn in {secs}"), 36.0, WHITE);
    }
    if app.lost {
        center_text_at(
            "connection lost",
            screen_height() / 2.0 - 80.0,
            32.0,
            ORANGE,
        );
    }
    let controls = match &app.source {
        Source::Replay(player) => {
            let state = if player.finished() {
                "finished"
            } else if player.paused {
                "paused"
            } else {
                "playing"
            };
            format!(
                "replay {state}  x{}  [space] pause  [.] step  [+/-] speed  [wheel] zoom",
                player.speed
            )
        }
        Source::Live(_) => {
            "[RMB] move/attack  [A] attack-move  [S] stop  [H] hold  [wheel] zoom".to_string()
        }
    };
    draw_text(&controls, 16.0, screen_height() - 14.0, 18.0, GRAY);
}

fn draw_top_panel(app: &App, view: &WorldView) {
    let rate = u32::from(app.tick_rate.max(1));
    for (slot, rect) in crate::hud::top_portraits(&view.players, screen_width()) {
        let Some(p) = view.players.iter().find(|pl| pl.slot == slot) else {
            continue;
        };
        let unit = p.unit.and_then(|id| view.units.iter().find(|u| u.id == id));
        let base = team_color(p.team);
        let bg = Color::new(base.r * 0.45, base.g * 0.45, base.b * 0.45, 0.95);
        draw_rectangle(rect.x, rect.y, rect.w, rect.h, bg);
        let border = if app.selection == Selection::Seat(slot) {
            WHITE
        } else {
            BLACK
        };
        draw_rectangle_lines(rect.x, rect.y, rect.w, rect.h, 2.0, border);
        draw_text(
            format!("{}", p.level),
            rect.x + 4.0,
            rect.y + 16.0,
            16.0,
            WHITE,
        );
        if let Some(u) = unit {
            let frac = (u.hp.max(0) as f32 / u.max_hp.max(1) as f32).clamp(0.0, 1.0);
            draw_rectangle(
                rect.x + 2.0,
                rect.y + rect.h - 8.0,
                rect.w - 4.0,
                6.0,
                HP_BACK,
            );
            draw_rectangle(
                rect.x + 2.0,
                rect.y + rect.h - 8.0,
                (rect.w - 4.0) * frac,
                6.0,
                base,
            );
        } else if p.unit.is_some() {
            draw_text(
                "?",
                rect.x + rect.w / 2.0 - 4.0,
                rect.y + rect.h - 6.0,
                16.0,
                GRAY,
            );
        } else {
            let secs = p.respawn_left / rate + 1;
            draw_rectangle(
                rect.x,
                rect.y,
                rect.w,
                rect.h,
                Color::new(0.0, 0.0, 0.0, 0.55),
            );
            draw_text(
                format!("{secs}"),
                rect.x + rect.w / 2.0 - 8.0,
                rect.y + rect.h / 2.0 + 7.0,
                22.0,
                WHITE,
            );
        }
        let name = app.seat_name(slot);
        let width = measure_text(&name, None, 14, 1.0).width;
        draw_text(
            &name,
            rect.x + (rect.w - width) / 2.0,
            rect.y + rect.h + 14.0,
            14.0,
            GRAY,
        );
    }
}

fn draw_bottom_panel(app: &App, view: &WorldView) {
    let rect = crate::hud::bottom_panel(screen_width(), screen_height());
    draw_rectangle(
        rect.x,
        rect.y,
        rect.w,
        rect.h,
        Color::new(0.05, 0.06, 0.08, 0.92),
    );
    draw_rectangle_lines(
        rect.x,
        rect.y,
        rect.w,
        rect.h,
        2.0,
        Color::new(0.4, 0.4, 0.4, 1.0),
    );
    let rate = u32::from(app.tick_rate.max(1));
    // A picked creep or building shows its own status.
    if let Selection::Unit(id) = app.selection
        && let Some(u) = view.units.iter().find(|u| u.id == id)
    {
        let title = format!("{:?} {}", u.team, kind_name(u.kind));
        draw_text(
            &title,
            rect.x + 14.0,
            rect.y + 26.0,
            22.0,
            team_color(u.team),
        );
        if u.kind == UnitKind::Fountain {
            draw_text("invulnerable", rect.x + 14.0, rect.y + 48.0, 14.0, GRAY);
        }
        draw_vitals(u, &rect, rate);
        // Anything may be looked at; only what this seat drives answers to
        // the keys.
        let own = app.drives(u.id);
        let level = if own { u.level } else { 0 };
        let pending = if own { app.pending_ability } else { None };
        draw_slot_boxes(Some(u), &rect, rate, level, own, pending, None);
        draw_effects(u, &rect, rate);
        return;
    }
    let slot = match app.selection {
        Selection::Seat(slot) => Some(slot),
        _ => app.default_slot(),
    };
    let Some(slot) = slot else {
        return;
    };
    let Some(p) = view.players.iter().find(|pl| pl.slot == slot) else {
        return;
    };
    let unit = p.unit.and_then(|id| view.units.iter().find(|u| u.id == id));

    // The seat itself.
    let x0 = rect.x + 14.0;
    let y0 = rect.y + 24.0;
    draw_text(app.seat_name(slot), x0, y0, 22.0, team_color(p.team));
    draw_text(format!("Level {}", p.level), x0, y0 + 22.0, 16.0, WHITE);
    draw_text(
        format!("K/D/A  {}/{}/{}", p.kills, p.deaths, p.assists),
        x0,
        y0 + 42.0,
        16.0,
        GRAY,
    );
    draw_text(
        format!("LH/DN  {}/{}", p.last_hits, p.denies),
        x0,
        y0 + 60.0,
        16.0,
        GRAY,
    );
    if let Some(gold) = p.gold {
        draw_text(format!("{gold} g"), x0, y0 + 80.0, 18.0, GOLD);
    }

    // Health, mana and stats of the living hero; its fate otherwise.
    if let Some(u) = unit {
        draw_vitals(u, &rect, rate);
    } else if p.unit.is_some() {
        draw_text(
            "somewhere in the fog",
            rect.x + 175.0,
            rect.y + 44.0,
            18.0,
            GRAY,
        );
    } else {
        let secs = p.respawn_left / rate + 1;
        draw_text(
            format!("dead - respawns in {secs}s"),
            rect.x + 175.0,
            rect.y + 44.0,
            18.0,
            GRAY,
        );
    }
    let own = app.my_slot == Some(slot);
    let pending = if own { app.pending_ability } else { None };
    let held = if own { app.held_item } else { None };
    draw_slot_boxes(unit, &rect, rate, p.level, own, pending, held);
    if let Some(u) = unit {
        draw_effects(u, &rect, rate);
    }
}

/// The minimap: terrain, lanes, every visible unit, the camera frame.
fn draw_minimap(app: &App, view: &WorldView) {
    let r = crate::hud::minimap(screen_height());
    let at = |wx: f32, wy: f32| (r.x + wx / MAP_SIZE * r.w, r.y + (1.0 - wy / MAP_SIZE) * r.h);
    match terrain_texture(app) {
        Some(tex) => draw_texture_ex(
            &tex,
            r.x,
            r.y,
            WHITE,
            DrawTextureParams {
                dest_size: Some(vec2(r.w, r.h)),
                ..Default::default()
            },
        ),
        None => draw_rectangle(r.x, r.y, r.w, r.h, Color::new(0.09, 0.11, 0.10, 0.95)),
    }
    for lane in MAP_LANES {
        for seg in lane.windows(2) {
            let (ax, ay) = at(seg[0].0, seg[0].1);
            let (bx, by) = at(seg[1].0, seg[1].1);
            draw_line(ax, ay, bx, by, 2.0, LANE);
        }
    }
    for (index, &(wx, wy)) in app.trees.iter().enumerate() {
        if view.felled_trees.contains(&(index as u32)) {
            continue;
        }
        let (x, y) = at(wx, wy);
        draw_circle(x, y, 1.0, TREE);
    }
    for spot in &view.planted_trees {
        let (x, y) = at(spot.x.to_f32(), spot.y.to_f32());
        draw_circle(x, y, 1.0, TREE);
    }
    draw_minimap_fog(app, view, &r);
    let me = app.my_hero();
    for u in &view.units {
        let (x, y) = at(u.pos.x.to_f32(), u.pos.y.to_f32());
        let color = team_color(u.team);
        match u.kind {
            UnitKind::Tower => draw_rectangle(x - 2.0, y - 2.0, 4.0, 4.0, color),
            UnitKind::Ancient => draw_rectangle(x - 3.0, y - 3.0, 6.0, 6.0, color),
            UnitKind::Fountain => draw_circle_lines(x, y, 3.0, 1.0, color),
            UnitKind::Courier => draw_poly(x, y, 3, 2.0, 30.0, color),
            UnitKind::Ward => {
                if u.true_sight_radius > bota_proto::Fixed::ZERO {
                    draw_poly_lines(x, y, 4, 2.5, 45.0, 1.0, color);
                } else {
                    draw_poly(x, y, 3, 2.5, 90.0, color);
                }
            }
            UnitKind::Hero => {
                draw_circle(x, y, 3.0, color);
                if me == Some(u.id) {
                    draw_circle_lines(x, y, 4.5, 1.0, WHITE);
                }
            }
            _ => draw_circle(x, y, 1.5, color),
        }
    }
    // The camera frame.
    let (sw, sh) = (screen_width(), screen_height());
    let (wx0, wy0) = app.camera.screen_to_world(0.0, 0.0, sw, sh);
    let (wx1, wy1) = app.camera.screen_to_world(sw, sh, sw, sh);
    let (fx0, fy0) = at(wx0.clamp(0.0, MAP_SIZE), wy0.clamp(0.0, MAP_SIZE));
    let (fx1, fy1) = at(wx1.clamp(0.0, MAP_SIZE), wy1.clamp(0.0, MAP_SIZE));
    draw_rectangle_lines(
        fx0,
        fy0,
        fx1 - fx0,
        fy1 - fy0,
        1.0,
        Color::new(1.0, 1.0, 1.0, 0.6),
    );
    draw_rectangle_lines(r.x, r.y, r.w, r.h, 2.0, Color::new(0.4, 0.4, 0.4, 1.0));
}

/// The chips of the timed effects on the panel's hero.
fn draw_effects(unit: &UnitView, rect: &crate::hud::UiRect, rate: u32) {
    let boxes = crate::hud::effect_boxes(rect, unit.effects.len());
    for (e, r) in unit.effects.iter().zip(boxes) {
        draw_rectangle(r.x, r.y, r.w, r.h, Color::new(0.08, 0.10, 0.14, 0.95));
        draw_rectangle_lines(r.x, r.y, r.w, r.h, 1.0, Color::new(0.7, 0.6, 0.2, 1.0));
        let name = crate::catalog::effect(e.id.0).map_or("?", |face| face.name);
        let secs = e.ticks_left / rate + 1;
        draw_text(name, r.x + 4.0, r.y + 15.0, 13.0, WHITE);
        let label = format!("{secs}s");
        let w = measure_text(&label, None, 12, 1.0).width;
        draw_text(&label, r.x + r.w - w - 4.0, r.y + 15.0, 12.0, GOLD);
    }
}

fn kind_name(kind: UnitKind) -> &'static str {
    match kind {
        UnitKind::Hero => "Hero",
        UnitKind::CreepMelee => "Melee Creep",
        UnitKind::CreepFlagbearer => "Flagbearer Creep",
        UnitKind::CreepRanged => "Ranged Creep",
        UnitKind::CreepSiege => "Siege Creep",
        UnitKind::CreepNeutral => "Neutral Creep",
        UnitKind::Roshan => "Roshan",
        UnitKind::Courier => "Courier",
        UnitKind::Tower => "Tower",
        UnitKind::Ancient => "Ancient",
        UnitKind::Fountain => "Fountain",
        UnitKind::Ward => "Ward",
    }
}

/// The middle of the bottom panel: bars with numbers and the stat lines.
fn draw_vitals(u: &UnitView, rect: &crate::hud::UiRect, rate: u32) {
    let bx = rect.x + 175.0;
    let bw = 225.0;
    let frac = (u.hp.max(0) as f32 / u.max_hp.max(1) as f32).clamp(0.0, 1.0);
    draw_rectangle(bx, rect.y + 14.0, bw, 20.0, HP_BACK);
    draw_rectangle(bx, rect.y + 14.0, bw * frac, 20.0, team_color(u.team));
    let hp = format!("{} / {}", u.hp.max(0), u.max_hp);
    let w = measure_text(&hp, None, 15, 1.0).width;
    draw_text(&hp, bx + (bw - w) / 2.0, rect.y + 29.0, 15.0, WHITE);
    draw_rectangle(bx, rect.y + 40.0, bw, 14.0, HP_BACK);
    if u.max_mana > 0 {
        let frac = (u.mana.max(0) as f32 / u.max_mana as f32).clamp(0.0, 1.0);
        draw_rectangle(bx, rect.y + 40.0, bw * frac, 14.0, MANA);
        let mana = format!("{} / {}", u.mana.max(0), u.max_mana);
        let w = measure_text(&mana, None, 13, 1.0).width;
        draw_text(&mana, bx + (bw - w) / 2.0, rect.y + 51.0, 13.0, WHITE);
    }
    draw_text(
        format!(
            "DMG {}   ARM {}   MR {}%   MS {}",
            u.attack_damage,
            u.armor.to_f32().round() as i32,
            (u.magic_resist.to_f32() * 100.0).round() as i32,
            u.move_speed.to_f32().round() as i32,
        ),
        bx,
        rect.y + 78.0,
        15.0,
        WHITE,
    );
    draw_text(
        format!(
            "Range {}   Attacks {:.1}/s   AS {}",
            u.attack_range.to_f32().round() as i32,
            f64::from(rate) / f64::from(u.attack_interval.max(1)),
            u.attack_speed,
        ),
        bx,
        rect.y + 98.0,
        15.0,
        GRAY,
    );
    draw_attributes(u, bx, rect.y + 114.0);
}

/// The three attributes in a row, the one paying for damage marked with a dot.
fn draw_attributes(u: &UnitView, x: f32, y: f32) {
    if u.attributes == bota_proto::Attributes::ZERO {
        return;
    }
    let held = [
        (bota_proto::Attribute::Strength, u.attributes.strength),
        (bota_proto::Attribute::Agility, u.attributes.agility),
        (
            bota_proto::Attribute::Intelligence,
            u.attributes.intelligence,
        ),
    ];
    for (at, (which, points)) in held.iter().enumerate() {
        let mark = if u.primary == Some(*which) { "*" } else { "" };
        draw_text(
            format!(
                "{} {}{}",
                attribute_letter(*which),
                points.to_f32().floor() as i32,
                mark
            ),
            x + at as f32 * 62.0,
            y,
            14.0,
            attribute_color(*which),
        );
    }
}

/// The letter one attribute is shown by.
fn attribute_letter(which: bota_proto::Attribute) -> &'static str {
    match which {
        bota_proto::Attribute::Strength => "STR",
        bota_proto::Attribute::Agility => "AGI",
        bota_proto::Attribute::Intelligence => "INT",
    }
}

/// The colour one attribute is shown in.
fn attribute_color(which: bota_proto::Attribute) -> Color {
    match which {
        bota_proto::Attribute::Strength => Color::new(0.85, 0.35, 0.30, 1.0),
        bota_proto::Attribute::Agility => Color::new(0.45, 0.80, 0.40, 1.0),
        bota_proto::Attribute::Intelligence => Color::new(0.40, 0.65, 0.95, 1.0),
    }
}

/// Hotkey letters of the four ability slots.
const ABILITY_KEYS: [&str; 6] = ["Q", "W", "E", "R", "T", "G"];

/// The key that casts one slot, if the panel has one for it.
pub fn ability_key(slot: usize) -> Option<&'static str> {
    ABILITY_KEYS.get(slot).copied()
}
/// Whether an ability could take a skill point at this hero level.
fn learnable(id: u16, level: u8, hero_level: u8) -> bool {
    let Some(face) = crate::catalog::ability(id) else {
        return false;
    };
    if level >= face.max_level {
        return false;
    }
    let floor = if face.ultimate {
        [6, 8, 10]
            .get(usize::from(level))
            .copied()
            .unwrap_or(u8::MAX)
    } else {
        2 * (level + 1) - 1
    };
    hero_level >= floor
}

/// The right of the bottom panel: four ability slots and six item slots.
fn draw_slot_boxes(
    unit: Option<&UnitView>,
    rect: &crate::hud::UiRect,
    rate: u32,
    hero_level: u8,
    own: bool,
    pending: Option<u8>,
    held: Option<u8>,
) {
    let ax = rect.x + 435.0;
    let points = unit.map_or(0, |u| {
        let spent: u8 = u.abilities.iter().map(|a| a.level).sum();
        hero_level.saturating_sub(spent)
    });
    let carried = unit.map_or(0, |unit| unit.abilities.len());
    for (slot, r) in crate::hud::ability_boxes(rect) {
        let i = usize::from(slot);
        if i >= carried {
            continue;
        }
        let (cx, cy, cw, ch) = (r.x, r.y, r.w, r.h);
        draw_rectangle(cx, cy, cw, ch, Color::new(0.12, 0.12, 0.16, 1.0));
        let outline = if pending == Some(slot) { GOLD } else { GRAY };
        draw_rectangle_lines(cx, cy, cw, ch, 1.0, outline);
        match unit.and_then(|u| u.abilities.get(i)) {
            Some(a) => {
                draw_text(
                    ability_key(i).unwrap_or(""),
                    cx + 4.0,
                    cy + 16.0,
                    16.0,
                    WHITE,
                );
                let name = crate::catalog::ability(a.id.0).map_or("?", |face| face.name);
                draw_text(name, cx + 2.0, cy + 28.0, 11.0, LIGHTGRAY);
                if a.level > 0 && a.mana_cost > 0 {
                    draw_text(
                        format!("{}", a.mana_cost),
                        cx + cw - 16.0,
                        cy + 16.0,
                        11.0,
                        SKYBLUE,
                    );
                }
                for pip in 0..usize::from(crate::catalog::ability_cap(a.id.0)) {
                    let lit = pip < usize::from(a.level);
                    let color = if lit { GOLD } else { DARKGRAY };
                    draw_rectangle(cx + 4.0 + pip as f32 * 8.0, cy + ch - 7.0, 6.0, 4.0, color);
                }
                if a.level == 0 {
                    draw_rectangle(cx, cy, cw, ch, Color::new(0.0, 0.0, 0.0, 0.55));
                }
                if a.cooldown_left > 0 {
                    draw_rectangle(cx, cy, cw, ch, Color::new(0.0, 0.0, 0.0, 0.6));
                    let secs = a.cooldown_left / rate + 1;
                    draw_text(format!("{secs}"), cx + 13.0, cy + 25.0, 18.0, WHITE);
                }
                if own && points > 0 && learnable(a.id.0, a.level, hero_level) {
                    draw_text("+", cx + cw - 12.0, cy + ch - 4.0, 16.0, GOLD);
                }
            }
            None => {
                draw_text("-", cx + 16.0, cy + 26.0, 16.0, DARKGRAY);
            }
        }
    }
    if own && points > 0 {
        draw_text(
            format!("{points} pt: Ctrl+key"),
            ax,
            rect.y + 8.0,
            13.0,
            GOLD,
        );
    }
    let held = if own { held } else { None };
    // Only the slots this unit has: a courier carries, but has no pocket to
    // carry inert things in.
    let slots = unit.map_or(0, |unit| unit.items.len());
    for (slot, r) in crate::hud::item_boxes(rect) {
        if usize::from(slot) >= slots {
            continue;
        }
        let item = unit
            .and_then(|u| u.items.get(usize::from(slot)))
            .and_then(|s| s.as_ref());
        draw_item_box(&r, item, slot >= 6, held == Some(slot), rate);
    }
}

/// One item box: the icon, charges, the cooldown shade, the held outline.
fn draw_item_box(
    r: &crate::hud::UiRect,
    item: Option<&bota_proto::ItemView>,
    backpack: bool,
    held: bool,
    rate: u32,
) {
    let bg = if backpack {
        Color::new(0.08, 0.08, 0.10, 1.0)
    } else {
        Color::new(0.10, 0.10, 0.13, 1.0)
    };
    draw_rectangle(r.x, r.y, r.w, r.h, bg);
    let outline = if held { GOLD } else { DARKGRAY };
    draw_rectangle_lines(r.x, r.y, r.w, r.h, 1.0, outline);
    let Some(item) = item else {
        draw_text("-", r.x + 12.0, r.y + 17.0, 14.0, DARKGRAY);
        return;
    };
    if !crate::icons::draw_item_icon(item.id.0, r.x + 1.0, r.y + 1.0, r.w - 2.0, r.h - 2.0) {
        let name = crate::catalog::item(item.id.0).map_or("?", |face| face.name);
        draw_text(name, r.x + 2.0, r.y + 16.0, 12.0, WHITE);
    }
    if item.charges > 0 {
        draw_text(
            format!("{}", item.charges),
            r.x + r.w - 9.0,
            r.y + r.h - 3.0,
            11.0,
            GRAY,
        );
    }
    if let Some(mode) = item.mode {
        draw_text(
            attribute_letter(mode),
            r.x + 3.0,
            r.y + r.h - 3.0,
            13.0,
            attribute_color(mode),
        );
    }
    if backpack {
        draw_rectangle(r.x, r.y, r.w, r.h, Color::new(0.0, 0.0, 0.0, 0.45));
    }
    if item.cooldown_left > 0 {
        draw_rectangle(r.x, r.y, r.w, r.h, Color::new(0.0, 0.0, 0.0, 0.55));
        let secs = item.cooldown_left / rate + 1;
        draw_text(format!("{secs}"), r.x + 10.0, r.y + 18.0, 14.0, WHITE);
    }
}

/// The stash strip above the bottom panel, dimmed away from home.
fn draw_stash(app: &App, view: &WorldView) {
    let (sw, sh) = (screen_width(), screen_height());
    let panel = crate::hud::bottom_panel(sw, sh);
    let rate = u32::from(app.tick_rate.max(1));
    let Some(my) = app.my_slot else {
        return;
    };
    let Some(p) = view.players.iter().find(|pl| pl.slot == my) else {
        return;
    };
    let boxes = crate::hud::stash_boxes(&panel);
    let first = boxes.first().expect("six stash boxes");
    draw_text("STASH", first.1.x, first.1.y - 6.0, 13.0, GRAY);
    let home = app.at_home_shop();
    for (slot, r) in &boxes {
        let item = p
            .stash
            .as_ref()
            .and_then(|s| s.get(usize::from(slot - 9)))
            .and_then(|s| s.as_ref());
        draw_item_box(r, item, false, app.held_item == Some(*slot), rate);
        if !home {
            draw_rectangle(r.x, r.y, r.w, r.h, Color::new(0.0, 0.0, 0.0, 0.45));
        }
    }
}

/// The button toggling the shop panel.
fn draw_shop_button(app: &App) {
    let r = crate::hud::shop_button(screen_width(), screen_height());
    let bg = if app.shop_open {
        Color::new(0.25, 0.22, 0.10, 0.95)
    } else {
        Color::new(0.10, 0.10, 0.13, 0.95)
    };
    draw_rectangle(r.x, r.y, r.w, r.h, bg);
    draw_rectangle_lines(r.x, r.y, r.w, r.h, 1.0, GRAY);
    draw_text("SHOP (B)", r.x + 10.0, r.y + 19.0, 16.0, GOLD);
}

/// The shop panel. Open anywhere: purchases away from home land in the
/// stash.
fn draw_shop(app: &App, view: &WorldView) {
    let (sw, sh) = (screen_width(), screen_height());
    let Some(my) = app.my_slot else {
        return;
    };
    let Some(p) = view.players.iter().find(|pl| pl.slot == my) else {
        return;
    };
    let shop = crate::hud::shop_panel(sw, sh);
    draw_rectangle(
        shop.x,
        shop.y,
        shop.w,
        shop.h,
        Color::new(0.05, 0.06, 0.08, 0.95),
    );
    draw_rectangle_lines(
        shop.x,
        shop.y,
        shop.w,
        shop.h,
        2.0,
        Color::new(0.4, 0.4, 0.4, 1.0),
    );
    draw_text("SHOP", shop.x + 8.0, shop.y + 20.0, 18.0, WHITE);
    if !app.at_home_shop() {
        draw_text(
            "away from home: buys go to the stash",
            shop.x + 60.0,
            shop.y + 19.0,
            12.0,
            GRAY,
        );
    }
    let gold = p.gold.unwrap_or(0);
    let held = held_items(view, p);
    for (id, r) in crate::hud::shop_rows(&shop, crate::catalog::ITEMS.len(), app.shop_scroll) {
        let Some(face) = crate::catalog::item(id) else {
            continue;
        };
        let price = crate::catalog::price_for(id, &held);
        let affordable = gold >= price;
        let color = if affordable { WHITE } else { GRAY };
        let icon = r.h - 4.0;
        let drawn = crate::icons::draw_item_icon(id, r.x + 2.0, r.y + 2.0, icon * 1.5, icon);
        let text = if drawn {
            r.x + icon * 1.5 + 6.0
        } else {
            r.x + 4.0
        };
        draw_text(face.name, text, r.y + 16.0, 15.0, color);
        draw_text(face.stats, text + 62.0, r.y + 16.0, 13.0, DARKGRAY);
        // What the parts already in hand save is worth showing beside what
        // the whole would have cost.
        let shown = if price < face.cost {
            format!("{} / {}", price, face.cost)
        } else {
            format!("{price}")
        };
        let w = measure_text(&shown, None, 14, 1.0).width;
        draw_text(&shown, r.x + r.w - w - 4.0, r.y + 16.0, 14.0, GOLD);
    }
    let sell = crate::hud::sell_strip(&shop);
    draw_rectangle_lines(sell.x, sell.y, sell.w, sell.h, 1.0, DARKGRAY);
    let hint = if app.held_item.is_some() {
        "SELL: drop the item here"
    } else {
        "drag an item here to sell it"
    };
    draw_text(hint, sell.x + 6.0, sell.y + 18.0, 13.0, GRAY);
}

/// How far from an allied building a scroll may land, in world units.
pub const TELEPORT_RANGE: f32 = 600.0;

/// The circle a tree stands in, in world units.
pub const TREE_RADIUS: f32 = 48.0;

/// Every item one seat holds, in its hero's bag and in its stash.
///
/// What the shop charges is worked out against this, so a part already in hand
/// is not asked for twice.
fn held_items(view: &WorldView, p: &bota_proto::PlayerView) -> Vec<u16> {
    let bag = p
        .unit
        .and_then(|id| view.units.iter().find(|u| u.id == id))
        .map(|u| u.items.as_slice())
        .unwrap_or_default();
    bag.iter()
        .chain(p.stash.iter().flatten())
        .flatten()
        .map(|item| item.id.0)
        .collect()
}

/// The hover popup for whatever HUD element sits under the cursor.
fn draw_tooltips(app: &App, view: &WorldView) {
    if app.held_item.is_some() {
        return; // mid-drag the cursor means a drop spot, not a question
    }
    let (mx, my) = mouse_position();
    let (sw, sh) = (screen_width(), screen_height());
    let panel = crate::hud::bottom_panel(sw, sh);
    let rate = u32::from(app.tick_rate.max(1));
    let shown = match app.selection {
        Selection::Unit(id) => view.units.iter().find(|u| u.id == id),
        _ => {
            let slot = match app.selection {
                Selection::Seat(slot) => Some(slot),
                _ => app.default_slot(),
            };
            slot.and_then(|s| view.players.iter().find(|p| p.slot == s))
                .and_then(|p| p.unit)
                .and_then(|id| view.units.iter().find(|u| u.id == id))
        }
    };
    let mut tip: Option<(String, Vec<String>)> = None;
    if let Some(u) = shown {
        for (slot, r) in crate::hud::ability_boxes(&panel) {
            if r.contains(mx, my)
                && let Some(a) = u.abilities.get(usize::from(slot))
            {
                let face = crate::catalog::ability(a.id.0);
                let name = face.map_or("?", |face| face.name);
                let mut lines = vec![face.map_or("", |face| face.blurb).to_string()];
                if a.level == 0 {
                    lines.push("Not learned. Ctrl+key spends a point.".to_string());
                }
                if a.cooldown_left > 0 {
                    lines.push(format!("Ready in {}s.", a.cooldown_left / rate + 1));
                }
                tip = Some((
                    format!(
                        "{} {name}  lv{}",
                        ability_key(usize::from(slot)).unwrap_or(""),
                        a.level
                    ),
                    lines,
                ));
            }
        }
        for (slot, r) in crate::hud::item_boxes(&panel) {
            if r.contains(mx, my)
                && let Some(item) = u.items.get(usize::from(slot)).copied().flatten()
            {
                tip = Some(item_tip(&item, slot >= 6, rate));
            }
        }
    }
    if app.controls_selection() {
        let my_seat = app
            .my_slot
            .and_then(|s| view.players.iter().find(|p| p.slot == s));
        for (slot, r) in crate::hud::stash_boxes(&panel) {
            if r.contains(mx, my)
                && let Some(item) = my_seat
                    .and_then(|p| p.stash.as_ref())
                    .and_then(|s| s.get(usize::from(slot - 9)).copied().flatten())
            {
                let mut t = item_tip(&item, false, rate);
                if app.at_home_shop() {
                    t.1.push("In the stash: drag it into a free slot.".to_string());
                } else {
                    t.1.push("In the stash: opens at the home shop.".to_string());
                }
                tip = Some(t);
            }
        }
        if app.shop_open {
            let shop = crate::hud::shop_panel(sw, sh);
            let held = app
                .my_slot
                .and_then(|slot| view.players.iter().find(|p| p.slot == slot))
                .map_or_else(Vec::new, |p| held_items(view, p));
            for (id, r) in
                crate::hud::shop_rows(&shop, crate::catalog::ITEMS.len(), app.shop_scroll)
            {
                if r.contains(mx, my)
                    && let Some(face) = crate::catalog::item(id)
                {
                    let where_to = if app.at_home_shop() {
                        "Click to buy."
                    } else {
                        "Click to buy into the stash."
                    };
                    let price = crate::catalog::price_for(id, &held);
                    let mut lines = vec![face.blurb.to_string()];
                    if price < face.cost {
                        lines.push(format!(
                            "{} g of it is already in hand; {} g to pay.",
                            face.cost - price,
                            price
                        ));
                    }
                    lines.push(where_to.to_string());
                    tip = Some((format!("{}  {} g", face.name, price), lines));
                }
            }
        }
    }
    if let Some(u) = shown {
        let boxes = crate::hud::effect_boxes(&panel, u.effects.len());
        for (e, r) in u.effects.iter().zip(boxes) {
            if r.contains(mx, my) {
                let face = crate::catalog::effect(e.id.0);
                tip = Some((
                    face.map_or("?", |face| face.name).to_string(),
                    vec![
                        face.map_or("", |face| face.blurb).to_string(),
                        format!("{}s left.", e.ticks_left / rate + 1),
                    ],
                ));
            }
        }
    }
    if let Some((title, lines)) = tip {
        draw_tooltip_box(mx, my, &title, &lines);
    }
}

/// The popup lines of one owned item.
fn item_tip(item: &bota_proto::ItemView, backpack: bool, rate: u32) -> (String, Vec<String>) {
    let face = crate::catalog::item(item.id.0);
    let name = face.map_or("?", |face| face.name);
    let mut lines = vec![face.map_or("", |face| face.blurb).to_string()];
    if item.charges > 0 {
        lines.push(format!(
            "{} charge(s). Click it or press its number key to use.",
            item.charges
        ));
    }
    if backpack {
        lines.push("In the backpack: inactive. Drag it into the inventory.".to_string());
    }
    if item.cooldown_left > 0 {
        lines.push(format!("Ready in {}s.", item.cooldown_left / rate + 1));
    }
    (name.to_string(), lines)
}

/// A small text box pinned to the cursor.
fn draw_tooltip_box(mx: f32, my: f32, title: &str, lines: &[String]) {
    let mut w = measure_text(title, None, 15, 1.0).width;
    for line in lines {
        w = w.max(measure_text(line, None, 13, 1.0).width);
    }
    let w = w + 16.0;
    let h = 26.0 + lines.len() as f32 * 16.0;
    let x = (mx + 14.0).min(screen_width() - w - 4.0);
    let y = (my - h - 8.0).max(4.0);
    draw_rectangle(x, y, w, h, Color::new(0.04, 0.05, 0.07, 0.96));
    draw_rectangle_lines(x, y, w, h, 1.0, Color::new(0.5, 0.5, 0.5, 1.0));
    draw_text(title, x + 8.0, y + 17.0, 15.0, WHITE);
    for (i, line) in lines.iter().enumerate() {
        draw_text(line, x + 8.0, y + 33.0 + i as f32 * 16.0, 13.0, LIGHTGRAY);
    }
}

fn draw_over(app: &App, winner: Team) {
    let text = match winner {
        Team::Neutral => "NOBODY WINS",
        Team::Radiant => "RADIANT WINS",
        Team::Dire => "DIRE WINS",
    };
    draw_rectangle(
        0.0,
        screen_height() / 2.0 - 60.0,
        screen_width(),
        120.0,
        Color::new(0.0, 0.0, 0.0, 0.6),
    );
    center_text(text, 64.0, team_color(winner));
    center_text_at("Esc to leave", screen_height() / 2.0 + 44.0, 22.0, GRAY);
    let _ = app;
}

fn center_text(text: &str, size: f32, color: Color) {
    center_text_at(text, screen_height() / 2.0, size, color);
}

fn center_text_at(text: &str, y: f32, size: f32, color: Color) {
    let width = measure_text(text, None, size as u16, 1.0).width;
    draw_text(text, (screen_width() - width) / 2.0, y, size, color);
}
