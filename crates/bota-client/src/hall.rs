//! The sign-in and hall screens: where everything sits, and its drawing.
//!
//! Drawing and input read the same rectangles from here, so a button is
//! never in one place and pressed in another.

use bota_proto::{
    MatchId, MatchSummary, Search, Stage, Team, TournamentFormat, TournamentInfo, TournamentStage,
    TournamentSummary,
};
use macroquad::prelude::*;

use crate::hud::UiRect;
use crate::state::{App, HallTab};

/// Height of one row of a hall list.
pub const ROW_H: f32 = 30.0;
/// Left edge of everything on these screens.
pub const LEFT: f32 = 60.0;
/// Top edge of the first list row.
pub const LIST_TOP: f32 = 210.0;
/// Width of a list row.
pub const ROW_W: f32 = 780.0;
/// Width of a button in a row.
pub const ACT_W: f32 = 96.0;

/// How many rows of a list fit on the screen.
pub fn rows_shown(screen_h: f32) -> usize {
    (((screen_h - LIST_TOP - 60.0) / ROW_H).floor().max(1.0)) as usize
}

/// One row of a list, by its place on screen.
pub fn row(at: usize) -> UiRect {
    UiRect {
        x: LEFT,
        y: LIST_TOP + ROW_H * at as f32,
        w: ROW_W,
        h: ROW_H - 4.0,
    }
}

/// A button at the right end of a row: zero is the rightmost.
pub fn row_button(at: usize, from_right: usize) -> UiRect {
    let row = row(at);
    UiRect {
        x: row.x + row.w - ACT_W * (from_right + 1) as f32 - 6.0 * from_right as f32,
        y: row.y,
        w: ACT_W,
        h: row.h,
    }
}

/// One of the tabs along the top of the hall.
pub fn tab(at: usize) -> UiRect {
    UiRect {
        x: LEFT + (ACT_W + 40.0) * at as f32,
        y: 120.0,
        w: ACT_W + 30.0,
        h: 30.0,
    }
}

/// One of the buttons under the tabs.
pub fn action(at: usize) -> UiRect {
    UiRect {
        x: LEFT + (ACT_W + 46.0) * at as f32,
        y: 165.0,
        w: ACT_W + 36.0,
        h: 28.0,
    }
}

/// One of the boxes typed into on the sign-in screen.
pub fn entry(at: usize) -> UiRect {
    UiRect {
        x: LEFT,
        y: 190.0 + 54.0 * at as f32,
        w: 320.0,
        h: 34.0,
    }
}

/// One of the buttons under the sign-in boxes.
pub fn entry_button(at: usize) -> UiRect {
    UiRect {
        x: LEFT + (ACT_W + 46.0) * at as f32,
        y: 310.0,
        w: ACT_W + 36.0,
        h: 32.0,
    }
}

/// Where the teams of a tournament are listed.
pub const TEAMS_W: f32 = 300.0;

/// One team of the tournament screen.
pub fn team_row(at: usize) -> UiRect {
    UiRect {
        x: LEFT,
        y: LIST_TOP + ROW_H * at as f32,
        w: TEAMS_W,
        h: ROW_H - 4.0,
    }
}

/// One of the buttons along the bottom of the tournament screen.
pub fn cup_button(at: usize, screen_h: f32) -> UiRect {
    UiRect {
        x: LEFT + (ACT_W + 46.0) * at as f32,
        y: screen_h - 56.0,
        w: ACT_W + 36.0,
        h: 32.0,
    }
}

/// Where the chat is shown on the hall's screens and in the lobby: right of
/// the lists, the seats and the bracket.
pub fn chat_panel(screen_w: f32, screen_h: f32) -> UiRect {
    let x = 930.0;
    UiRect {
        x,
        y: 120.0,
        w: (screen_w - x - 30.0).max(240.0),
        h: (screen_h - 210.0).max(120.0),
    }
}

/// The box a name is typed into, over whatever it was asked from.
pub fn prompt_box(screen_w: f32, screen_h: f32) -> UiRect {
    UiRect {
        x: screen_w / 2.0 - 230.0,
        y: screen_h / 2.0 - 60.0,
        w: 460.0,
        h: 120.0,
    }
}

/// The three tabs, in the order they are drawn.
pub const TABS: [(HallTab, &str); 3] = [
    (HallTab::Matches, "Matches"),
    (HallTab::Tournaments, "Cups"),
    (HallTab::Replays, "Replays"),
];

/// Text that is read.
pub const INK: Color = Color::new(0.88, 0.90, 0.94, 1.0);
/// Text that is glanced at.
pub const FAINT: Color = Color::new(0.55, 0.60, 0.68, 1.0);
/// The fill of a box or a button.
pub const BOX: Color = Color::new(0.14, 0.16, 0.20, 1.0);
/// The fill of a button that is on.
pub const PICKED: Color = Color::new(0.22, 0.28, 0.38, 1.0);
/// The edge of a box or a button.
pub const EDGE: Color = Color::new(0.35, 0.40, 0.50, 1.0);
/// What darkens the screen behind a question.
pub const SHADE: Color = Color::new(0.0, 0.0, 0.0, 0.6);

/// What a button does, which decides its colours.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Tone {
    /// An everyday button.
    Plain,
    /// The way forward: ready, reconnect.
    Go,
    /// What cannot be taken back.
    Danger,
    /// A button that does nothing now.
    Off,
}

/// Whether the mouse is over a box.
pub fn hovered(rect: &UiRect) -> bool {
    let (mx, my) = mouse_position();
    rect.contains(mx, my)
}

/// Draws a box with a label in it, lit while the mouse is over it.
pub fn button(rect: &UiRect, label: &str, on: bool) {
    let fill = if on { PICKED } else { BOX };
    let fill = if hovered(rect) {
        lighter(fill, 0.06)
    } else {
        fill
    };
    draw_rectangle(rect.x, rect.y, rect.w, rect.h, fill);
    let edge = if hovered(rect) { INK } else { EDGE };
    draw_rectangle_lines(rect.x, rect.y, rect.w, rect.h, 1.0, edge);
    let size = 20.0;
    let width = measure_text(label, None, size as u16, 1.0).width;
    draw_text(
        label,
        rect.x + (rect.w - width) / 2.0,
        rect.y + rect.h / 2.0 + 7.0,
        size,
        INK,
    );
}

/// Draws a button coloured by what it does, its label sized to its height.
pub fn tone_button(rect: &UiRect, label: &str, tone: Tone) {
    let (fill, edge, ink) = match tone {
        Tone::Plain => (BOX, EDGE, INK),
        Tone::Go => (
            Color::new(0.16, 0.42, 0.24, 1.0),
            Color::new(0.38, 0.78, 0.46, 1.0),
            WHITE,
        ),
        Tone::Danger => (
            Color::new(0.38, 0.12, 0.11, 1.0),
            Color::new(0.80, 0.34, 0.30, 1.0),
            WHITE,
        ),
        Tone::Off => (Color::new(0.12, 0.13, 0.15, 1.0), BOX, FAINT),
    };
    let lit = tone != Tone::Off && hovered(rect);
    let fill = if lit { lighter(fill, 0.08) } else { fill };
    draw_rectangle(rect.x, rect.y, rect.w, rect.h, fill);
    draw_rectangle_lines(
        rect.x,
        rect.y,
        rect.w,
        rect.h,
        if lit { 2.0 } else { 1.0 },
        edge,
    );
    let size = (rect.h * 0.48).clamp(16.0, 26.0).round();
    let width = measure_text(label, None, size as u16, 1.0).width;
    draw_text(
        label,
        (rect.x + (rect.w - width) / 2.0).round(),
        (rect.y + rect.h / 2.0 + size * 0.34).round(),
        size,
        ink,
    );
}

/// A colour moved towards white.
pub fn lighter(color: Color, by: f32) -> Color {
    Color::new(
        (color.r + by).min(1.0),
        (color.g + by).min(1.0),
        (color.b + by).min(1.0),
        color.a,
    )
}

/// The panel at the bottom right of the hall's screens that offers back a
/// match being played.
pub fn rejoin_panel(screen_w: f32, screen_h: f32) -> UiRect {
    let chat = chat_panel(screen_w, screen_h);
    let top = chat.y + chat.h + 8.0;
    UiRect {
        x: chat.x,
        y: top,
        w: chat.w,
        h: (screen_h - top - 12.0).max(60.0),
    }
}

/// The button of that panel that goes back into the match.
pub fn rejoin_button(screen_w: f32, screen_h: f32) -> UiRect {
    let panel = rejoin_panel(screen_w, screen_h);
    UiRect {
        x: panel.x + 10.0,
        y: panel.y + panel.h - 40.0,
        w: (panel.w * 0.5).min(170.0),
        h: 32.0,
    }
}

/// The button of that panel that gives the match up for good.
pub fn abandon_button(screen_w: f32, screen_h: f32) -> UiRect {
    let panel = rejoin_panel(screen_w, screen_h);
    let w = (panel.w * 0.36).min(130.0);
    UiRect {
        x: panel.x + panel.w - 10.0 - w,
        y: panel.y + panel.h - 40.0,
        w,
        h: 32.0,
    }
}

/// The colour of the corner panel and of a match found.
const GO_EDGE: Color = Color::new(0.38, 0.78, 0.46, 1.0);

/// Draws the corner panel's box.
fn corner_box(panel: &UiRect) {
    draw_rectangle(
        panel.x,
        panel.y,
        panel.w,
        panel.h,
        Color::new(0.10, 0.16, 0.13, 0.96),
    );
    draw_rectangle_lines(panel.x, panel.y, panel.w, panel.h, 2.0, GO_EDGE);
}

/// Minutes and seconds, as a clock shows them.
pub fn clock(seconds: f32) -> String {
    let whole = seconds.max(0.0) as u32;
    format!("{}:{:02}", whole / 60, whole % 60)
}

/// Draws the panel at the bottom right of the hall's screens: the match
/// this account holds a seat in and is not in, offered back, or else the
/// search going on.
pub fn draw_corner(app: &App) {
    let (sw, sh) = (screen_width(), screen_height());
    let panel = rejoin_panel(sw, sh);
    if let Some(m) = app.match_to_rejoin() {
        corner_box(&panel);
        draw_text(
            format!("Match in progress: {}", m.name),
            panel.x + 10.0,
            panel.y + 20.0,
            19.0,
            INK,
        );
        tone_button(&rejoin_button(sw, sh), "Reconnect", Tone::Go);
        if matches!(m.stage, Stage::Picking | Stage::Playing) {
            tone_button(&abandon_button(sw, sh), "Abandon", Tone::Danger);
        }
        return;
    }
    if let Search::Queued { mode, waiting } = &app.search {
        corner_box(&panel);
        let name = app
            .modes
            .iter()
            .find(|m| m.id == *mode)
            .map_or("a match", |m| m.name.as_str());
        draw_text(
            format!("Searching: {name}"),
            panel.x + 10.0,
            panel.y + 20.0,
            19.0,
            INK,
        );
        draw_text(
            clock(app.searched_for),
            panel.x + 10.0,
            panel.y + 52.0,
            28.0,
            GO_EDGE,
        );
        draw_text(
            format!("{waiting} waiting"),
            panel.x + 90.0,
            panel.y + 50.0,
            18.0,
            FAINT,
        );
        tone_button(&abandon_button(sw, sh), "Cancel", Tone::Plain);
    }
}

/// The box a match found in the queue is offered in.
pub fn found_box(screen_w: f32, screen_h: f32) -> UiRect {
    UiRect {
        x: screen_w / 2.0 - 230.0,
        y: screen_h / 2.0 - 110.0,
        w: 460.0,
        h: 220.0,
    }
}

/// The button of that box that accepts the match.
pub fn found_accept(screen_w: f32, screen_h: f32) -> UiRect {
    let rect = found_box(screen_w, screen_h);
    UiRect {
        x: rect.x + rect.w - 20.0 - 180.0,
        y: rect.y + rect.h - 64.0,
        w: 180.0,
        h: 46.0,
    }
}

/// The button of that box that turns the match down.
pub fn found_decline(screen_w: f32, screen_h: f32) -> UiRect {
    let rect = found_box(screen_w, screen_h);
    UiRect {
        x: rect.x + 20.0,
        y: rect.y + rect.h - 64.0,
        w: 140.0,
        h: 46.0,
    }
}

/// Draws the match the queue found, over whatever screen is up: how long is
/// left to accept it, who has, and the buttons.
pub fn draw_found(app: &App) {
    let Search::Found {
        mode,
        accepted,
        players,
        yours,
        ms_left,
        ..
    } = &app.search
    else {
        return;
    };
    let (sw, sh) = (screen_width(), screen_height());
    let rect = found_box(sw, sh);
    draw_rectangle(0.0, 0.0, sw, sh, SHADE);
    draw_rectangle(rect.x, rect.y, rect.w, rect.h, BOX);
    draw_rectangle_lines(rect.x, rect.y, rect.w, rect.h, 2.0, GO_EDGE);
    draw_text("Match found", rect.x + 20.0, rect.y + 42.0, 34.0, INK);
    let name = app
        .modes
        .iter()
        .find(|m| m.id == *mode)
        .map_or("", |m| m.name.as_str());
    draw_text(name, rect.x + 20.0, rect.y + 70.0, 20.0, FAINT);
    let left = App::seconds_left(*ms_left, app.search_age);
    let share = if app.found_span > 0.0 {
        (left / app.found_span).clamp(0.0, 1.0)
    } else {
        0.0
    };
    let bar = UiRect {
        x: rect.x + 20.0,
        y: rect.y + 88.0,
        w: rect.w - 40.0,
        h: 8.0,
    };
    draw_rectangle(
        bar.x,
        bar.y,
        bar.w,
        bar.h,
        Color::new(0.08, 0.09, 0.11, 1.0),
    );
    draw_rectangle(bar.x, bar.y, bar.w * share, bar.h, GO_EDGE);
    draw_text(
        format!(
            "{accepted} of {players} accepted    {} left",
            clock(left.ceil())
        ),
        rect.x + 20.0,
        rect.y + 124.0,
        20.0,
        INK,
    );
    if *yours {
        draw_text(
            "accepted; waiting for the others",
            rect.x + 20.0,
            rect.y + rect.h - 34.0,
            20.0,
            FAINT,
        );
    } else {
        tone_button(&found_decline(sw, sh), "Decline", Tone::Plain);
        tone_button(&found_accept(sw, sh), "Accept", Tone::Go);
    }
}

/// The box a yes-or-no question is asked in.
pub fn confirm_box(screen_w: f32, screen_h: f32) -> UiRect {
    UiRect {
        x: screen_w / 2.0 - 250.0,
        y: screen_h / 2.0 - 90.0,
        w: 500.0,
        h: 180.0,
    }
}

/// The button of that box that says yes.
pub fn confirm_yes(screen_w: f32, screen_h: f32) -> UiRect {
    let rect = confirm_box(screen_w, screen_h);
    UiRect {
        x: rect.x + rect.w - 20.0 - 150.0,
        y: rect.y + rect.h - 56.0,
        w: 150.0,
        h: 38.0,
    }
}

/// The button of that box that says no.
pub fn confirm_no(screen_w: f32, screen_h: f32) -> UiRect {
    let yes = confirm_yes(screen_w, screen_h);
    UiRect {
        x: yes.x - 16.0 - 150.0,
        ..yes
    }
}

/// Draws the question waiting for a yes or a no, over whatever screen asked
/// it.
pub fn draw_confirm(app: &App) {
    let Some(confirm) = &app.confirm else {
        return;
    };
    let (sw, sh) = (screen_width(), screen_height());
    let rect = confirm_box(sw, sh);
    draw_rectangle(0.0, 0.0, sw, sh, SHADE);
    draw_rectangle(rect.x, rect.y, rect.w, rect.h, BOX);
    draw_rectangle_lines(rect.x, rect.y, rect.w, rect.h, 2.0, EDGE);
    let (title, lines, yes) = confirm.words();
    draw_text(title, rect.x + 20.0, rect.y + 40.0, 28.0, INK);
    for (at, line) in lines.iter().enumerate() {
        draw_text(
            line,
            rect.x + 20.0,
            rect.y + 72.0 + 24.0 * at as f32,
            20.0,
            FAINT,
        );
    }
    tone_button(&confirm_no(sw, sh), "Stay", Tone::Plain);
    tone_button(&confirm_yes(sw, sh), yes, Tone::Danger);
}

/// The sign-in screen: a login, a password, and what to do with them.
pub fn draw_signin(app: &App) {
    draw_text("bota", LEFT, 90.0, 48.0, INK);
    let hint = if app.greeted {
        "signing in..."
    } else {
        "sign in, or register a new account   [Tab] switches boxes   [Enter] signs in"
    };
    draw_text(hint, LEFT, 140.0, 22.0, FAINT);
    for (at, (label, text, hidden)) in [
        ("login", app.login.clone(), false),
        ("password", app.password.clone(), true),
    ]
    .into_iter()
    .enumerate()
    {
        let rect = entry(at);
        let picked = at == usize::from(app.field == crate::state::Field::Password);
        draw_text(label, rect.x, rect.y - 8.0, 18.0, FAINT);
        draw_rectangle(rect.x, rect.y, rect.w, rect.h, BOX);
        draw_rectangle_lines(
            rect.x,
            rect.y,
            rect.w,
            rect.h,
            if picked { 2.0 } else { 1.0 },
            if picked { INK } else { EDGE },
        );
        let shown = if hidden {
            "*".repeat(text.chars().count())
        } else {
            text
        };
        let caret = if picked { "_" } else { "" };
        draw_text(
            format!("{shown}{caret}"),
            rect.x + 8.0,
            rect.y + 24.0,
            22.0,
            INK,
        );
    }
    button(&entry_button(0), "Sign in", false);
    button(&entry_button(1), "Register", false);
    button(&entry_button(2), "Watch as guest", false);
}

/// The hall: matches, tournaments or recordings, with what can be done to
/// each.
pub fn draw_hall(app: &App) {
    draw_text("bota", LEFT, 80.0, 44.0, INK);
    let who = match &app.account {
        Some(account) => format!("signed in as {}", account.login),
        None => format!("watching as {}", app.login),
    };
    draw_text(&who, LEFT + 180.0, 80.0, 22.0, FAINT);
    for (at, (which, label)) in TABS.into_iter().enumerate() {
        button(&tab(at), label, app.tab == which);
    }
    for (at, label) in actions(app).into_iter().enumerate() {
        button(&action(at), &label, false);
    }
    match app.tab {
        HallTab::Matches => draw_matches(app),
        HallTab::Tournaments => draw_tournaments(app),
        HallTab::Replays => draw_replays(app),
    }
    crate::chat::draw_box(
        app,
        &chat_panel(screen_width(), screen_height()),
        "[T] to talk   [/] command",
    );
    if let Some(fetching) = &app.fetching {
        draw_text(
            format!("fetching replay {}: {} bytes", fetching.id.0, fetching.got),
            LEFT,
            screen_height() - 70.0,
            20.0,
            FAINT,
        );
    }
    draw_corner(app);
}

/// The buttons under the tabs, in the order they are drawn.
pub fn actions(app: &App) -> Vec<String> {
    let mode = app
        .modes
        .get(app.mode_at)
        .map_or("no modes".to_string(), |m| m.name.clone());
    match app.tab {
        HallTab::Matches => vec![
            format!("Mode: {mode}"),
            if app.searching() {
                "Cancel search".to_string()
            } else {
                "Find match".to_string()
            },
            "New match".to_string(),
        ],
        HallTab::Tournaments => vec![
            format!("Mode: {mode}"),
            format!("{}", format_name(app.cup_format)),
            format!("Best of {}", app.cup_best_of),
            "New cup".to_string(),
        ],
        HallTab::Replays => vec!["Refresh".to_string(), "Older".to_string()],
    }
}

fn draw_matches(app: &App) {
    let rows = rows_shown(screen_height());
    if app.hall_matches.is_empty() {
        draw_text("no matches yet", LEFT, LIST_TOP + 22.0, 22.0, FAINT);
    }
    for (at, m) in app
        .hall_matches
        .iter()
        .skip(app.hall_scroll)
        .take(rows)
        .enumerate()
    {
        let rect = row(at);
        draw_rectangle(rect.x, rect.y, rect.w, rect.h, BOX);
        let held = m.seats.iter().filter(|s| !s.login.is_empty()).count();
        let line = format!(
            "{}  {}  {}  {held}/{} seats",
            m.id.0,
            m.name,
            stage_name(m.stage),
            m.seats.len()
        );
        draw_text(&line, rect.x + 8.0, rect.y + 21.0, 21.0, INK);
        if m.stage == Stage::Gathering && held < m.seats.len() {
            button(&row_button(at, 1), "Join", false);
        }
        if app.holds_seat_in(m) {
            tone_button(&row_button(at, 0), "Rejoin", Tone::Go);
        } else if matches!(m.stage, Stage::Gathering | Stage::Picking | Stage::Playing) {
            button(&row_button(at, 0), "Watch", false);
        } else if let Some(winner) = m.winner {
            draw_text(
                format!("{winner:?} won"),
                rect.x + rect.w - 150.0,
                rect.y + 21.0,
                20.0,
                team_tint(winner),
            );
        }
    }
    draw_text(
        "[Tab] next list   [Enter] find match   [Esc] quit",
        LEFT,
        screen_height() - 40.0,
        20.0,
        FAINT,
    );
}

fn draw_tournaments(app: &App) {
    let rows = rows_shown(screen_height());
    if app.hall_tournaments.is_empty() {
        draw_text("no cups yet", LEFT, LIST_TOP + 22.0, 22.0, FAINT);
    }
    for (at, t) in app
        .hall_tournaments
        .iter()
        .skip(app.hall_scroll)
        .take(rows)
        .enumerate()
    {
        let rect = row(at);
        draw_rectangle(rect.x, rect.y, rect.w, rect.h, BOX);
        let mode = app
            .modes
            .iter()
            .find(|m| m.id == t.mode)
            .map_or(String::new(), |m| format!("{}  ", m.name));
        let line = format!(
            "{}  {}  {mode}{}, bo{}  {}  {} teams of {}",
            t.id.0,
            t.name,
            format_name(t.format),
            t.best_of,
            cup_stage_name(t.stage),
            t.teams,
            t.team_size
        );
        draw_text(&line, rect.x + 8.0, rect.y + 21.0, 21.0, INK);
        if t.stage == TournamentStage::Registration {
            button(&row_button(at, 1), "Enter", false);
        }
        button(&row_button(at, 0), "Open", false);
    }
}

/// What a format is called in the hall.
pub fn format_name(format: TournamentFormat) -> &'static str {
    match format {
        TournamentFormat::SingleElimination => "single elimination",
        TournamentFormat::RoundRobin => "round robin",
    }
}

/// One drawn line of a bracket, and the game it stands for.
pub struct BracketLine {
    /// Where it sits.
    pub rect: UiRect,
    /// What it says.
    pub text: String,
    /// The match it names, for a line that stands for one game.
    pub game: Option<MatchId>,
    /// What the button at its end offers, if anything.
    pub action: Option<&'static str>,
    /// Whether it is a round heading rather than a pairing.
    pub heading: bool,
    /// Whether the pairing is settled.
    pub settled: bool,
}

/// The bracket as lines to draw and press, in reading order.
///
/// `matches` is the hall listing, which says whether a game can be watched
/// or only fetched.
pub fn bracket_lines(cup: &TournamentInfo, matches: &[MatchSummary]) -> Vec<BracketLine> {
    let left = LEFT + TEAMS_W + 30.0;
    let mut lines = Vec::new();
    let mut y = LIST_TOP;
    let name = |team: Option<u8>| {
        team.and_then(|i| cup.teams.get(usize::from(i)))
            .map_or("-".to_string(), |t| t.name.clone())
    };
    for (r, round) in cup.rounds.iter().enumerate() {
        lines.push(BracketLine {
            rect: UiRect {
                x: left,
                y,
                w: 420.0,
                h: 22.0,
            },
            text: format!("round {}", r + 1),
            game: None,
            action: None,
            heading: true,
            settled: false,
        });
        y += 26.0;
        for pair in &round.pairs {
            let won = |team: Option<u8>| pair.games.iter().filter(|g| g.winner == team).count();
            lines.push(BracketLine {
                rect: UiRect {
                    x: left + 12.0,
                    y,
                    w: 420.0,
                    h: 24.0,
                },
                text: format!(
                    "{}  {} - {}  {}",
                    name(pair.a),
                    won(pair.a),
                    won(pair.b),
                    name(pair.b)
                ),
                game: None,
                action: None,
                heading: false,
                settled: pair.winner.is_some(),
            });
            y += 26.0;
            for (g, game) in pair.games.iter().enumerate() {
                let stage = game
                    .match_id
                    .and_then(|id| matches.iter().find(|m| m.id == id))
                    .map(|m| m.stage);
                let (said, action) = match (game.match_id, stage) {
                    (None, _) => ("walkover".to_string(), None),
                    (Some(_), Some(Stage::Gathering)) => ("gathering".to_string(), Some("Watch")),
                    (Some(_), Some(Stage::Picking)) => {
                        ("picking heroes".to_string(), Some("Watch"))
                    }
                    (Some(_), Some(Stage::Playing)) => ("being played".to_string(), Some("Watch")),
                    (Some(_), _) => match game.winner {
                        Some(team) => (format!("{} won", name(Some(team))), Some("Replay")),
                        None => ("unfinished".to_string(), Some("Replay")),
                    },
                };
                lines.push(BracketLine {
                    rect: UiRect {
                        x: left + 28.0,
                        y,
                        w: 400.0,
                        h: 22.0,
                    },
                    text: format!("game {}: {said}", g + 1),
                    game: game.match_id,
                    action,
                    heading: false,
                    settled: game.winner.is_some(),
                });
                y += 24.0;
            }
        }
        y += 6.0;
    }
    lines
}

/// The tournament screen: who is in it, how it is set up, and its bracket.
pub fn draw_cup(app: &App) {
    let Some(cup) = &app.tournament else {
        draw_text("opening the cup...", LEFT, 90.0, 32.0, FAINT);
        button(&cup_button(0, screen_height()), "Back", false);
        return;
    };
    draw_text(&cup.name, LEFT, 80.0, 40.0, INK);
    let mode = app
        .modes
        .iter()
        .find(|m| m.id == cup.mode)
        .map_or(String::new(), |m| m.name.clone());
    draw_text(
        format!(
            "{mode}   {}   best of {}   teams of {}   {}   by {}",
            format_name(cup.format),
            cup.best_of,
            cup.team_size,
            cup_stage_name(cup.stage),
            cup.organizer
        ),
        LEFT,
        112.0,
        21.0,
        FAINT,
    );
    let mine = app.account.as_ref().and_then(|me| {
        cup.teams
            .iter()
            .position(|t| t.members.iter().any(|m| m.login == me.login))
    });
    let hint = match (cup.stage, mine) {
        (TournamentStage::Registration, None) => {
            "press Enter to join a team; a new name makes one".to_string()
        }
        (TournamentStage::Registration, Some(at)) => format!("you are in {}", cup.teams[at].name),
        (_, _) => match cup.winner.and_then(|i| cup.teams.get(usize::from(i))) {
            Some(winner) => format!("{} wins the cup", winner.name),
            None => "games are being played; you are taken to yours".to_string(),
        },
    };
    let tint = if cup.winner.is_some() { GOLD } else { FAINT };
    draw_text(&hint, LEFT, 140.0, 21.0, tint);
    draw_text("teams", LEFT, LIST_TOP - 12.0, 20.0, FAINT);
    for (at, team) in cup.teams.iter().enumerate() {
        let rect = team_row(at);
        draw_rectangle(rect.x, rect.y, rect.w, rect.h, BOX);
        let seats: usize = team.members.iter().map(|m| usize::from(m.seats)).sum();
        let full = seats == usize::from(cup.team_size);
        let tint = if mine == Some(at) {
            GOLD
        } else if full {
            INK
        } else {
            FAINT
        };
        let members = team
            .members
            .iter()
            .map(|m| match m.seats {
                1 => m.login.clone(),
                n => format!("{} x{n}", m.login),
            })
            .collect::<Vec<_>>()
            .join(", ");
        draw_text(
            format!("{} {seats}/{}  {members}", team.name, cup.team_size),
            rect.x + 8.0,
            rect.y + 20.0,
            20.0,
            tint,
        );
    }
    if cup.teams.is_empty() {
        draw_text("nobody yet", LEFT + 8.0, LIST_TOP + 20.0, 20.0, FAINT);
    }
    for line in bracket_lines(cup, &app.hall_matches) {
        let tint = if line.heading {
            FAINT
        } else if line.settled {
            INK
        } else {
            Color::new(0.72, 0.76, 0.82, 1.0)
        };
        draw_text(&line.text, line.rect.x, line.rect.y + 17.0, 20.0, tint);
        if let Some(action) = line.action {
            button(
                &UiRect {
                    x: line.rect.x + line.rect.w,
                    y: line.rect.y,
                    w: ACT_W,
                    h: line.rect.h,
                },
                action,
                false,
            );
        }
    }
    for (at, label) in cup_actions(app).into_iter().enumerate() {
        button(&cup_button(at, screen_height()), label, false);
    }
    crate::chat::draw_box(
        app,
        &chat_panel(screen_width(), screen_height()),
        "[T] to talk   [/] command",
    );
    draw_corner(app);
}

/// The buttons along the bottom of the tournament screen, in order.
pub fn cup_actions(app: &App) -> Vec<&'static str> {
    let Some(cup) = &app.tournament else {
        return vec!["Back"];
    };
    let mine = app.account.as_ref().is_some_and(|me| {
        cup.teams
            .iter()
            .any(|t| t.members.iter().any(|m| m.login == me.login))
    });
    let organiser = app
        .account
        .as_ref()
        .is_some_and(|me| me.login == cup.organizer);
    let mut actions = vec!["Back"];
    if cup.stage == TournamentStage::Registration {
        actions.push(if mine { "Leave" } else { "Join team" });
        if organiser {
            actions.push("Start");
            actions.push("Cancel");
        }
    }
    actions
}

/// The box a name is typed into, over whatever screen asked for it.
pub fn draw_prompt(app: &App) {
    let Some(asking) = &app.asking else {
        return;
    };
    let rect = prompt_box(screen_width(), screen_height());
    draw_rectangle(0.0, 0.0, screen_width(), screen_height(), SHADE);
    draw_rectangle(rect.x, rect.y, rect.w, rect.h, BOX);
    draw_rectangle_lines(rect.x, rect.y, rect.w, rect.h, 2.0, EDGE);
    draw_text(asking.prompt(), rect.x + 16.0, rect.y + 34.0, 22.0, FAINT);
    draw_text(
        format!("{}_", app.entry),
        rect.x + 16.0,
        rect.y + 74.0,
        26.0,
        INK,
    );
    draw_text(
        "[Enter] confirm   [Esc] cancel",
        rect.x + 16.0,
        rect.y + 104.0,
        18.0,
        FAINT,
    );
}

fn draw_replays(app: &App) {
    let rows = rows_shown(screen_height());
    if app.replays.is_empty() {
        draw_text(
            "no recordings; press Refresh",
            LEFT,
            LIST_TOP + 22.0,
            22.0,
            FAINT,
        );
    }
    for (at, m) in app.replays.iter().take(rows).enumerate() {
        let rect = row(at);
        draw_rectangle(rect.x, rect.y, rect.w, rect.h, BOX);
        let seats = m
            .seats
            .iter()
            .map(|s| s.login.as_str())
            .filter(|login| !login.is_empty())
            .collect::<Vec<_>>()
            .join(" vs ");
        let ticks = m.duration / 30;
        let line = format!(
            "{}  {}  {seats}  {}:{:02}",
            m.id.0,
            m.name,
            ticks / 60,
            ticks % 60
        );
        draw_text(&line, rect.x + 8.0, rect.y + 21.0, 21.0, INK);
        if let Some(winner) = m.winner {
            draw_text(
                format!("{winner:?} won"),
                rect.x + rect.w - 250.0,
                rect.y + 21.0,
                20.0,
                team_tint(winner),
            );
        }
        button(&row_button(at, 0), "Watch", false);
    }
}

fn stage_name(stage: Stage) -> &'static str {
    match stage {
        Stage::Gathering => "gathering",
        Stage::Picking => "picking",
        Stage::Playing => "playing",
        Stage::Over => "over",
        Stage::Abandoned => "abandoned",
    }
}

fn cup_stage_name(stage: TournamentStage) -> &'static str {
    match stage {
        TournamentStage::Registration => "open",
        TournamentStage::Running => "running",
        TournamentStage::Finished => "finished",
    }
}

fn team_tint(team: Team) -> Color {
    match team {
        Team::Radiant => Color::new(0.30, 0.80, 0.35, 1.0),
        Team::Dire => Color::new(0.90, 0.30, 0.25, 1.0),
        Team::Neutral => FAINT,
    }
}

/// The match a row of the matches list names.
pub fn match_at(app: &App, at: usize) -> Option<&MatchSummary> {
    app.hall_matches.get(app.hall_scroll + at)
}

/// The tournament a row of the cups list names.
pub fn cup_at(app: &App, at: usize) -> Option<&TournamentSummary> {
    app.hall_tournaments.get(app.hall_scroll + at)
}

/// The recording a row of the replays list names.
pub fn replay_at(app: &App, at: usize) -> Option<&MatchSummary> {
    app.replays.get(at)
}
