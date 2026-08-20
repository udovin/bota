//! Noticing that the body is not getting anywhere, and working it loose.
//!
//! The server lays a route round what is built and slides a hero along what it
//! grazes, so most of the way there needs no help. What it does not answer is a
//! body pressed into a corner by other bodies: the order stands, the route is
//! fine, and the hero stays where it is. That is what is watched for here.

use bota_proto::{StatusFlags, UnitView, Vec2};

use crate::{Params, along, aside, span};

/// What the bot remembers about getting where it meant to go.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Footing {
    /// Where the body stood when it was last looked at.
    was: Option<Vec2>,
    /// Ticks it has meant to move and not moved.
    stuck: u32,
    /// The spot it is walking at to work itself loose, and the ticks left of
    /// trying.
    detour: Option<(Vec2, u32)>,
    /// Which side the next way round goes.
    side: bool,
    /// Ways round tried since it last got anywhere.
    tries: u8,
}

impl Footing {
    /// Nothing remembered yet.
    pub fn new() -> Footing {
        Footing::default()
    }

    /// Forgets everything: a body that has just stood up is not the body that
    /// was stuck.
    pub fn forget(&mut self) {
        *self = Footing::new();
    }

    /// Notes where the body is, and whether the ground it covered is the
    /// ground it meant to cover.
    ///
    /// Held still by something the bot cannot help — a stun, a root — is not
    /// being stuck, and neither is standing still on purpose.
    pub fn watch(&mut self, body: &UnitView, meant_to_move: bool, params: &Params) {
        let held = body.statuses.bits & (StatusFlags::STUNNED | StatusFlags::ROOTED) != 0;
        let moved = self
            .was
            .is_none_or(|was| span(was, body.pos) > params.wedge_step);
        self.was = Some(body.pos);
        if let Some((at, left)) = self.detour {
            self.detour = (left > 1).then_some((at, left - 1));
        }
        if !meant_to_move || held || moved {
            self.stuck = 0;
            if moved && self.detour.is_none() {
                self.tries = 0;
            }
            return;
        }
        self.stuck = self.stuck.saturating_add(1);
    }

    /// Whether the body has stood still long enough to need help.
    pub fn is_wedged(&self, params: &Params) -> bool {
        self.stuck as f32 >= params.wedge_ticks
    }

    /// The way round it is already walking, while it still is.
    pub fn detour(&self) -> Option<Vec2> {
        self.detour.map(|(at, _)| at)
    }

    /// Picks a way round something in the way and keeps to it for a while.
    ///
    /// The first try steps to one side of the line to the destination, the
    /// next to the other, and once neither has worked it backs off the way it
    /// came.
    pub fn work_loose(&mut self, from: Vec2, towards: Vec2, params: &Params) -> Vec2 {
        let reach = params.detour_reach;
        let at = if self.tries >= 2 {
            along(from, towards, -reach)
        } else if self.side {
            aside(from, towards, reach)
        } else {
            aside(from, towards, -reach)
        };
        self.side = !self.side;
        self.tries = self.tries.saturating_add(1) % 4;
        self.stuck = 0;
        self.detour = Some((at, params.detour_ticks.max(1.0) as u32));
        at
    }
}
