//! Keys and mouse into orders and camera motion.

use bota_proto::{
    AbilitySlot, Aim, Auth, EntityId, ItemId, ItemSlot, MAX_CHAT_CHARS, Order, Role, Stage, Target,
    Team, ToGame, ToHub, TournamentFormat, TournamentStage, UnitKind, Vec2, WorldView,
};
use macroquad::prelude::*;

use crate::slots::{Press, Slot, order_for, press};
use crate::state::{App, Confirm, Draft, Phase, Source, Tap};

/// Handles one frame of input.
pub fn handle(app: &mut App) {
    if found_controls(app) {
        return;
    }
    if prompt_controls(app) {
        return;
    }
    if confirm_controls(app) {
        return;
    }
    if chat_controls(app) {
        return;
    }
    if is_key_pressed(KeyCode::Escape) {
        if app.attack_move_armed || app.aiming.is_some() || app.held_item.is_some() {
            app.attack_move_armed = false;
            app.aiming = None;
            app.held_item = None;
        } else if app.shop_open {
            app.shop_open = false;
        } else if app.watching.is_some() {
            app.stop_watching();
        } else if matches!(app.source, Source::Replay(_)) {
            app.quit = true;
        } else if matches!(app.phase, Phase::Lobby | Phase::Picking | Phase::Playing) {
            app.source.tell_hub(&ToHub::LeaveMatch);
            app.leave_room();
        } else if app.phase == Phase::Cup {
            app.close_cup();
        } else {
            app.quit = true;
        }
    }
    if app.phase == Phase::Playing && app.my_slot.is_some() && is_key_pressed(KeyCode::B) {
        app.shop_open = !app.shop_open;
    }
    // A live spectator borrows eyes with Tab: everything, then each seat in
    // turn.
    if app.phase == Phase::Playing
        && app.my_slot.is_none()
        && !app.replaying()
        && is_key_pressed(KeyCode::Tab)
    {
        let seats: Vec<bota_proto::SlotId> = app
            .view
            .as_ref()
            .map(|view| view.players.iter().map(|player| player.slot).collect())
            .unwrap_or_default();
        app.eyes = match app.eyes {
            None => seats.first().copied(),
            Some(current) => seats
                .iter()
                .position(|slot| *slot == current)
                .and_then(|at| seats.get(at + 1))
                .copied(),
        };
        let seat = app.eyes;
        app.source.tell_game(&ToGame::ViewAs { seat });
    }
    if app.phase == Phase::SignIn {
        signin_controls(app);
        return;
    }
    if app.phase == Phase::Hall {
        hall_controls(app);
        return;
    }
    if app.phase == Phase::Cup {
        cup_controls(app);
        return;
    }
    camera_controls(app);
    match app.phase {
        Phase::SignIn | Phase::Hall | Phase::Cup => {}
        Phase::Lobby => lobby_controls(app),
        Phase::Picking => picking_controls(app),
        Phase::Playing => {
            pick_controls(app);
            replay_controls(app);
            let ui_click = ui_clicks(app);
            if !ui_click {
                // A click that confirms an armed order never doubles as a
                // selection of whatever it landed on.
                let armed = app.attack_move_armed || app.aiming.is_some();
                if app.over.is_none() {
                    order_controls(app);
                }
                if !armed {
                    selection_clicks(app);
                }
            }
        }
    }
}

/// Opens a chat line, types into it while it is open, and sends it on Enter.
///
/// In a match Enter opens a line to one's own side and Shift+Enter one to
/// everybody; on the hall's screens T opens one. A slash opens one with the
/// slash already typed, for a command. Tab turns a line between one's side
/// and everybody, Escape drops it. Returns whether the line took the frame's
/// keys, so typing never doubles as hotkeys.
fn chat_controls(app: &mut App) -> bool {
    let Some(draft) = app.draft.as_mut() else {
        app.draft = chat_opened(app);
        if app.draft.is_none() {
            return false;
        }
        while get_char_pressed().is_some() {}
        return true;
    };
    while let Some(typed) = get_char_pressed() {
        if !typed.is_control() && draft.text.chars().count() < MAX_CHAT_CHARS {
            draft.text.push(typed);
        }
    }
    if is_key_pressed(KeyCode::Backspace) {
        draft.text.pop();
    }
    if is_key_pressed(KeyCode::Tab) {
        draft.team = !draft.team;
    }
    if is_key_pressed(KeyCode::Escape) {
        app.draft = None;
    } else if is_key_pressed(KeyCode::Enter)
        && let Some(said) = app.draft.take()
        && !said.text.trim().is_empty()
    {
        app.say(said.text, said.team);
    }
    true
}

/// The chat line a key opens on this screen this frame, if one does.
fn chat_opened(app: &App) -> Option<Draft> {
    if app.me.is_none() || app.replaying() {
        return None;
    }
    let in_match = matches!(app.phase, Phase::Lobby | Phase::Picking | Phase::Playing);
    let in_hall = matches!(app.phase, Phase::Hall | Phase::Cup);
    let shift = is_key_down(KeyCode::LeftShift) || is_key_down(KeyCode::RightShift);
    if (in_match || in_hall) && is_key_pressed(KeyCode::Slash) {
        return Some(Draft {
            text: "/".to_string(),
            team: false,
        });
    }
    if in_match && is_key_pressed(KeyCode::Enter) {
        return Some(Draft {
            text: String::new(),
            team: !shift,
        });
    }
    if in_hall && is_key_pressed(KeyCode::T) {
        return Some(Draft {
            text: String::new(),
            team: false,
        });
    }
    None
}

/// Handles clicks landing on the HUD. Returns whether one did, so a click on
/// a panel never doubles as an order into the world behind it.
fn ui_clicks(app: &mut App) -> bool {
    let left = is_mouse_button_pressed(MouseButton::Left);
    let right = is_mouse_button_pressed(MouseButton::Right);
    let released = is_mouse_button_released(MouseButton::Left);
    if !left && !right && !released {
        return false;
    }
    let Some(view) = &app.view else {
        return false;
    };
    let (mx, my) = mouse_position();
    let (sw, sh) = (screen_width(), screen_height());
    // Letting go of a dragged item resolves the drag wherever it lands.
    if released {
        if app.held_item.is_some() {
            finish_item_drag(app, mx, my, sw, sh);
            return true;
        }
        return false;
    }
    for (slot, rect) in crate::hud::top_portraits(&view.players, sw) {
        if rect.contains(mx, my) {
            if left {
                // The body a seat last stood in, so a hero in the fog or one
                // that has fallen is still something to pick.
                let body = app.body_of(slot);
                let again = app.tapped_twice(Tap::Seat(slot));
                app.choose(body, again);
            }
            return true;
        }
    }
    // The minimap: right click orders a move there, A-click an attack-move,
    // a plain left click sends the camera.
    let minimap = crate::hud::minimap(sh);
    if minimap.contains(mx, my) {
        let wx = (mx - minimap.x) / minimap.w * 18432.0;
        let wy = (1.0 - (my - minimap.y) / minimap.h) * 18432.0;
        let ground = world_vec(wx, wy);
        let commands = app.controls_selection() && app.hero_stands();
        if right && commands {
            app.send_order(Order::Move {
                target: Target::Pos(ground),
            });
        } else if left && app.attack_move_armed && commands {
            app.attack_move_armed = false;
            app.send_order(Order::Attack {
                target: Target::Pos(ground),
            });
        } else if left {
            // Sending the camera somewhere lets go of what carried it, or it
            // would be pulled straight back.
            app.pinned = None;
            app.camera.x = wx;
            app.camera.y = wy;
        }
        return true;
    }
    if app.my_slot.is_some() && crate::hud::shop_button(sw, sh).contains(mx, my) {
        if left {
            app.shop_open = !app.shop_open;
        }
        return true;
    }
    let panel = crate::hud::bottom_panel(sw, sh);
    if app.controls_selection() {
        // Only the boxes this unit actually carries: the rest are not drawn,
        // and a box that is not drawn must not swallow a click either.
        let carried = match app.slot_unit() {
            Some(unit) => unit.abilities.len(),
            None => app.slot_kit().map_or(0, |kit| kit.abilities.len()),
        };
        for (slot, rect) in crate::hud::ability_boxes(&panel) {
            if usize::from(slot) < carried && rect.contains(mx, my) {
                if left {
                    let ctrl =
                        is_key_down(KeyCode::LeftControl) || is_key_down(KeyCode::RightControl);
                    do_press(app, Slot::Ability(slot), ctrl);
                } else if right {
                    app.aiming = None;
                }
                return true;
            }
        }
        if let Some(slot) = item_box_under(mx, my, sw, sh) {
            if left && app.item_at(slot) {
                app.held_item = Some(slot);
            } else if right {
                app.held_item = None;
                app.aiming = None;
            }
            return true;
        }
        if app.shop_open {
            let shop = crate::hud::shop_panel(sw, sh);
            if shop.contains(mx, my) {
                if left {
                    for (id, rect) in
                        crate::hud::shop_rows(&shop, crate::catalog::ITEMS.len(), app.shop_scroll)
                    {
                        if rect.contains(mx, my) {
                            app.send_order(Order::Buy { item: ItemId(id) });
                        }
                    }
                }
                return true;
            }
        }
    }
    panel.contains(mx, my)
}

/// The carried or stash item box under the cursor.
fn item_box_under(mx: f32, my: f32, sw: f32, sh: f32) -> Option<u8> {
    let panel = crate::hud::bottom_panel(sw, sh);
    for (slot, rect) in crate::hud::item_boxes(&panel) {
        if rect.contains(mx, my) {
            return Some(slot);
        }
    }
    for (slot, rect) in crate::hud::stash_boxes(&panel) {
        if rect.contains(mx, my) {
            return Some(slot);
        }
    }
    None
}

/// Ends an item drag: dropped on itself a consumable is used, on another box
/// it moves, on the sell strip it is sold or marked for sale, on an ally's
/// portrait it is handed over, and over the open world it is put out of the
/// bag — into an allied bag under the cursor, or onto the ground. The rest
/// of the HUD swallows the drop.
fn finish_item_drag(app: &mut App, mx: f32, my: f32, sw: f32, sh: f32) {
    let Some(from) = app.held_item.take() else {
        return;
    };
    if let Some(to) = item_box_under(mx, my, sw, sh) {
        if to == from {
            // Taken up and set down where it came from, a click is a press of
            // that slot and nothing more.
            do_press(app, Slot::Item(from), false);
        } else {
            app.send_order(Order::Swap {
                from: ItemSlot(from),
                to: ItemSlot(to),
            });
        }
        return;
    }
    if app.shop_open {
        let shop = crate::hud::shop_panel(sw, sh);
        if crate::hud::sell_strip(&shop).contains(mx, my) {
            app.send_order(Order::Sell {
                slot: ItemSlot(from),
            });
            return;
        }
        if shop.contains(mx, my) {
            return;
        }
    }
    // A drop on an ally's portrait hands the item to that seat's body,
    // wherever it stands: the walk is the server's business.
    let portraits = app
        .view
        .as_ref()
        .map(|view| crate::hud::top_portraits(&view.players, sw))
        .unwrap_or_default();
    for (seat, rect) in portraits {
        if !rect.contains(mx, my) {
            continue;
        }
        let ally = app
            .view
            .as_ref()
            .and_then(|view| view.players.iter().find(|p| p.slot == seat))
            .is_some_and(|p| Some(p.team) == app.my_team());
        if from < crate::slots::BAG_SLOTS
            && ally
            && let Some(body) = app.body_of(seat)
        {
            app.send_order(Order::Put {
                slot: ItemSlot(from),
                target: Target::Unit(body),
            });
        }
        return;
    }
    if crate::hud::bottom_panel(sw, sh).contains(mx, my)
        || crate::hud::minimap(sh).contains(mx, my)
        || crate::hud::shop_button(sw, sh).contains(mx, my)
    {
        return;
    }
    // Over the open world. What the stash holds stays put: it is a shelf at
    // the shop, not a pair of hands.
    if from >= crate::slots::BAG_SLOTS {
        return;
    }
    let (wx, wy) = app.camera.screen_to_world(mx, my, sw, sh);
    // Whoever the panel is about is the one that puts: the courier lays its
    // own load out the same way the hero does. Its own body under the cursor
    // reads as the ground at its feet.
    let carrier = app.commanded();
    let target = app
        .view
        .as_ref()
        .and_then(|view| unit_under_cursor(view, wx, wy, carrier, true))
        .filter(|id| takes_a_handover(app, *id))
        .map(Target::Unit)
        .unwrap_or(Target::Pos(world_vec(wx, wy)));
    app.send_order(Order::Put {
        slot: ItemSlot(from),
        target,
    });
}

/// Whether a unit is one an item can be handed to: an allied hero or courier.
fn takes_a_handover(app: &App, id: EntityId) -> bool {
    let Some(view) = &app.view else {
        return false;
    };
    view.units.iter().find(|u| u.id == id).is_some_and(|u| {
        Some(u.team) == app.my_team() && matches!(u.kind, UnitKind::Hero | UnitKind::Courier)
    })
}

/// A free left click picks what it lands on, whatever that is; the ground
/// picks nothing. A second click on the same one pins the camera to it.
fn selection_clicks(app: &mut App) {
    if !is_mouse_button_pressed(MouseButton::Left) {
        return;
    }
    let (sx, sy) = mouse_position();
    let (wx, wy) = app
        .camera
        .screen_to_world(sx, sy, screen_width(), screen_height());
    let Some(view) = &app.view else {
        return;
    };
    // One rule for everything that stands: a hero, a courier, a creep, a
    // building, one's own side or the other. A click picks it; a second click
    // on the same one pins the camera to it.
    let hit = unit_under_cursor(view, wx, wy, None, false);
    let again = hit.is_some_and(|id| app.tapped_twice(Tap::Unit(id)));
    app.choose(hit, again);
}

/// How near an edge the cursor drives the camera that way, in pixels.
const EDGE_MARGIN: f32 = 6.0;

/// Which ability of a courier fetches the stash.
const TAKE_STASH: u16 = 10;

/// F1 picks one's own hero and F2 one's own courier; pressed twice, either
/// one pins the camera to what it picked. F3 sends the courier for the stash
/// without looking away from the fight, and picks nothing.
fn pick_controls(app: &mut App) {
    if is_key_pressed(KeyCode::F1) {
        let hero = app.my_hero();
        let again = app.tapped_twice(Tap::Hero);
        app.choose(hero, again);
    }
    if is_key_pressed(KeyCode::F2)
        && let Some(courier) = app.my_courier()
    {
        let again = app.tapped_twice(Tap::Courier);
        app.choose(Some(courier), again);
    }
    if is_key_pressed(KeyCode::F3)
        && let Some(courier) = app.my_courier()
        && let Some(slot) = app.courier_slot(TAKE_STASH)
    {
        app.send_order_to(
            Some(courier),
            Order::Cast {
                slot: AbilitySlot(slot),
                target: Target::None,
            },
        );
    }
}

/// Types a name the client asked for. Returns whether the box took the
/// frame's keys, so typing never doubles as anything else.
fn prompt_controls(app: &mut App) -> bool {
    if app.asking.is_none() {
        return false;
    }
    while let Some(typed) = get_char_pressed() {
        if !typed.is_control() {
            app.entry.push(typed);
        }
    }
    if is_key_pressed(KeyCode::Backspace) {
        app.entry.pop();
    }
    if is_key_pressed(KeyCode::Escape) {
        app.asking = None;
        app.entry.clear();
    } else if is_key_pressed(KeyCode::Enter) {
        app.confirm_entry();
    }
    true
}

/// Answers the question waiting for a yes or a no. Returns whether one was
/// waiting, so nothing behind it takes the frame's keys or clicks.
fn confirm_controls(app: &mut App) -> bool {
    let Some(confirm) = app.confirm else {
        return false;
    };
    let (sw, sh) = (screen_width(), screen_height());
    let (mx, my) = mouse_position();
    let clicked = is_mouse_button_pressed(MouseButton::Left);
    if is_key_pressed(KeyCode::Escape)
        || (clicked && crate::hall::confirm_no(sw, sh).contains(mx, my))
    {
        app.confirm = None;
    } else if clicked && crate::hall::confirm_yes(sw, sh).contains(mx, my) {
        match confirm {
            Confirm::Abandon(id) => app.abandon(id),
        }
    }
    true
}

/// Answers a match the queue found: Enter or a click accepts, a click on
/// Decline turns it down. Returns whether one was offered, so nothing behind
/// it takes the frame's keys or clicks.
fn found_controls(app: &mut App) -> bool {
    if !matches!(app.search, bota_proto::Search::Found { .. }) {
        return false;
    }
    let (sw, sh) = (screen_width(), screen_height());
    let (mx, my) = mouse_position();
    let clicked = is_mouse_button_pressed(MouseButton::Left);
    while get_char_pressed().is_some() {}
    if is_key_pressed(KeyCode::Enter)
        || (clicked && crate::hall::found_accept(sw, sh).contains(mx, my))
    {
        app.accept_found();
    } else if clicked && crate::hall::found_decline(sw, sh).contains(mx, my) {
        app.decline_found();
    }
    true
}

/// Presses the buttons of the panel at the bottom right: a match offered
/// back, or the search going on. Returns whether a click landed on one.
fn corner_controls(app: &mut App) -> bool {
    if !is_mouse_button_pressed(MouseButton::Left) {
        return false;
    }
    let (sw, sh) = (screen_width(), screen_height());
    let (mx, my) = mouse_position();
    if let Some((id, stage)) = app.match_to_rejoin().map(|m| (m.id, m.stage)) {
        if crate::hall::rejoin_button(sw, sh).contains(mx, my) {
            app.rejoin(id);
            return true;
        }
        let started = matches!(stage, Stage::Picking | Stage::Playing);
        if started && crate::hall::abandon_button(sw, sh).contains(mx, my) {
            app.confirm = Some(Confirm::Abandon(id));
            return true;
        }
        return false;
    }
    if app.searching() && crate::hall::abandon_button(sw, sh).contains(mx, my) {
        app.cancel_search();
        return true;
    }
    false
}

/// Presses the buttons of the tournament screen and its bracket.
fn cup_controls(app: &mut App) {
    if corner_controls(app) {
        return;
    }
    let (mx, my) = mouse_position();
    let clicked = is_mouse_button_pressed(MouseButton::Left);
    let cup = app.tournament.as_ref().map(|cup| (cup.id, cup.stage));
    if is_key_pressed(KeyCode::Enter)
        && let Some((id, TournamentStage::Registration)) = cup
    {
        app.ask(crate::state::Asking::JoinTeam(id));
        return;
    }
    if !clicked {
        return;
    }
    if let Some(cup) = &app.tournament {
        let lines = crate::hall::bracket_lines(cup, &app.hall_matches);
        for line in lines {
            let Some(action) = line.action else {
                continue;
            };
            let button = crate::hud::UiRect {
                x: line.rect.x + line.rect.w,
                y: line.rect.y,
                w: crate::hall::ACT_W,
                h: line.rect.h,
            };
            if !button.contains(mx, my) {
                continue;
            }
            let Some(match_id) = line.game else {
                continue;
            };
            if action == "Watch" {
                app.source.tell_hub(&ToHub::JoinMatch {
                    id: match_id,
                    role: Role::Spectator,
                });
            } else {
                app.source.tell_hub(&ToHub::FetchReplay { match_id });
            }
            return;
        }
    }
    let actions = crate::hall::cup_actions(app);
    let Some(at) = (0..actions.len())
        .find(|at| crate::hall::cup_button(*at, screen_height()).contains(mx, my))
    else {
        return;
    };
    let Some((id, _)) = cup else {
        app.close_cup();
        return;
    };
    match actions[at] {
        "Back" => app.close_cup(),
        "Join team" => app.ask(crate::state::Asking::JoinTeam(id)),
        "Leave" => app.source.tell_hub(&ToHub::LeaveTournament),
        "Start" => app.source.tell_hub(&ToHub::StartTournament { id }),
        "Cancel" => app.source.tell_hub(&ToHub::CancelTournament { id }),
        _ => {}
    }
}

/// Types a login and a password, and sends them.
fn signin_controls(app: &mut App) {
    let typed: Vec<char> = std::iter::from_fn(get_char_pressed).collect();
    let field = app.field;
    let line = match field {
        crate::state::Field::Login => &mut app.login,
        crate::state::Field::Password => &mut app.password,
    };
    for ch in typed {
        if !ch.is_control() {
            line.push(ch);
        }
    }
    if is_key_pressed(KeyCode::Backspace) {
        line.pop();
    }
    if is_key_pressed(KeyCode::Tab) {
        app.field = match field {
            crate::state::Field::Login => crate::state::Field::Password,
            crate::state::Field::Password => crate::state::Field::Login,
        };
    }
    let (mx, my) = mouse_position();
    let clicked = is_mouse_button_pressed(MouseButton::Left);
    if clicked {
        if crate::hall::entry(0).contains(mx, my) {
            app.field = crate::state::Field::Login;
        }
        if crate::hall::entry(1).contains(mx, my) {
            app.field = crate::state::Field::Password;
        }
    }
    let pressed = |at: usize| clicked && crate::hall::entry_button(at).contains(mx, my);
    let sign_in = is_key_pressed(KeyCode::Enter) || pressed(0);
    if sign_in && !app.login.is_empty() {
        let auth = Auth::Login {
            login: app.login.clone(),
            password: app.password.clone(),
        };
        app.say_hello(auth);
    } else if pressed(1) && !app.login.is_empty() {
        let auth = Auth::Register {
            login: app.login.clone(),
            password: app.password.clone(),
        };
        app.say_hello(auth);
    } else if pressed(2) && !app.login.is_empty() {
        let auth = Auth::Guest {
            name: app.login.clone(),
        };
        app.say_hello(auth);
    }
}

/// Picks a list, a mode, and what to do with a row of it.
fn hall_controls(app: &mut App) {
    if corner_controls(app) {
        return;
    }
    let (mx, my) = mouse_position();
    let clicked = is_mouse_button_pressed(MouseButton::Left);
    let (_, wheel) = mouse_wheel();
    if wheel != 0.0 {
        let by = if wheel > 0.0 { -1 } else { 1 };
        app.hall_scroll = app.hall_scroll.saturating_add_signed(by);
    }
    if is_key_pressed(KeyCode::Tab) {
        let at = crate::hall::TABS
            .iter()
            .position(|(tab, _)| *tab == app.tab)
            .unwrap_or(0);
        app.tab = crate::hall::TABS[(at + 1) % crate::hall::TABS.len()].0;
        app.hall_scroll = 0;
        if app.tab == crate::state::HallTab::Replays {
            app.ask_for_replays(None);
        }
    }
    for (at, (tab, _)) in crate::hall::TABS.into_iter().enumerate() {
        if clicked && crate::hall::tab(at).contains(mx, my) {
            app.tab = tab;
            app.hall_scroll = 0;
            if tab == crate::state::HallTab::Replays {
                app.ask_for_replays(None);
            }
        }
    }
    let actions = crate::hall::actions(app).len();
    let action = (0..actions).find(|at| clicked && crate::hall::action(*at).contains(mx, my));
    match (app.tab, action) {
        (crate::state::HallTab::Matches, Some(0))
        | (crate::state::HallTab::Tournaments, Some(0)) => {
            if !app.modes.is_empty() {
                app.mode_at = (app.mode_at + 1) % app.modes.len();
            }
        }
        (crate::state::HallTab::Matches, Some(1)) => {
            if app.searching() {
                app.cancel_search();
            } else {
                app.find_match();
            }
        }
        (crate::state::HallTab::Matches, Some(2)) => app.ask(crate::state::Asking::NewMatch),
        (crate::state::HallTab::Tournaments, Some(1)) => {
            app.cup_format = match app.cup_format {
                TournamentFormat::SingleElimination => TournamentFormat::RoundRobin,
                TournamentFormat::RoundRobin => TournamentFormat::SingleElimination,
            };
        }
        (crate::state::HallTab::Tournaments, Some(2)) => {
            app.cup_best_of = match app.cup_best_of {
                1 => 3,
                3 => 5,
                5 => 7,
                _ => 1,
            };
        }
        (crate::state::HallTab::Tournaments, Some(3)) => app.ask(crate::state::Asking::NewCup),
        (crate::state::HallTab::Replays, Some(0)) => app.ask_for_replays(None),
        (crate::state::HallTab::Replays, Some(1)) => {
            let next = app.more_replays;
            app.ask_for_replays(next);
        }
        _ => {}
    }
    if is_key_pressed(KeyCode::Enter)
        && app.tab == crate::state::HallTab::Matches
        && !app.searching()
    {
        app.find_match();
    }
    if !clicked {
        return;
    }
    let rows = crate::hall::rows_shown(screen_height());
    for at in 0..rows {
        let first = crate::hall::row_button(at, 0).contains(mx, my);
        let second = crate::hall::row_button(at, 1).contains(mx, my);
        if !first && !second {
            continue;
        }
        match app.tab {
            crate::state::HallTab::Matches => {
                let Some(found) = crate::hall::match_at(app, at) else {
                    continue;
                };
                let (id, stage) = (found.id, found.stage);
                let mine = app.holds_seat_in(found);
                let role = if (second && stage == Stage::Gathering) || (first && mine) {
                    Role::Player { seats: 1 }
                } else {
                    Role::Spectator
                };
                app.source.tell_hub(&ToHub::JoinMatch { id, role });
            }
            crate::state::HallTab::Tournaments => {
                let Some(found) = crate::hall::cup_at(app, at) else {
                    continue;
                };
                let (id, stage) = (found.id, found.stage);
                if second && stage == TournamentStage::Registration {
                    app.open_cup(id);
                    app.ask(crate::state::Asking::JoinTeam(id));
                } else {
                    app.open_cup(id);
                }
            }
            crate::state::HallTab::Replays => {
                let Some(found) = crate::hall::replay_at(app, at) else {
                    continue;
                };
                let match_id = found.id;
                app.source.tell_hub(&ToHub::FetchReplay { match_id });
            }
        }
        break;
    }
}

/// Leaves the match, or starts it for its host.
fn lobby_controls(app: &mut App) {
    if !is_mouse_button_pressed(MouseButton::Left) {
        return;
    }
    let (sw, sh) = (screen_width(), screen_height());
    let (mx, my) = mouse_position();
    if app.room.is_some() && crate::lobby::leave_button(sw, sh).contains(mx, my) {
        app.source.tell_hub(&ToHub::LeaveMatch);
        app.leave_room();
        return;
    }
    let gathering = app
        .room
        .as_ref()
        .is_some_and(|room| room.stage == Stage::Gathering);
    if gathering && app.hosts() && crate::lobby::action_button(sw, sh).contains(mx, my) {
        app.start_match();
    }
}

/// Selects a hero by card or by number, picks the one selected or one at
/// random, and leaves.
fn picking_controls(app: &mut App) {
    let picks = [
        KeyCode::Key1,
        KeyCode::Key2,
        KeyCode::Key3,
        KeyCode::Key4,
        KeyCode::Key5,
        KeyCode::Key6,
        KeyCode::Key7,
        KeyCode::Key8,
        KeyCode::Key9,
    ];
    let (sw, sh) = (screen_width(), screen_height());
    let (mx, my) = mouse_position();
    let clicked = is_mouse_button_pressed(MouseButton::Left);
    if clicked && crate::lobby::picking_leave_button(sw, sh).contains(mx, my) {
        app.source.tell_hub(&ToHub::LeaveMatch);
        app.leave_room();
        return;
    }
    if !app.may_pick() {
        return;
    }
    let heroes = crate::catalog::HEROES.len();
    let hero_at = |at: usize| bota_proto::HeroId(crate::catalog::HEROES[at].id);
    if clicked && crate::lobby::action_button(sw, sh).contains(mx, my) {
        app.pick_selected();
        return;
    }
    if clicked && crate::lobby::random_button(sw, sh).contains(mx, my) {
        app.pick_hero(hero_at(macroquad::rand::gen_range(0, heroes)));
        return;
    }
    let mut selected = picks.into_iter().take(heroes).position(is_key_pressed);
    if clicked && selected.is_none() {
        selected = crate::lobby::cards(app)
            .iter()
            .position(|card| card.contains(mx, my));
    }
    if let Some(at) = selected {
        app.select_hero(hero_at(at));
    }
}

fn camera_controls(app: &mut App) {
    let (_, wheel) = mouse_wheel();
    if wheel != 0.0 {
        // Over the shop the wheel runs the catalog; anywhere else it zooms.
        let (sw, sh) = (screen_width(), screen_height());
        let (mx, my) = mouse_position();
        let shop = crate::hud::shop_panel(sw, sh);
        if app.shop_open && shop.contains(mx, my) {
            let end = crate::hud::shop_scroll_end(&shop, crate::catalog::ITEMS.len());
            app.shop_scroll = if wheel > 0.0 {
                app.shop_scroll.saturating_sub(1)
            } else {
                (app.shop_scroll + 1).min(end)
            };
        } else {
            app.camera.zoom_by(wheel.signum());
        }
    }
    let dt = get_frame_time();
    let pan = 900.0 * dt;
    // The arrows always drive the camera. W, A, S and D are orders while our
    // hero stands; otherwise they pan, unless Control holds them for skill
    // points.
    let ctrl = is_key_down(KeyCode::LeftControl) || is_key_down(KeyCode::RightControl);
    let letters = !app.hero_stands() && !ctrl;
    let mut steer = (0.0, 0.0);
    if is_key_down(KeyCode::Up) || (letters && is_key_down(KeyCode::W)) {
        steer.1 -= pan;
    }
    if is_key_down(KeyCode::Down) || (letters && is_key_down(KeyCode::S)) {
        steer.1 += pan;
    }
    if is_key_down(KeyCode::Left) || (letters && is_key_down(KeyCode::A)) {
        steer.0 -= pan;
    }
    if is_key_down(KeyCode::Right) || (letters && is_key_down(KeyCode::D)) {
        steer.0 += pan;
    }
    let (edge_x, edge_y) = edge_push(pan);
    steer = (steer.0 + edge_x, steer.1 + edge_y);
    if steer != (0.0, 0.0) {
        // Driving the camera by hand lets go of whatever it was pinned to:
        // asking to look elsewhere is asking to stop being carried.
        app.pinned = None;
        app.camera.pan(steer.0, steer.1);
        return;
    }
    // The first hero to stand is looked at once, so a match does not open on
    // an empty middle of the map.
    if !app.found_my_hero
        && app.my_hero().is_some()
        && let Some(slot) = app.my_slot
    {
        app.found_my_hero = true;
        app.pinned = Some(crate::state::Pin::Hero(slot));
    }
    if let Some(pinned) = app.pinned_unit()
        && let Some(view) = app.view.as_ref()
        && let Some(unit) = view.units.iter().find(|u| u.id == pinned)
    {
        let (x, y) = (unit.pos.x.to_f32(), unit.pos.y.to_f32());
        app.camera.follow(x, y, dt);
    }
}

/// How hard the cursor at the edge of the screen pushes the camera.
///
/// Nothing at all while the cursor is outside the window, so a click on
/// another window does not drag the map with it.
fn edge_push(pan: f32) -> (f32, f32) {
    let (mx, my) = mouse_position();
    edge_of(mx, my, screen_width(), screen_height(), pan)
}

/// How hard a cursor at one spot on a screen of that size pushes the camera.
pub fn edge_of(mx: f32, my: f32, sw: f32, sh: f32, pan: f32) -> (f32, f32) {
    if mx < 0.0 || my < 0.0 || mx > sw || my > sh {
        return (0.0, 0.0);
    }
    let mut push = (0.0, 0.0);
    if mx <= EDGE_MARGIN {
        push.0 = -pan;
    } else if mx >= sw - EDGE_MARGIN {
        push.0 = pan;
    }
    if my <= EDGE_MARGIN {
        push.1 = -pan;
    } else if my >= sh - EDGE_MARGIN {
        push.1 = pan;
    }
    push
}

fn replay_controls(app: &mut App) {
    let Some(player) = app.player_mut() else {
        return;
    };
    if is_key_pressed(KeyCode::Space) {
        player.paused = !player.paused;
    }
    if is_key_pressed(KeyCode::Equal) || is_key_pressed(KeyCode::KpAdd) {
        player.speed = (player.speed * 2.0).min(16.0);
    }
    if is_key_pressed(KeyCode::Minus) || is_key_pressed(KeyCode::KpSubtract) {
        player.speed = (player.speed / 2.0).max(0.25);
    }
    if player.paused && is_key_pressed(KeyCode::Period) {
        player.advance_ticks(1.0);
    }
}

fn order_controls(app: &mut App) {
    if app.my_hero().is_none() || !app.controls_selection() {
        return;
    }
    // A fallen hero still spends skill points; every other order waits for
    // it to stand.
    if !app.hero_stands() {
        if is_key_down(KeyCode::LeftControl) || is_key_down(KeyCode::RightControl) {
            ability_keys(app);
        }
        return;
    }
    // A player's hero cannot pan by keys, so the letters are free for orders.
    if is_key_pressed(KeyCode::A) {
        app.attack_move_armed = true;
    }
    if is_key_pressed(KeyCode::S) {
        app.attack_move_armed = false;
        app.aiming = None;
        app.send_order(Order::Move {
            target: Target::None,
        });
    }
    if is_key_pressed(KeyCode::H) {
        app.attack_move_armed = false;
        app.send_order(Order::Attack {
            target: Target::None,
        });
    }
    ability_keys(app);
    item_keys(app);
    let (sx, sy) = mouse_position();
    let (wx, wy) = app
        .camera
        .screen_to_world(sx, sy, screen_width(), screen_height());
    let ground = world_vec(wx, wy);
    // A slot taken up spends the next left click, on the ground or on a unit.
    if let Some(slot) = app.aiming
        && is_mouse_button_pressed(MouseButton::Left)
    {
        app.aiming = None;
        let target = match app.aim_in(slot) {
            // A tree and a landing spot are both named by the ground under
            // the cursor; which one was meant is the server's to settle.
            Some(Aim::Point | Aim::Tree | Aim::Building) => Some(Target::Pos(ground)),
            // One's own hero is a target like any other here: a salve is
            // drunk by clicking the one drinking it.
            Some(Aim::Unit) => app
                .view
                .as_ref()
                .and_then(|view| unit_under_cursor(view, wx, wy, None, true))
                .map(Target::Unit),
            Some(Aim::Own) | None => Some(Target::None),
        };
        if let Some(target) = target
            && let Some(sent) = app.send_order(order_for(slot, target))
        {
            app.aimed_from = Some((sent, slot));
        }
        return;
    }
    if app.attack_move_armed && is_mouse_button_pressed(MouseButton::Left) {
        app.attack_move_armed = false;
        // A unit under the cursor takes the attack order itself: on an ally
        // this is the aggro-drop click. Near an enemy the order sticks to it.
        let me = app.my_hero();
        let target = app.view.as_ref().and_then(|view| {
            unit_under_cursor(view, wx, wy, me, true)
                .or_else(|| enemy_near_cursor(view, wx, wy, app.my_team()))
        });
        let aimed_at = match target {
            Some(target) => Target::Unit(target),
            None => Target::Pos(ground),
        };
        app.send_order(Order::Attack { target: aimed_at });
        return;
    }
    if is_mouse_button_pressed(MouseButton::Right) {
        app.attack_move_armed = false;
        app.aiming = None;
        let me = app.my_hero();
        let target = app
            .view
            .as_ref()
            .and_then(|view| unit_under_cursor(view, wx, wy, me, true));
        // A right click reads the ground the way Dota does: an enemy under
        // the cursor is attacked, one of this side's own is followed, an
        // item lying there is picked up, open ground is walked to. Denying
        // is the attack click's business.
        let lying = match target {
            Some(_) => None,
            None => app
                .view
                .as_ref()
                .and_then(|view| loot_under_cursor(view, wx, wy)),
        };
        let own = target.is_some_and(|id| {
            app.view
                .as_ref()
                .and_then(|view| view.units.iter().find(|unit| unit.id == id))
                .is_some_and(|unit| Some(unit.team) == app.my_team())
        });
        let order = match (target, lying) {
            (Some(target), _) if own => Order::Move {
                target: Target::Unit(target),
            },
            (Some(target), _) => Order::Attack {
                target: Target::Unit(target),
            },
            (None, Some(item)) => Order::Take {
                target: Target::Unit(item),
            },
            // Near an enemy the click still attacks it; only open ground is
            // walked to.
            (None, None) => {
                let near = app
                    .view
                    .as_ref()
                    .and_then(|view| enemy_near_cursor(view, wx, wy, app.my_team()));
                match near {
                    Some(mark) => Order::Attack {
                        target: Target::Unit(mark),
                    },
                    None => Order::Move {
                        target: Target::Pos(ground),
                    },
                }
            }
        };
        app.send_order(order);
    }
}

/// The ground item nearest the cursor, if the cursor is close enough.
pub fn loot_under_cursor(view: &WorldView, wx: f32, wy: f32) -> Option<EntityId> {
    let mut best: Option<(f32, EntityId)> = None;
    for lying in &view.loot {
        let dx = lying.pos.x.to_f32() - wx;
        let dy = lying.pos.y.to_f32() - wy;
        let dist = (dx * dx + dy * dy).sqrt();
        if dist <= 35.0 && best.is_none_or(|(b, _)| dist < b) {
            best = Some((dist, lying.id));
        }
    }
    best.map(|(_, id)| id)
}

/// The number keys use the six inventory slots.
fn item_keys(app: &mut App) {
    let keys = [
        KeyCode::Key1,
        KeyCode::Key2,
        KeyCode::Key3,
        KeyCode::Key4,
        KeyCode::Key5,
        KeyCode::Key6,
    ];
    for (i, key) in keys.into_iter().enumerate() {
        if is_key_pressed(key) {
            do_press(app, Slot::Item(i as u8), false);
        }
    }
}

/// Answers a press of one slot, however it was pressed.
///
/// The one door a key and a click both go through, so neither can grow a rule
/// the other does not have. What a press means is [`press`]; nothing is asked
/// here about whether the order would be carried out.
pub fn do_press(app: &mut App, slot: Slot, ctrl: bool) {
    match press(app, slot, ctrl) {
        Press::Send(order) => {
            app.aiming = None;
            if let Some(sent) = app.send_order(order) {
                app.aimed_from = Some((sent, slot));
            }
        }
        Press::Aim(slot) => {
            app.attack_move_armed = false;
            app.aiming = Some(slot);
        }
        Press::Nothing => {}
    }
}

/// Q, W, E, R, T and G cast the slots of whatever is selected; with Control
/// held they spend a skill point instead. One that is aimed arms and waits
/// for a click. Nothing is sent for a unit this seat does not drive.
fn ability_keys(app: &mut App) {
    let keys = [
        (KeyCode::Q, 0u8),
        (KeyCode::W, 1),
        (KeyCode::E, 2),
        (KeyCode::R, 3),
        (KeyCode::T, 4),
        (KeyCode::G, 5),
    ];
    // What is selected may be looked at whatever it is; the keys answer only
    // for what this seat drives.
    if !app.commanded().is_some_and(|unit| app.drives(unit)) {
        return;
    }
    let ctrl = is_key_down(KeyCode::LeftControl) || is_key_down(KeyCode::RightControl);
    for (key, slot) in keys {
        if is_key_pressed(key) {
            do_press(app, Slot::Ability(slot), ctrl);
        }
    }
}

/// The clickable unit nearest the cursor, if the cursor is close enough.
///
/// `exclude` names a unit that never matches — one's own hero when aiming an
/// order, so a click on it is a move like any other. `for_orders` also skips
/// what cannot be a target at all; selection wants those too.
pub fn unit_under_cursor(
    view: &WorldView,
    wx: f32,
    wy: f32,
    exclude: Option<EntityId>,
    for_orders: bool,
) -> Option<EntityId> {
    let mut best: Option<(f32, EntityId)> = None;
    for u in &view.units {
        if Some(u.id) == exclude || (for_orders && u.kind == UnitKind::Fountain) {
            continue;
        }
        let dx = u.pos.x.to_f32() - wx;
        let dy = u.pos.y.to_f32() - wy;
        let dist = (dx * dx + dy * dy).sqrt();
        let slack = u.collision.to_f32().max(20.0) + 15.0;
        if dist <= slack && best.is_none_or(|(b, _)| dist < b) {
            best = Some((dist, u.id));
        }
    }
    best.map(|(_, id)| id)
}

/// How far past a body's edge an attack click still sticks to it, in world
/// units.
const ATTACK_SNAP: f32 = 25.0;

/// The enemy nearest the cursor within the attack snap, if one is that near.
///
/// Only enemies stick. An order at one of this side's own — a follow, an
/// aggro drop or a deny — is asked for by the click that lands on the body
/// itself.
pub fn enemy_near_cursor(
    view: &WorldView,
    wx: f32,
    wy: f32,
    mine: Option<Team>,
) -> Option<EntityId> {
    let mut best: Option<(f32, EntityId)> = None;
    for u in &view.units {
        if u.kind == UnitKind::Fountain || Some(u.team) == mine {
            continue;
        }
        let dx = u.pos.x.to_f32() - wx;
        let dy = u.pos.y.to_f32() - wy;
        let dist = (dx * dx + dy * dy).sqrt();
        let reach = u.collision.to_f32() + ATTACK_SNAP;
        if dist <= reach && best.is_none_or(|(b, _)| dist < b) {
            best = Some((dist, u.id));
        }
    }
    best.map(|(_, id)| id)
}

/// A world position as the wire carries it.
pub fn world_vec(x: f32, y: f32) -> Vec2 {
    Vec2 {
        x: bota_proto::Fixed {
            raw: (x * 65536.0) as i32,
        },
        y: bota_proto::Fixed {
            raw: (y * 65536.0) as i32,
        },
    }
}
