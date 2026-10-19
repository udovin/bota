//! Sample values for the tests.
//!
//! Every struct here is built by listing all of its fields. Adding a field to a
//! wire type breaks compilation right here, which is the point: it forces a
//! decision about what the new field carries before it can ship.

use crate::*;

pub fn entity(idx: u32) -> EntityId {
    EntityId { idx, generation: 1 }
}

pub fn fixed(units: i32) -> Fixed {
    Fixed { raw: units << 16 }
}

pub fn ability_view(slot: u16) -> AbilityView {
    AbilityView {
        id: AbilityId(slot),
        level: 2,
        max_level: 4,
        cooldown_left: 40,
        mana_cost: 90,
        range: 600,
        aim: Aim::Unit,
        passive: false,
        on: false,
        can_level: true,
    }
}

pub fn item_view(slot: u16) -> ItemView {
    ItemView {
        id: ItemId(slot),
        charges: Some(3),
        cooldown_left: 0,
        mute_left: 15,
        mode: None,
        mana_cost: 25,
        range: 400,
        aim: Some(Aim::Point),
        for_sale: false,
    }
}

pub fn loot_view() -> LootView {
    LootView {
        id: entity(21),
        pos: Vec2 {
            x: fixed(6000),
            y: fixed(6200),
        },
        item: ItemId(3),
        charges: Some(2),
    }
}

pub fn hero_unit() -> UnitView {
    UnitView {
        id: entity(7),
        kind: UnitKind::Hero,
        team: Team::Radiant,
        pos: Vec2 {
            x: fixed(4096),
            y: fixed(3200),
        },
        facing: Angle { brads: 12345 },
        hp: 640,
        max_hp: 700,
        mana: 280,
        max_mana: 350,
        move_speed: fixed(300),
        attack_damage: 54,
        attack_range: fixed(550),
        attack_time: 1700,
        attack_point: 500,
        attack_speed: 100,
        attributes: Attributes::ZERO,
        primary: None,
        armor: fixed(3),
        magic_resist: Fixed { raw: 16384 },
        collision: fixed(27),
        bound: fixed(24),
        vision_radius: fixed(1800),
        true_sight_radius: fixed(0),
        statuses: StatusFlags {
            bits: StatusFlags::SLOWED | StatusFlags::DOT,
        },
        hero: Some(HeroId(0)),
        owner: Some(SlotId(0)),
        level: 6,
        abilities: (0..4).map(ability_view).collect(),
        items: (0..9)
            .map(|i| if i < 3 { Some(item_view(i)) } else { None })
            .collect(),
        effects: vec![
            EffectView {
                id: EffectId(0),
                ticks_left: Some(120),
                stacks: None,
            },
            EffectView {
                id: EffectId(11),
                ticks_left: None,
                stacks: Some(21),
            },
        ],
    }
}

pub fn creep_unit(idx: u32) -> UnitView {
    UnitView {
        id: entity(idx),
        kind: UnitKind::CreepMelee,
        team: Team::Dire,
        pos: Vec2 {
            x: fixed(2048),
            y: fixed(2048),
        },
        facing: Angle { brads: 0 },
        hp: 480,
        max_hp: 550,
        mana: 0,
        max_mana: 0,
        move_speed: fixed(325),
        attack_damage: 21,
        attack_range: fixed(100),
        attack_time: 1000,
        attack_point: 466,
        attack_speed: 100,
        attributes: Attributes::ZERO,
        primary: None,
        armor: fixed(2),
        magic_resist: Fixed { raw: 0 },
        collision: fixed(36),
        bound: fixed(16),
        vision_radius: fixed(1100),
        true_sight_radius: fixed(0),
        statuses: StatusFlags::default(),
        hero: None,
        owner: None,
        level: 0,
        abilities: Vec::new(),
        items: Vec::new(),
        effects: Vec::new(),
    }
}

pub fn projectile() -> ProjectileView {
    ProjectileView {
        id: entity(99),
        pos: Vec2 {
            x: fixed(3000),
            y: fixed(3000),
        },
        facing: Angle { brads: 32768 },
        team: Team::Radiant,
        ability: Some(AbilityId(2)),
    }
}

pub fn player_view(slot: u8) -> PlayerView {
    PlayerView {
        slot: SlotId(slot),
        team: Team::Radiant,
        hero: HeroId(0),
        unit: Some(entity(7)),
        level: 6,
        xp: 2400,
        gold: Some(1200),
        stash: Some(
            (0..6)
                .map(|i| if i == 0 { Some(item_view(9)) } else { None })
                .collect(),
        ),
        kit: Some(Kit {
            abilities: vec![ability_view(0)],
            items: vec![Some(item_view(3)), None],
        }),
        kills: 3,
        deaths: 1,
        assists: 0,
        last_hits: 42,
        denies: 7,
        respawn_left: 0,
    }
}

/// A view holding one hero, `creeps` creeps, a projectile and two seats.
pub fn world_view(creeps: u32) -> WorldView {
    WorldView {
        tick: 5400,
        viewer: Some(Team::Radiant),
        units: std::iter::once(hero_unit())
            .chain((0..creeps).map(|i| creep_unit(100 + i)))
            .collect(),
        projectiles: vec![projectile()],
        players: (0..2).map(player_view).collect(),
        felled_trees: Vec::new(),
        planted_trees: Vec::new(),
        loot: vec![loot_view()],
    }
}

pub fn match_info() -> MatchInfo {
    MatchInfo {
        match_id: MatchId(0xdead_beef),
        map: MapId(1),
        tick_rate: 30,
        pregame_ticks: 900,
        trees: vec![
            Vec2 {
                x: fixed(5000),
                y: fixed(4000),
            },
            Vec2 {
                x: fixed(11384),
                y: fixed(12384),
            },
        ],
        terrain_cells: 4,
        terrain_rle: vec![(12, 0x81), (4, 0xc0)],
        opaque_cells: vec![(1, 1), (2, 1)],
        mode: TickMode::Lockstep,
        shop: vec![
            ShopEntry {
                id: ItemId(0),
                cost: 500,
                components: Vec::new(),
            },
            ShopEntry {
                id: ItemId(29),
                cost: 1400,
                components: vec![ItemId(0), ItemId(19), ItemId(13)],
            },
        ],
        picks: vec![
            Pick {
                slot: SlotId(0),
                team: Team::Radiant,
                hero: HeroId(0),
            },
            Pick {
                slot: SlotId(1),
                team: Team::Dire,
                hero: HeroId(0),
            },
        ],
        rules: mid_rules(),
    }
}

/// Mid alone, the side towers out of reach, two lives and a tower.
pub fn mid_rules() -> Rules {
    Rules {
        waves: Lanes::MID,
        invulnerable_towers: Lanes::SIDES,
        death_limit: 2,
        tower_ends_it: true,
        time_limit: 27_000,
        cheats: false,
    }
}

pub fn lobby_slot() -> LobbySlot {
    LobbySlot {
        slot: SlotId(0),
        team: Team::Radiant,
        login: "sylla-fan".to_string(),
        connected: true,
    }
}

pub fn match_state() -> MatchState {
    MatchState {
        id: MatchId(42),
        name: "mid or feed".to_string(),
        mode: ModeId(2),
        host: Some(AccountId(7)),
        stage: Stage::Picking,
        slots: vec![lobby_slot()],
        watchers: vec!["eye".to_string()],
        yours: vec![SlotId(0), SlotId(2)],
    }
}

/// Picking half done, as the Dire sees it: the Radiant has its hero, the
/// Dire is set on one and has not picked it yet.
pub fn pick_state() -> PickState {
    PickState {
        seats: vec![
            PickSeat {
                slot: SlotId(0),
                team: Team::Radiant,
                login: "sylla-fan".to_string(),
                hero: Some(HeroId(2)),
                selected: None,
            },
            PickSeat {
                slot: SlotId(1),
                team: Team::Dire,
                login: "pudge-fan".to_string(),
                hero: None,
                selected: Some(HeroId(1)),
            },
        ],
        yours: vec![SlotId(1)],
        ms_left: 59_500,
    }
}

pub fn match_summary() -> MatchSummary {
    MatchSummary {
        id: MatchId(42),
        name: "mid or feed".to_string(),
        mode: ModeId(2),
        map: MapId(0),
        stage: Stage::Over,
        host: Some(AccountId(7)),
        seats: vec![SeatSummary {
            slot: SlotId(0),
            team: Team::Radiant,
            login: "sylla-fan".to_string(),
            hero: Some(HeroId(0)),
            abandoned: true,
        }],
        tournament: Some(TournamentRef {
            id: TournamentId(3),
            round: 1,
            pair: 0,
            game: 2,
        }),
        created_at: 1_800_000_000,
        duration: 54_000,
        winner: Some(Team::Dire),
        seed: Some(7),
    }
}

pub fn mode_info() -> ModeInfo {
    ModeInfo {
        id: ModeId(2),
        name: "Mid 1v1".to_string(),
        map: MapId(0),
        seats: 2,
        seats_per_player: 1,
        tick: TickMode::Realtime,
        tick_rate: 30,
        rules: mid_rules(),
        spectator_chat: false,
    }
}

pub fn tournament_info() -> TournamentInfo {
    TournamentInfo {
        id: TournamentId(3),
        name: "the cup".to_string(),
        mode: ModeId(2),
        format: TournamentFormat::SingleElimination,
        best_of: 3,
        team_size: 1,
        organizer: "sylla-fan".to_string(),
        stage: TournamentStage::Running,
        teams: vec![
            TeamInfo {
                name: "radiant".to_string(),
                members: vec![MemberInfo {
                    login: "sylla-fan".to_string(),
                    seats: 1,
                }],
            },
            TeamInfo {
                name: "dire".to_string(),
                members: vec![MemberInfo {
                    login: "pudge-fan".to_string(),
                    seats: 1,
                }],
            },
        ],
        rounds: vec![RoundInfo {
            pairs: vec![PairInfo {
                a: Some(0),
                b: Some(1),
                games: vec![
                    GameInfo {
                        match_id: Some(MatchId(42)),
                        winner: Some(1),
                    },
                    GameInfo {
                        match_id: None,
                        winner: None,
                    },
                ],
                winner: None,
            }],
        }],
        winner: None,
        created_at: 1_800_000_000,
    }
}

pub fn tournament_summary() -> TournamentSummary {
    TournamentSummary {
        id: TournamentId(3),
        name: "the cup".to_string(),
        mode: ModeId(2),
        format: TournamentFormat::RoundRobin,
        best_of: 1,
        stage: TournamentStage::Registration,
        teams: 2,
        team_size: 1,
    }
}

pub fn match_stats() -> MatchStats {
    MatchStats {
        duration: 54_000,
        slots: vec![SlotStats {
            slot: SlotId(0),
            kills: 3,
            deaths: 1,
            assists: 0,
            last_hits: 42,
            denies: 7,
            net_worth: 5400,
            hero_damage: 3100,
            structure_damage: 900,
        }],
    }
}

/// Every [`Order`] variant.
pub fn all_orders() -> Vec<Order> {
    vec![
        Order::Move {
            target: Target::None,
        },
        Order::Attack {
            target: Target::None,
        },
        Order::Move {
            target: Target::Pos(Vec2 {
                x: fixed(100),
                y: fixed(-100),
            }),
        },
        Order::Attack {
            target: Target::Pos(Vec2 {
                x: fixed(0),
                y: fixed(0),
            }),
        },
        Order::Attack {
            target: Target::Unit(entity(7)),
        },
        Order::Move {
            target: Target::Unit(entity(6)),
        },
        Order::Cast {
            slot: AbilitySlot(3),
            target: Target::None,
        },
        Order::Cast {
            slot: AbilitySlot(0),
            target: Target::Pos(Vec2 {
                x: fixed(512),
                y: fixed(512),
            }),
        },
        Order::Cast {
            slot: AbilitySlot(1),
            target: Target::Unit(entity(8)),
        },
        Order::Use {
            slot: ItemSlot(2),
            target: Target::Unit(entity(9)),
        },
        Order::Learn {
            slot: AbilitySlot(2),
        },
        Order::Buy { item: ItemId(4) },
        Order::Sell { slot: ItemSlot(5) },
        Order::Swap {
            from: ItemSlot(0),
            to: ItemSlot(7),
        },
        Order::Put {
            slot: ItemSlot(1),
            target: Target::None,
        },
        Order::Put {
            slot: ItemSlot(0),
            target: Target::Pos(Vec2 {
                x: fixed(700),
                y: fixed(900),
            }),
        },
        Order::Put {
            slot: ItemSlot(6),
            target: Target::Unit(entity(10)),
        },
        Order::Take {
            target: Target::Unit(entity(21)),
        },
    ]
}

/// Every [`EventKind`] variant.
pub fn all_events() -> Vec<EventKind> {
    vec![
        EventKind::Damaged {
            source: Some(entity(7)),
            target: entity(100),
            amount: 54,
            kind: DamageKind::Physical,
            crit: true,
        },
        EventKind::Damaged {
            source: None,
            target: entity(7),
            amount: 300,
            kind: DamageKind::Pure,
            crit: false,
        },
        EventKind::Missed {
            source: Some(entity(7)),
            target: entity(100),
        },
        EventKind::Healed {
            source: None,
            target: entity(7),
            amount: 12,
            mana: 40,
        },
        EventKind::Died {
            unit: entity(100),
            killer: Some(entity(7)),
            denied: false,
            gold: 43,
        },
        EventKind::AbilityCast {
            caster: entity(7),
            ability: AbilityId(3),
        },
        EventKind::LevelUp {
            unit: entity(7),
            level: 7,
        },
        EventKind::ItemBought {
            slot: SlotId(0),
            item: ItemId(4),
        },
        EventKind::StructureDestroyed {
            unit: entity(200),
            team: Team::Dire,
        },
    ]
}

/// A token, a ticket, a nonce or a proof: thirty-two bytes of one value.
pub fn secret(byte: u8) -> [u8; 32] {
    [byte; 32]
}

/// Every [`ToHub`] variant.
pub fn all_to_hub() -> Vec<ToHub> {
    vec![
        ToHub::Hello {
            auth: Auth::Register {
                login: "player-one".to_string(),
                password: "hunter2".to_string(),
            },
        },
        ToHub::Hello {
            auth: Auth::Login {
                login: "player-one".to_string(),
                password: "hunter2".to_string(),
            },
        },
        ToHub::Hello {
            auth: Auth::Resume {
                token: Token(secret(0xa5)),
            },
        },
        ToHub::Hello {
            auth: Auth::Guest {
                name: "eye".to_string(),
            },
        },
        ToHub::ChangePassword {
            old: "hunter2".to_string(),
            new: "hunter3".to_string(),
        },
        ToHub::Logout,
        ToHub::ListMatches {
            filter: MatchFilter {
                stage: Some(Stage::Over),
                tournament: Some(TournamentId(3)),
                account: Some(AccountId(7)),
            },
            before: Some(MatchId(42)),
            limit: 20,
        },
        ToHub::CreateMatch {
            mode: ModeId(2),
            name: "mid or feed".to_string(),
            seed: Some(7),
            seats: 2,
        },
        ToHub::FindMatch {
            mode: ModeId(2),
            seats: 5,
        },
        ToHub::CancelSearch,
        ToHub::Accept { id: MatchId(43) },
        ToHub::Decline { id: MatchId(43) },
        ToHub::JoinMatch {
            id: MatchId(42),
            role: Role::Spectator,
        },
        ToHub::JoinMatch {
            id: MatchId(42),
            role: Role::Player { seats: 3 },
        },
        ToHub::LeaveMatch,
        ToHub::StartMatch,
        ToHub::Abandon { id: MatchId(42) },
        ToHub::TakeSeat { slot: SlotId(1) },
        ToHub::DropSeat { slot: None },
        ToHub::Kick {
            account: AccountId(8),
        },
        ToHub::CreateTournament {
            name: "the cup".to_string(),
            mode: ModeId(2),
            format: TournamentFormat::SingleElimination,
            best_of: 3,
        },
        ToHub::JoinTournament {
            id: TournamentId(3),
            team: "radiant".to_string(),
            seats: 2,
        },
        ToHub::LeaveTournament,
        ToHub::StartTournament {
            id: TournamentId(3),
        },
        ToHub::CancelTournament {
            id: TournamentId(3),
        },
        ToHub::ViewTournament {
            id: Some(TournamentId(3)),
        },
        ToHub::FetchReplay {
            match_id: MatchId(42),
        },
        ToHub::Chat {
            text: "мид не идите".to_string(),
            team: true,
        },
        ToHub::Chat {
            text: "/roll 6".to_string(),
            team: false,
        },
    ]
}

/// Every [`ToGame`] variant.
pub fn all_to_game() -> Vec<ToGame> {
    vec![
        ToGame::Join {
            ticket: Ticket(secret(0x3c)),
        },
        ToGame::Select {
            seat: SlotId(2),
            hero: HeroId(0),
        },
        ToGame::Pick {
            seat: SlotId(2),
            hero: HeroId(1),
        },
        ToGame::Orders {
            orders: vec![unit_order(7), unit_order(30)],
        },
        ToGame::Ack { tick: 5400 },
        ToGame::ViewAs {
            seat: Some(SlotId(1)),
        },
        ToGame::Chat {
            text: "/item Butterfly @3".to_string(),
            team: false,
        },
    ]
}

/// One order for the unit at `idx`, as it is given and as it is told back.
pub fn unit_order(idx: u32) -> UnitOrder {
    UnitOrder {
        unit: entity(idx),
        order: Order::Attack {
            target: Target::Unit(entity(7)),
        },
    }
}

/// Every [`FromHub`] variant.
pub fn all_from_hub() -> Vec<FromHub> {
    vec![
        FromHub::Welcome {
            player_id: PlayerId(3),
            account: Some(AccountInfo {
                id: AccountId(7),
                login: "sylla-fan".to_string(),
            }),
            token: Some(Token(secret(0xa5))),
            modes: vec![mode_info()],
        },
        FromHub::MatchState {
            state: match_state(),
        },
        FromHub::Play {
            match_id: MatchId(42),
            addr: "127.0.0.1:4456".to_string(),
            ticket: Ticket(secret(0x3c)),
        },
        FromHub::Refused {
            reason: HallReason::SeatTaken,
        },
        FromHub::Hall {
            matches: vec![match_summary()],
            tournaments: vec![tournament_summary()],
        },
        FromHub::Matches {
            page: vec![match_summary()],
            before: Some(MatchId(41)),
        },
        FromHub::TournamentState {
            state: tournament_info(),
        },
        FromHub::ReplayChunk {
            match_id: MatchId(42),
            offset: 65_536,
            bytes: vec![1, 2, 3],
        },
        FromHub::ReplayEnd {
            match_id: MatchId(42),
            total: 65_539,
        },
        FromHub::Chat {
            from: "sylla-fan".to_string(),
            side: None,
            team: false,
            text: "gg".to_string(),
        },
        FromHub::Notice {
            text: "sylla-fan rolls 42 (1-100)".to_string(),
        },
        FromHub::Search {
            search: Search::Idle,
        },
        FromHub::Search {
            search: Search::Queued {
                mode: ModeId(2),
                waiting: 3,
            },
        },
        FromHub::Search {
            search: Search::Found {
                id: MatchId(43),
                mode: ModeId(2),
                accepted: 1,
                players: 4,
                yours: true,
                ms_left: 12_250,
            },
        },
    ]
}

/// Every [`FromGame`] variant.
pub fn all_from_game() -> Vec<FromGame> {
    vec![
        FromGame::Picking {
            state: pick_state(),
        },
        FromGame::PickRefused {
            seat: SlotId(0),
            hero: HeroId(1),
            reason: PickReason::AlreadyPicked,
        },
        FromGame::PickRefused {
            seat: SlotId(3),
            hero: HeroId(9),
            reason: PickReason::NoSuchHero,
        },
        FromGame::MatchStart { info: match_info() },
        FromGame::Snapshot {
            view: world_view(4),
        },
        FromGame::Events {
            tick: 5400,
            events: all_events(),
        },
        FromGame::OrderRejected {
            order: unit_order(7),
            reason: RejectReason::UnknownTarget,
        },
        FromGame::Orders {
            tick: 5400,
            orders: vec![unit_order(30), unit_order(31)],
        },
        FromGame::MatchOver {
            winner: Team::Radiant,
            stats: match_stats(),
        },
        FromGame::ParticipantLeft {
            slots: vec![SlotId(0), SlotId(2)],
        },
        FromGame::Chat {
            from: "sylla-fan".to_string(),
            side: Some(Team::Dire),
            team: true,
            text: "мид не идите".to_string(),
        },
        FromGame::Notice {
            text: "sylla-fan: /gold 500".to_string(),
        },
        FromGame::Refused {
            reason: GameReason::UnknownTicket,
        },
        FromGame::Refused {
            reason: GameReason::NotJoined,
        },
    ]
}

/// Who may enter a match: a seat holder.
pub fn pass() -> Pass {
    Pass {
        ticket: Ticket(secret(0x3c)),
        login: "sylla-fan".to_string(),
        seats: vec![SlotId(0), SlotId(2)],
    }
}

/// Everything a game server is given to run a match.
pub fn match_setup() -> MatchSetup {
    MatchSetup {
        match_id: MatchId(42),
        seed: 7,
        map: MapId(0),
        seats: vec![
            SeatSetup {
                slot: SlotId(0),
                team: Team::Radiant,
                login: "sylla-fan".to_string(),
            },
            SeatSetup {
                slot: SlotId(1),
                team: Team::Dire,
                login: "pudge-fan".to_string(),
            },
        ],
        pick_ms: 60_000,
        rules: mid_rules(),
        tick: TickMode::Lockstep,
        tick_rate: 30,
        ack_timeout_ticks: 150,
        abandon_after_ms: 120_000,
        spectator_chat: false,
        passes: vec![
            pass(),
            Pass {
                ticket: Ticket(secret(0x5a)),
                login: "eye".to_string(),
                seats: Vec::new(),
            },
        ],
    }
}

/// Every [`Assign`] variant.
pub fn all_assign() -> Vec<Assign> {
    vec![
        Assign::Challenge {
            nonce: Nonce(secret(1)),
        },
        Assign::Accepted {
            proof: Proof(secret(2)),
        },
        Assign::Start {
            setup: match_setup(),
        },
        Assign::Admit {
            match_id: MatchId(42),
            pass: pass(),
        },
        Assign::Abandon {
            match_id: MatchId(42),
            seats: vec![SlotId(0), SlotId(2)],
            login: "sylla-fan".to_string(),
        },
        Assign::Stop {
            match_id: MatchId(42),
        },
    ]
}

/// Every [`Report`] variant.
pub fn all_report() -> Vec<Report> {
    vec![
        Report::Register {
            proof: Proof(secret(3)),
            nonce: Nonce(secret(4)),
            addr: "127.0.0.1:4456".to_string(),
            capacity: 8,
        },
        Report::Picked {
            match_id: MatchId(42),
            picks: match_info().picks,
        },
        Report::Replay {
            match_id: MatchId(42),
            bytes: vec![4, 0, 0, 0, 1, 2, 3, 4],
        },
        Report::Ended {
            match_id: MatchId(42),
            winner: Some(Team::Neutral),
            stats: match_stats(),
            duration: 27_900,
        },
        Report::Ended {
            match_id: MatchId(43),
            winner: None,
            stats: match_stats(),
            duration: 120,
        },
    ]
}

/// Every [`ReplayRecord`] variant.
pub fn all_replay_records() -> Vec<ReplayRecord> {
    vec![
        ReplayRecord::Msg(FromGame::Events {
            tick: 5400,
            events: all_events(),
        }),
        ReplayRecord::Orders {
            tick: 5400,
            orders: vec![
                UnitOrder {
                    unit: entity(7),
                    order: Order::Move {
                        target: Target::None,
                    },
                },
                unit_order(31),
            ],
        },
    ]
}

/// Names a variant, and fails to compile when one is added without being
/// covered above.
pub fn to_hub_name(msg: &ToHub) -> &'static str {
    match msg {
        ToHub::Hello { .. } => "Hello",
        ToHub::ChangePassword { .. } => "ChangePassword",
        ToHub::Logout => "Logout",
        ToHub::ListMatches { .. } => "ListMatches",
        ToHub::CreateMatch { .. } => "CreateMatch",
        ToHub::FindMatch { .. } => "FindMatch",
        ToHub::CancelSearch => "CancelSearch",
        ToHub::Accept { .. } => "Accept",
        ToHub::Decline { .. } => "Decline",
        ToHub::JoinMatch { .. } => "JoinMatch",
        ToHub::LeaveMatch => "LeaveMatch",
        ToHub::StartMatch => "StartMatch",
        ToHub::Abandon { .. } => "Abandon",
        ToHub::TakeSeat { .. } => "TakeSeat",
        ToHub::DropSeat { .. } => "DropSeat",
        ToHub::Kick { .. } => "Kick",
        ToHub::CreateTournament { .. } => "CreateTournament",
        ToHub::JoinTournament { .. } => "JoinTournament",
        ToHub::LeaveTournament => "LeaveTournament",
        ToHub::StartTournament { .. } => "StartTournament",
        ToHub::CancelTournament { .. } => "CancelTournament",
        ToHub::ViewTournament { .. } => "ViewTournament",
        ToHub::FetchReplay { .. } => "FetchReplay",
        ToHub::Chat { .. } => "Chat",
    }
}

/// Names a variant, and fails to compile when one is added without being
/// covered above.
pub fn from_hub_name(msg: &FromHub) -> &'static str {
    match msg {
        FromHub::Welcome { .. } => "Welcome",
        FromHub::MatchState { .. } => "MatchState",
        FromHub::Play { .. } => "Play",
        FromHub::Refused { .. } => "Refused",
        FromHub::Hall { .. } => "Hall",
        FromHub::Matches { .. } => "Matches",
        FromHub::TournamentState { .. } => "TournamentState",
        FromHub::ReplayChunk { .. } => "ReplayChunk",
        FromHub::ReplayEnd { .. } => "ReplayEnd",
        FromHub::Chat { .. } => "Chat",
        FromHub::Notice { .. } => "Notice",
        FromHub::Search { .. } => "Search",
    }
}

/// Names a variant, and fails to compile when one is added without being
/// covered above.
pub fn to_game_name(msg: &ToGame) -> &'static str {
    match msg {
        ToGame::Join { .. } => "Join",
        ToGame::Select { .. } => "Select",
        ToGame::Pick { .. } => "Pick",
        ToGame::Orders { .. } => "Orders",
        ToGame::Ack { .. } => "Ack",
        ToGame::ViewAs { .. } => "ViewAs",
        ToGame::Chat { .. } => "Chat",
    }
}

/// Names a variant, and fails to compile when one is added without being
/// covered above.
pub fn from_game_name(msg: &FromGame) -> &'static str {
    match msg {
        FromGame::Picking { .. } => "Picking",
        FromGame::PickRefused { .. } => "PickRefused",
        FromGame::MatchStart { .. } => "MatchStart",
        FromGame::Snapshot { .. } => "Snapshot",
        FromGame::Events { .. } => "Events",
        FromGame::OrderRejected { .. } => "OrderRejected",
        FromGame::Orders { .. } => "Orders",
        FromGame::MatchOver { .. } => "MatchOver",
        FromGame::ParticipantLeft { .. } => "ParticipantLeft",
        FromGame::Chat { .. } => "Chat",
        FromGame::Notice { .. } => "Notice",
        FromGame::Refused { .. } => "Refused",
    }
}

/// Names a variant, and fails to compile when one is added without being
/// covered above.
pub fn assign_name(msg: &Assign) -> &'static str {
    match msg {
        Assign::Challenge { .. } => "Challenge",
        Assign::Accepted { .. } => "Accepted",
        Assign::Start { .. } => "Start",
        Assign::Admit { .. } => "Admit",
        Assign::Abandon { .. } => "Abandon",
        Assign::Stop { .. } => "Stop",
    }
}

/// Names a variant, and fails to compile when one is added without being
/// covered above.
pub fn report_name(msg: &Report) -> &'static str {
    match msg {
        Report::Register { .. } => "Register",
        Report::Picked { .. } => "Picked",
        Report::Replay { .. } => "Replay",
        Report::Ended { .. } => "Ended",
    }
}
