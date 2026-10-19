//! A match before it is played: its lobby while it gathers, where seats are
//! taken and the host starts it, and the picking of heroes on its game
//! server.
//!
//! Drawing and input read the same rectangles from here, so a card is never
//! in one place and pressed in another.

use bota_proto::{HeroId, MatchState, PickSeat, Stage, Team};
use macroquad::prelude::*;

use crate::catalog::{HEROES, HeroFace};
use crate::hall::{BOX, EDGE, FAINT, INK, LEFT, Tone, lighter, tone_button};
use crate::hud::UiRect;
use crate::state::App;

/// Top of the seat rows.
pub const SEATS_TOP: f32 = 168.0;
/// Height of one seat row, the gap under it included.
pub const SEAT_H: f32 = 46.0;
/// Width of a hero card at its full size.
pub const CARD_W: f32 = 250.0;
/// Height of a hero card at its full size.
pub const CARD_H: f32 = 340.0;
/// Room between two cards at their full size.
const CARD_GAP: f32 = 24.0;
/// Smallest a card is drawn, against its full size.
const CARD_SHRINK: f32 = 0.5;

/// The Radiant's colour.
const RADIANT: Color = Color::new(0.30, 0.80, 0.35, 1.0);
/// The Dire's colour.
const DIRE: Color = Color::new(0.90, 0.30, 0.25, 1.0);
/// What marks this client's own: its seat and its pick.
const OWN: Color = Color::new(0.95, 0.78, 0.30, 1.0);
/// The square a card's number key sits on.
const BADGE: Color = Color::new(0.04, 0.04, 0.05, 1.0);
/// An empty portrait frame.
const NO_ART: Color = Color::new(0.08, 0.09, 0.11, 1.0);
/// The tint of a portrait of a hero selected and not picked.
const FADED: Color = Color::new(0.45, 0.45, 0.45, 1.0);
/// How a line is started, shown under the chat while it is empty.
const CHAT_HINT: &str = "[Enter] team   [Shift+Enter] all";

fn team_color(team: Team) -> Color {
    match team {
        Team::Radiant => RADIANT,
        Team::Dire => DIRE,
        Team::Neutral => FAINT,
    }
}

/// The right edge of the lobby's part of the screen, short of the chat.
fn right_edge(screen_w: f32, screen_h: f32) -> f32 {
    crate::hall::chat_panel(screen_w, screen_h).x - 30.0
}

/// How many seats the larger side has, of seats on these sides.
pub fn seats_per_side(teams: impl Iterator<Item = Team> + Clone) -> usize {
    [Team::Radiant, Team::Dire]
        .into_iter()
        .map(|team| teams.clone().filter(|seat| *seat == team).count())
        .max()
        .unwrap_or(0)
}

/// One seat row: by side, zero for the Radiant, and by place on that side.
pub fn seat_row(side: usize, at: usize, screen_w: f32, screen_h: f32) -> UiRect {
    let right = right_edge(screen_w, screen_h);
    let gap = 24.0;
    let w = ((right - LEFT - gap) / 2.0).max(120.0);
    UiRect {
        x: LEFT + (w + gap) * side as f32,
        y: SEATS_TOP + SEAT_H * at as f32,
        w,
        h: SEAT_H - 8.0,
    }
}

/// Top of the strip the buttons sit in.
fn bar_top(screen_h: f32) -> f32 {
    screen_h - 74.0
}

/// Where every hero's card is, in hero order, under the seats of a match
/// whose larger side has `per_side` seats. Cards shrink to fit above the
/// buttons, down to half their size, and wrap onto more rows when a row is
/// full.
pub fn hero_cards(heroes: usize, per_side: usize, screen_w: f32, screen_h: f32) -> Vec<UiRect> {
    let right = right_edge(screen_w, screen_h);
    let top = SEATS_TOP + SEAT_H * per_side.max(1) as f32 + 40.0;
    let room = bar_top(screen_h) - 16.0 - top;
    let scale = (room / CARD_H).clamp(CARD_SHRINK, 1.0);
    let (w, h, gap) = (CARD_W * scale, CARD_H * scale, CARD_GAP * scale);
    let per_row = (((right - LEFT + gap) / (w + gap)).floor() as usize).max(1);
    let span = |n: usize| n as f32 * w + n.saturating_sub(1) as f32 * gap;
    (0..heroes)
        .map(|at| {
            let (row, col) = (at / per_row, at % per_row);
            let in_row = (heroes - row * per_row).min(per_row);
            let start = LEFT + ((right - LEFT) - span(in_row)).max(0.0) / 2.0;
            UiRect {
                x: start + (w + gap) * col as f32,
                y: top + (h + gap) * row as f32,
                w,
                h,
            }
        })
        .collect()
}

/// The cards of the match whose heroes this client sees picked, laid out
/// for the screen.
pub fn cards(app: &App) -> Vec<UiRect> {
    let per_side = app.picking.as_ref().map_or(1, |picking| {
        seats_per_side(picking.seats.iter().map(|seat| seat.team))
    });
    hero_cards(HEROES.len(), per_side, screen_width(), screen_height())
}

/// The buttons of the strip, from its right end leftwards: the first is
/// the wide one.
fn bar_button(at: usize, screen_w: f32, screen_h: f32) -> UiRect {
    let right = right_edge(screen_w, screen_h);
    let (wide, narrow, gap) = (220.0, 150.0, 16.0);
    let x = if at == 0 {
        right - wide
    } else {
        right - wide - (narrow + gap) * at as f32
    };
    UiRect {
        x,
        y: bar_top(screen_h) + 14.0,
        w: if at == 0 { wide } else { narrow },
        h: 46.0,
    }
}

/// The button at the right of the strip: Start in the lobby, Pick while
/// heroes are picked.
pub fn action_button(screen_w: f32, screen_h: f32) -> UiRect {
    bar_button(0, screen_w, screen_h)
}

/// The button that leaves a gathering match.
pub fn leave_button(screen_w: f32, screen_h: f32) -> UiRect {
    bar_button(1, screen_w, screen_h)
}

/// The button that picks a hero at random.
pub fn random_button(screen_w: f32, screen_h: f32) -> UiRect {
    bar_button(1, screen_w, screen_h)
}

/// The button that leaves a match whose heroes are being picked.
pub fn picking_leave_button(screen_w: f32, screen_h: f32) -> UiRect {
    bar_button(2, screen_w, screen_h)
}

/// Whether every seat of a gathering match is held by somebody in it.
pub fn all_here(room: &MatchState) -> bool {
    room.slots
        .iter()
        .all(|slot| !slot.login.is_empty() && slot.connected)
}

/// Draws the name of a match and the line under it: its mode and number.
fn heading(app: &App, name: &str, mode: Option<bota_proto::ModeId>, id: Option<u64>) {
    label(name, LEFT, 66.0, 40.0, INK);
    let mode = mode
        .and_then(|mode| app.modes.iter().find(|m| m.id == mode))
        .map_or("", |m| m.name.as_str());
    let number = id.map(|id| format!("   match {id}")).unwrap_or_default();
    label(format!("{mode}{number}"), LEFT, 94.0, 20.0, FAINT);
}

/// Draws a side's name over its first seat.
fn side_name(team: Team, first: &UiRect) {
    let name = if team == Team::Radiant {
        "RADIANT"
    } else {
        "DIRE"
    };
    label(name, first.x, SEATS_TOP - 8.0, 18.0, team_color(team));
}

/// Draws the lobby of a gathering match: the seats of both sides, who may
/// start it, and the buttons.
pub fn draw_lobby(app: &App) {
    let (sw, sh) = (screen_width(), screen_height());
    let Some(room) = &app.room else {
        label("bota", LEFT, 80.0, 48.0, INK);
        label("waiting for the match...", LEFT, 130.0, 24.0, FAINT);
        return;
    };
    heading(app, &room.name, Some(room.mode), Some(room.id.0));
    let (status, tint) = lobby_status(app, room);
    label(&status, LEFT, 124.0, 22.0, tint);
    for (side, team) in [Team::Radiant, Team::Dire].into_iter().enumerate() {
        side_name(team, &seat_row(side, 0, sw, sh));
        for (at, slot) in room.slots.iter().filter(|s| s.team == team).enumerate() {
            let away = !slot.login.is_empty() && !slot.connected;
            let look = SeatLook {
                team,
                login: &slot.login,
                mine: app.my_slot == Some(slot.slot),
                portrait: None,
                faded: false,
                detail: "",
                badge: if away { ("AWAY", ORANGE) } else { ("", FAINT) },
            };
            draw_seat(&look, &seat_row(side, at, sw, sh));
        }
    }
    let per_side = seats_per_side(room.slots.iter().map(|slot| slot.team));
    let under = SEATS_TOP + SEAT_H * per_side.max(1) as f32 + 24.0;
    label(
        "Heroes are picked once the match starts, and a pick is final.",
        LEFT,
        under,
        20.0,
        FAINT,
    );
    let action = action_button(sw, sh);
    if app.hosts() && room.stage == Stage::Gathering {
        let tone = if all_here(room) { Tone::Go } else { Tone::Off };
        tone_button(&action, "Start", tone);
    }
    tone_button(&leave_button(sw, sh), "Leave", Tone::Plain);
    let held = room.slots.iter().filter(|s| !s.login.is_empty()).count();
    label(
        format!("{held} of {} seats taken", room.slots.len()),
        LEFT,
        action.y + 20.0,
        20.0,
        FAINT,
    );
    if !room.watchers.is_empty() {
        label(
            format!("watching: {}", room.watchers.join(", ")),
            LEFT,
            action.y + 42.0,
            18.0,
            FAINT,
        );
    }
    crate::chat::draw_box(app, &crate::hall::chat_panel(sw, sh), CHAT_HINT);
}

/// What the lobby waits for, in a line, and the colour to say it in.
fn lobby_status(app: &App, room: &MatchState) -> (String, Color) {
    if app.game_lost {
        return ("the game server could not be reached".to_string(), ORANGE);
    }
    if room.stage != Stage::Gathering {
        return ("the match is starting...".to_string(), OWN);
    }
    let open = room.slots.iter().filter(|s| s.login.is_empty()).count();
    let away = room
        .slots
        .iter()
        .filter(|s| !s.login.is_empty() && !s.connected)
        .count();
    let short = if open > 0 {
        Some(format!("{open} more to take a seat"))
    } else if away > 0 {
        Some(format!("{away} more to come back"))
    } else {
        None
    };
    match (room.host.is_some(), app.hosts(), short) {
        (false, _, Some(short)) => (
            format!("the match starts once everybody is here: waiting for {short}"),
            FAINT,
        ),
        (false, _, None) => (
            "everybody is here: the match is starting...".to_string(),
            OWN,
        ),
        (true, true, Some(short)) => (format!("waiting for {short}"), FAINT),
        (true, true, None) => ("everybody is here: press Start".to_string(), INK),
        (true, false, Some(short)) => (
            format!("waiting for {short}, then for the host to start"),
            FAINT,
        ),
        (true, false, None) => ("waiting for the host to start the match".to_string(), FAINT),
    }
}

/// Draws the picking of heroes: the time left, the seats of both sides and
/// the heroes they took, the heroes to take, and the buttons.
pub fn draw_picking(app: &App) {
    let (sw, sh) = (screen_width(), screen_height());
    let Some(picking) = &app.picking else {
        return;
    };
    let room = app.room.as_ref();
    heading(
        app,
        room.map_or("Pick your heroes", |room| room.name.as_str()),
        room.map(|room| room.mode),
        room.map(|room| room.id.0),
    );
    let left = crate::hall::clock(App::seconds_left(picking.ms_left, app.pick_age).ceil());
    let width = measure_text(&left, None, 40, 1.0).width;
    label(&left, right_edge(sw, sh) - width, 66.0, 40.0, OWN);
    let (status, tint) = picking_status(app, &picking.seats, &left);
    label(&status, LEFT, 124.0, 22.0, tint);
    for (side, team) in [Team::Radiant, Team::Dire].into_iter().enumerate() {
        side_name(team, &seat_row(side, 0, sw, sh));
        for (at, seat) in picking.seats.iter().filter(|s| s.team == team).enumerate() {
            let mine = app.my_slot == Some(seat.slot);
            let selected = if mine {
                app.my_selection()
            } else {
                seat.selected
            };
            let name = |hero: HeroId| crate::catalog::hero(hero.0).map_or("?", |face| face.name);
            let (detail, portrait, faded) = match (seat.hero, selected) {
                (Some(hero), _) => (name(hero).to_string(), Some(hero), false),
                (None, Some(hero)) => (format!("considering {}", name(hero)), Some(hero), true),
                (None, None) => ("picking...".to_string(), None, false),
            };
            let look = SeatLook {
                team,
                login: &seat.login,
                mine,
                portrait: Some(portrait),
                faded,
                detail: &detail,
                badge: ("", FAINT),
            };
            draw_seat(&look, &seat_row(side, at, sw, sh));
        }
    }
    let cards = cards(app);
    if let Some(first) = cards.first() {
        let title = match (app.my_slot, app.my_pick()) {
            (None, _) => "Heroes",
            (Some(_), None) => "Choose your hero, then press Pick",
            (Some(_), Some(_)) => "Your hero is picked",
        };
        label(title, LEFT, first.y - 12.0, 22.0, FAINT);
    }
    let can_pick = app.may_pick();
    for (face, rect) in HEROES.iter().zip(&cards) {
        draw_card(app, &picking.seats, face, rect, can_pick);
    }
    let action = action_button(sw, sh);
    if can_pick {
        let tone = if app.my_selection().is_some() {
            Tone::Go
        } else {
            Tone::Off
        };
        tone_button(&action, "Pick", tone);
        tone_button(&random_button(sw, sh), "Random", Tone::Plain);
    }
    tone_button(&picking_leave_button(sw, sh), "Leave", Tone::Plain);
    let picked = picking.seats.iter().filter(|s| s.hero.is_some()).count();
    label(
        format!("{picked} of {} heroes picked", picking.seats.len()),
        LEFT,
        action.y + 20.0,
        20.0,
        FAINT,
    );
    crate::chat::draw_box(app, &crate::hall::chat_panel(sw, sh), CHAT_HINT);
}

/// What the picking waits for, in a line, and the colour to say it in.
fn picking_status(app: &App, seats: &[PickSeat], left: &str) -> (String, Color) {
    if app.game_lost {
        return ("the game server could not be reached".to_string(), ORANGE);
    }
    let waiting = seats.iter().filter(|s| s.hero.is_none()).count();
    let selected = app
        .my_selection()
        .and_then(|hero| crate::catalog::hero(hero.0));
    match (app.my_slot, app.my_pick(), selected) {
        (None, _, _) => (format!("watching the picks: {left} left"), FAINT),
        (Some(_), None, None) => (
            format!("choose a hero and press Pick: {left} left, or one is picked at random"),
            INK,
        ),
        (Some(_), None, Some(face)) => (
            format!(
                "press Pick to take {}: {left} left, or it is picked for you",
                face.name
            ),
            INK,
        ),
        (Some(_), Some(_), _) if waiting > 0 => (
            format!("picked: waiting for {waiting} more, {left} at most"),
            FAINT,
        ),
        (Some(_), Some(_), _) => (
            "every hero is picked: the match begins".to_string(),
            RADIANT,
        ),
    }
}

/// How one seat row looks.
struct SeatLook<'a> {
    /// The side it plays for.
    team: Team,
    /// Who holds it. Empty while it is open.
    login: &'a str,
    /// Whether it is this client's.
    mine: bool,
    /// The portrait at its left: absent draws none, an absent hero an empty
    /// frame.
    portrait: Option<Option<HeroId>>,
    /// Whether the portrait is drawn dimmed: a hero selected, not picked.
    faded: bool,
    /// The line under the login. Empty draws the login alone.
    detail: &'a str,
    /// A word at the right edge and its colour. Empty draws none.
    badge: (&'a str, Color),
}

/// Draws one seat: the hero it took, who holds it, and a word on them.
fn draw_seat(look: &SeatLook, rect: &UiRect) {
    draw_rectangle(rect.x, rect.y, rect.w, rect.h, BOX);
    draw_rectangle(rect.x, rect.y, 4.0, rect.h, team_color(look.team));
    let mut text_x = rect.x + 16.0;
    if let Some(hero) = look.portrait {
        let thumb = UiRect {
            x: rect.x + 10.0,
            y: rect.y + 3.0,
            w: (rect.h - 6.0) * 1.5,
            h: rect.h - 6.0,
        };
        let tint = if look.faded { FADED } else { WHITE };
        let drawn = hero.is_some_and(|hero| {
            crate::icons::draw_hero_portrait(hero.0, thumb.x, thumb.y, thumb.w, thumb.h, tint)
        });
        if !drawn {
            draw_rectangle(thumb.x, thumb.y, thumb.w, thumb.h, NO_ART);
            label(
                "?",
                thumb.x + thumb.w / 2.0 - 5.0,
                thumb.y + 24.0,
                22.0,
                FAINT,
            );
        }
        draw_rectangle_lines(thumb.x, thumb.y, thumb.w, thumb.h, 1.0, EDGE);
        text_x = thumb.x + thumb.w + 12.0;
    }
    if look.login.is_empty() {
        label("open seat", text_x, rect.y + 25.0, 20.0, FAINT);
    } else {
        let tint = if look.mine { OWN } else { INK };
        if look.detail.is_empty() {
            label(look.login, text_x, rect.y + 25.0, 20.0, tint);
        } else {
            label(look.login, text_x, rect.y + 17.0, 20.0, tint);
            label(look.detail, text_x, rect.y + 33.0, 16.0, FAINT);
        }
        let (badge, badge_tint) = look.badge;
        if !badge.is_empty() {
            let width = measure_text(badge, None, 18, 1.0).width;
            label(
                badge,
                rect.x + rect.w - 12.0 - width,
                rect.y + 25.0,
                18.0,
                badge_tint,
            );
        }
    }
    if look.mine {
        draw_rectangle_lines(rect.x, rect.y, rect.w, rect.h, 2.0, OWN);
    }
}

/// Draws one hero's card: the portrait, the name, how it plays, its
/// abilities, who picked it, and who is considering it.
fn draw_card(app: &App, seats: &[PickSeat], face: &HeroFace, rect: &UiRect, can_pick: bool) {
    let s = rect.w / CARD_W;
    let this = HeroId(face.id);
    let lit = can_pick && crate::hall::hovered(rect);
    let mine = app.my_pick() == Some(this);
    let chosen = app.my_selection() == Some(this);
    let fill = if lit || chosen {
        lighter(BOX, 0.05)
    } else {
        BOX
    };
    draw_rectangle(rect.x, rect.y, rect.w, rect.h, fill);
    let pad = 8.0 * s;
    let art = UiRect {
        x: rect.x + pad,
        y: rect.y + pad,
        w: rect.w - 2.0 * pad,
        h: (rect.w - 2.0 * pad) * 2.0 / 3.0,
    };
    // Cards wait dimmed for a pointer while a pick is open, but for the one
    // selected, and stay dimmed past the one picked once it is made.
    let dim = (can_pick && !lit && !chosen) || (app.my_pick().is_some() && !mine);
    let tint = if dim {
        Color::new(0.80, 0.80, 0.80, 1.0)
    } else {
        WHITE
    };
    if !crate::icons::draw_hero_portrait(face.id, art.x, art.y, art.w, art.h, tint) {
        draw_rectangle(art.x, art.y, art.w, art.h, NO_ART);
        let initial = &face.name[..1];
        label(
            initial,
            art.x + art.w / 2.0 - 12.0 * s,
            art.y + art.h / 2.0 + 16.0 * s,
            48.0 * s,
            FAINT,
        );
    }
    if can_pick {
        let key = format!("{}", face.id + 1);
        draw_rectangle(art.x, art.y, 22.0 * s, 22.0 * s, BADGE);
        label(&key, art.x + 6.0 * s, art.y + 17.0 * s, 18.0 * s, INK);
    }
    let ribbon = if mine {
        Some(("YOUR PICK", OWN))
    } else if chosen {
        Some(("SELECTED", INK))
    } else {
        None
    };
    if let Some((ribbon, back)) = ribbon {
        let size = 16.0 * s;
        let width = measure_text(ribbon, None, size as u16, 1.0).width + 12.0 * s;
        draw_rectangle(art.x + art.w - width, art.y, width, 22.0 * s, back);
        label(
            ribbon,
            art.x + art.w - width + 6.0 * s,
            art.y + 16.0 * s,
            size,
            Color::new(0.10, 0.08, 0.02, 1.0),
        );
    }
    let text_x = rect.x + pad;
    let width = rect.w - 2.0 * pad;
    let mut y = art.y + art.h + 28.0 * s;
    label(face.name, text_x, y, 26.0 * s, INK);
    y += 20.0 * s;
    label(face.kind, text_x, y, 17.0 * s, OWN);
    y += 22.0 * s;
    for line in wrap(face.blurb, width, 16.0 * s) {
        label(
            &line,
            text_x,
            y,
            16.0 * s,
            Color::new(0.74, 0.77, 0.83, 1.0),
        );
        y += 18.0 * s;
    }
    y += 4.0 * s;
    let abilities = face
        .abilities
        .iter()
        .filter_map(|id| crate::catalog::ability(*id))
        .map(|ability| ability.name)
        .collect::<Vec<_>>()
        .join(", ");
    for line in wrap(&abilities, width, 15.0 * s) {
        label(&line, text_x, y, 15.0 * s, FAINT);
        y += 17.0 * s;
    }
    // Who picked it, in filled chips, then who of the seats this client may
    // see is considering it, in hollow ones; its own selection is the
    // ribbon.
    let picked = seats.iter().filter(|seat| seat.hero == Some(this));
    let considering = seats.iter().filter(|seat| {
        seat.hero.is_none() && seat.selected == Some(this) && app.my_slot != Some(seat.slot)
    });
    let mut chip_x = text_x;
    let chip_y = rect.y + rect.h - pad - 20.0 * s;
    for (seat, filled) in picked
        .map(|seat| (seat, true))
        .chain(considering.map(|seat| (seat, false)))
    {
        let size = 15.0 * s;
        let chip_w = measure_text(&seat.login, None, size as u16, 1.0).width + 12.0 * s;
        if chip_x + chip_w > rect.x + rect.w - pad {
            break;
        }
        let base = team_color(seat.team);
        if filled {
            draw_rectangle(
                chip_x,
                chip_y,
                chip_w,
                20.0 * s,
                Color::new(base.r * 0.35, base.g * 0.35, base.b * 0.35, 1.0),
            );
        }
        draw_rectangle_lines(chip_x, chip_y, chip_w, 20.0 * s, 1.0, base);
        let text = if filled { INK } else { FAINT };
        label(&seat.login, chip_x + 6.0 * s, chip_y + 15.0 * s, size, text);
        chip_x += chip_w + 6.0 * s;
    }
    let (edge, thick) = if mine {
        (OWN, 3.0)
    } else if chosen {
        (OWN, 2.0)
    } else if lit {
        (INK, 2.0)
    } else {
        (EDGE, 1.0)
    };
    draw_rectangle_lines(rect.x, rect.y, rect.w, rect.h, thick, edge);
}

/// Draws text set on whole pixels at a whole size, so no letter takes in a
/// sliver of its neighbours in the font's atlas.
fn label(text: impl AsRef<str>, x: f32, y: f32, size: f32, color: Color) {
    draw_text(text, x.round(), y.round(), size.round(), color);
}

/// Text split into lines no wider than a width at a size.
fn wrap(text: &str, width: f32, size: f32) -> Vec<String> {
    let mut lines = Vec::new();
    let mut line = String::new();
    for word in text.split_whitespace() {
        let longer = if line.is_empty() {
            word.to_string()
        } else {
            format!("{line} {word}")
        };
        if !line.is_empty() && measure_text(&longer, None, size as u16, 1.0).width > width {
            lines.push(std::mem::replace(&mut line, word.to_string()));
        } else {
            line = longer;
        }
    }
    if !line.is_empty() {
        lines.push(line);
    }
    lines
}
