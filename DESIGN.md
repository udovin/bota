# bota — design

A simplified Dota 2 in Rust for AI bots and humans. Fully deterministic simulation,
server authority, minimal dependencies.

Code conventions — `CLAUDE.md`.

## Key decisions

| Decision | Choice | Rationale |
|---|---|---|
| Client rendering | macroquad | one direct dependency, 2D + text out of the box, Linux/Win/macOS/WASM |
| Tick mode | Realtime **and** Lockstep | humans play in realtime, bots are debugged/trained in reproducible lockstep |
| v0.1 scope | 1v1, three lanes, 1 hero | a vertical slice of the whole stack on minimal content |
| Transport | TCP + length-prefixed frames | no tokio: a thread per client + a simulation thread + mpsc |
| Serialization | `serde` + `postcard` | derive removes ~250 lines of manual put/get; the format is compact (varint) and stable within 1.x |
| Player networking | full snapshots with fog | measured: 1425 bytes per snapshot in 1v1, 41 KB/s at 30 Hz. Deltas will be needed by 5v5, not earlier |
| Live spectating | the same snapshots, without fog | client-side simulation is impossible, and that is intentional |
| Replay | fogless `ServerMsg` stream + all orders | plays back without a server: the client reads frames from a file instead of a socket; coupled to the wire format only, so it survives balance changes; the client still cannot simulate |
| PRNG | `rand_chacha::ChaCha8Rng` in `bota-server` | value-stable by the crate's contract; `rand::StdRng` is **forbidden** — it is allowed to change its algorithm in a minor release |

## Structure

```
bota/
├── Cargo.toml               # workspace, resolver = "3"
├── crates/
│   ├── bota-proto/          # shared vocabulary + codec. deps: serde, postcard
│   ├── bota-server/         # simulation + networking + lobby. deps: proto
│   ├── bota-client/         # macroquad: rendering, input, spectating. deps: proto
│   └── bota-bot/            # bot SDK + example. deps: proto
├── assets/
├── replays/
└── tests/
```

```
        bota-proto
        ↑    ↑    ↑
   server  client  bot
```

The workspace currently contains `bota-proto`, `bota-server` and `bota-client`;
`bota-bot` joins at stage 8.

The bot and the client are symmetric consumers of `proto`: a human and a bot see
literally the same `WorldView` type. No asymmetry that could be exploited.

### Membership criterion for bota-proto

The single rule deciding where code lives:

> If it does not cross the wire and is not needed to read the wire — it is not in `proto`.

| In `bota-proto` | In `bota-server` |
|---|---|
| `Fixed`, `Angle`, `Vec2`, `EntityId`, `SlotId`, `PlayerId`, `Team`, `HeroId`, `AbilityId`, `ItemId`, `AbilitySlot`, `ItemSlot`, `MapId`, `UnitKind` | `World`, `step`, units, combat, movement, vision, economy |
| `Order`, `EventKind`, `WorldView`, codec and framing | `Command`, `Event.visible_to`, `MatchRng`, `Stream`, `Chance`, `Ratio` |
| `MatchInfo`, `ClientMsg`, `ServerMsg`, `ReplayRecord` | `MatchConfig`, balance constants, hero stats, ability implementations |

A previous version of the design had a `bota-core` crate holding the simulation. It was
dropped: "core" had no checkable membership criterion, and `seed`, `tick_rate` and
`PlayerId` steadily leaked into it — exactly what does not belong there. A side benefit
of moving the simulation into the server: the client is **structurally** incapable of
simulating, `World` lives in a crate it does not depend on.

### Module layout

```
proto/src/
├── math.rs       Fixed (Q16.16 in i32), Angle (brads), Vec2
├── ids.rs        EntityId, SlotId, PlayerId, Team, HeroId, AbilityId, ItemId,
│                 AbilitySlot, ItemSlot, MapId, UnitKind
├── order.rs      Order, OrderTarget
├── event.rs      EventKind, DamageKind
├── view.rs       WorldView, UnitView, PlayerView, ProjectileView, AbilityView,
│                 ItemView, StatusFlags
├── msg.rs        ClientMsg, ServerMsg, MatchInfo, lobby, RejectReason, MatchStats,
│                 ReplayRecord
└── codec.rs      encode_frame, decode_payload, FrameReader, CodecError
```

```
server/src/
├── sim/          KNOWS NOTHING ABOUT SOCKETS
│   ├── arena.rs      Arena<T>: generational slot store behind EntityId
│   ├── rng.rs        MatchRng over ChaCha8Rng: streams by purpose, Ratio/Chance
│   ├── config.rs     MatchConfig
│   ├── world.rs      World: entity arenas, tick, spawn, FNV-1a hash
│   ├── units.rs      Unit, UnitOrder, SeatState; hero/creep/building constructors
│   ├── heroes/       hero stats and ability implementations (stage 9)
│   ├── abilities.rs  ability engine: cast point / channel / cooldown / mana (stage 9)
│   ├── combat.rs     windups, projectiles, the damage queue, armor and resist
│   ├── movement.rs   isqrt, stepping, blocking, turning, passability grid
│   ├── path.rs       A* over the grid, grid line of sight
│   ├── vision.rs     fog of war: pure radius queries, nothing cached
│   ├── econ.rs       gold, experience, levels, deaths, respawns
│   ├── rules.rs      balance constants
│   ├── project.rs    World → WorldView
│   └── step.rs       Command, Event, validate, step: the tick order
├── net/          accept loop, per-connection reader/writer threads, Outbox
├── lobby.rs      Roster (PlayerId ↔ SlotId), seats, picks, readiness
├── game_loop.rs  lobby phase, then the tick loop in both modes
├── replay.rs     writes the replay: fogless frames plus per-tick orders
└── main.rs       clap arguments
```

No ECS: `World` is a set of `Arena<T>` stores — generational slot arenas iterated in
slot order. Removing an entity bumps the slot's generation, so a stale `EntityId` never
resolves to whoever took the slot over.

## Contracts

### Simulation (server/src/sim)

```rust
pub struct MatchConfig {
    pub match_id: u64, pub master_key: [u8; 32], pub picks: Vec<Pick>,
    pub map: MapId, pub tick_rate: u16, pub mode: TickMode, pub ack_timeout_ticks: u32,
}

impl MatchConfig {
    pub fn rng(&self) -> MatchRng;        // see below
    pub fn info(&self) -> MatchInfo;      // projection onto the wire, the type has no seed field
}
```

The match seed is derived with `rand_chacha` itself, no separate hash function needed:
`master_key` is a 32-byte seed, `match_id` is a stream number. A match is reproducible
from the pair `(master_key, match_id)`, which is convenient for debugging. Implemented
in `sim/rng.rs`:

```rust
// MatchRng::new(master_key, match_id)
let mut root = ChaCha8Rng::from_seed(*master_key);
root.set_stream(match_id);
let mut seed = [0u8; 32];
root.fill_bytes(&mut seed);

// one stream per purpose — crits, runes, spawn scatter
rng.global(Purpose::Rune)
rng.for_unit(Purpose::Crit, unit, source)
```

Streams are separated by purpose (`Purpose`: `Crit`, `Block`, `Evasion`, `Rune`,
`NeutralSpawn`) so that a new draw in one place does not shift generation anywhere
else. A per-unit stream is keyed by `(purpose, slot index, source)` packed into the
64-bit ChaCha8 stream id — purpose in the top bits, slot index in the middle, a source
byte to separate several sources of chance on the same unit (a crit passive and a
bash). The key uses the slot index rather than the full `EntityId`: the stream id
space stays bounded, and a unit reusing a freed slot continues that slot's hidden
sequence, which no observer can distinguish from a fresh one.

```rust
impl World {
    pub fn new(cfg: &MatchConfig, rng: MatchRng) -> World;  // rng is initial state, not config
    pub fn step(&mut self, cmds: &[Command]) -> Vec<Event>;
    pub fn view(&self, team: Team) -> WorldView;        // with fog
    pub fn view_full(&self) -> WorldView;               // spectator
    pub fn can_see(&self, team: Team, target: EntityId) -> bool;   // order validation
    pub fn winner(&self) -> Option<Team>;
    pub fn stats(&self) -> MatchStats;
    pub fn hash(&self) -> u64;                          // determinism check
}
```

`seed` is not configuration but initial hidden state, so it is passed as a separate
argument, already a constructed `MatchRng`; the policy of deriving it belongs entirely
to the server.

The simulation knows about `SlotId` but not `PlayerId`: network identity must not leak
into the rules of the game. The mapping table lives in `Roster` in the lobby.

`Event.visible_to` is computed by the simulation — who sees what is a gameplay
question. The network layer only routes; what goes on the wire is `EventKind` without
the mask.

### Exchange

```
server:  World --view(team)--> WorldView --encode_frame--> socket
client:  socket --FrameReader--> WorldView --> rendering
```

The client's `WorldView` is born from the wire, not from a constructor. The client has
no `World` and cannot have one, so there is no second `new` contract either. The
asymmetry of the sides is expressed by two types — `World` versus `WorldView` — not by
two implementations of one trait.

There is intentionally no trait over `World`: no polymorphic call sites exist, and
determinism demands exactly one implementation.

## Determinism rules

1. No `f32/f64` in `bota-proto` and `bota-server`: `#![deny(clippy::float_arithmetic)]`.
   Float operations themselves are deterministic per IEEE-754, but `sin/cos/sqrt` from
   libm are not — they differ across glibc / musl / macOS / wasm. The client renders in
   float freely. The bot is also free to think in float: what gets recorded are its
   orders, not its reasoning.
2. Scalars are `Fixed` = Q16.16 in `i32`, multiplication through an intermediate `i64`.
   Range ±32768 units, precision 1/65536. The 16384-unit map keeps squared distances
   inside an `i64`; segment projections that would square a dot product go through
   `i128`.
3. Angles are "brads": `u16`, 65536 = a full turn. sin/cos from a hardcoded table of
   1024 entries.
4. Distances are compared as squares, no sqrt.
5. Entity iteration is always by ascending `EntityId.idx`. `HashMap` is forbidden in
   the simulation.
6. Commands are sorted by `(tick, slot, seq)` before applying.
7. Time exists only as ticks (`u32`), 30 ticks/sec. No `std::time` in `sim`.
8. Damage, gold, experience are integers.
9. External primitives are taken only if value-stable. `rand::StdRng` and
   `std::collections::hash_map::DefaultHasher` explicitly give no such guarantee
   between releases: the former is replaced by `rand_chacha::ChaCha8Rng`, and for
   `world.hash()` we write FNV-1a (ten lines, xor and multiply in a loop).
10. `world.hash()` covers the whole state, hidden included: entity arenas, stream
    positions, every `Chance` mask. A divergence in randomness consumption must move
    the hash on the tick it happens, not when its first visible outcome differs.

### Chances (crits, block, evasion)

The requirements conflict: an exact 30% rate **and** no way to predict the next crit.
A simple accumulator (`acc += 0.30`, firing at `acc >= 1`) gives the first but not the
second: it is periodic, an observer sees crits in damage events and reconstructs the
whole phase after one or two observations.

The solution is exact counting per block with a hidden order inside the block. A chance
is declared as a fraction:

```rust
pub struct Ratio { num: u8, den: u8 }     // rules.rs: CRIT_CHANCE = Ratio::new(3, 10)

pub struct Chance {
    stream: Stream,   // this source's own hidden ChaCha8 stream
    mask: u64,        // which attempts of the current block hit
    idx: u8,          // position within the block
    current: Ratio,   // the ratio the current block was built with
}

impl Chance {
    pub fn roll(&mut self, ratio: Ratio) -> bool {
        if self.idx >= self.current.den() {
            self.current = ratio;                     // new ratio takes effect here
            self.reshuffle();                         // partial Fisher-Yates off the stream
        }
        let hit = self.mask & (1 << self.idx) != 0;
        self.idx += 1;
        hit
    }
}
```

- The rate is exactly `num/den` per block. Balance constants are declared as fractions,
  `den <= 64` (the width of the mask).
- The order comes from a ChaCha8 stream: observing past crits says nothing about the
  next block. A replay reproduces bit-for-bit — the stream is deterministic, just
  unreachable from outside.
- The initial block offset of every source is drawn from the same stream, so block
  boundaries are not known to an observer.

Residual leak: within a block the opponent can count (saw 3 crits in 6 hits — knows the
next 4 are clean). Unavoidable for any scheme with an exact rate. The alternative is
giving up the exact rate for a plain hidden PRNG, which contradicts the determinism
requirement.

When the chance changes (a crit buff), the current block finishes under the old
fraction; the new one takes effect from the next block.

The PRNG is also used where an exact rate is meaningless (rune drop, spawn scatter).
Streams are separated by purpose via `ChaCha8Rng::set_stream`, so that a new call in
one place does not shift the rest of the generation.

### Hidden state

The hard rule: the client receives nothing from which a future outcome can be derived.
Players send intents, the server alone computes, and every tick it broadcasts the
outcome.

Secrecy is a property of the channel, not of the data, so the simulation does not think
about it. The protection is structural: the types of `bota-proto` are incapable of
expressing hidden state. `MatchInfo` versus `MatchConfig`, `WorldView` versus `World`.
A leak becomes a compile error, not a review oversight.

The seed is **never** published — not even in replays: a replay is a recorded stream
plus the orders, nothing in it is re-simulated, so nothing in it needs the seed.

Server-only, never reaches `WorldView`:

- `MatchRng` and the positions of all streams;
- each unit's `Chance { mask, idx }`;
- outcomes of scheduled events that have not happened yet (which rune will spawn).

Checked by a test: `view_full()` is run through serialization and compared against a
whitelist of fields, so a new field in `Unit` cannot leak silently.

It also follows that client-side prediction covers only movement and animation. Damage
numbers and the fact of a crit arrive as events from the server.

Two further channels are closed by rule, because a reward-driven bot will find and
exploit any leak a human reviewer shrugs off:

- A reject reason does not depend on hidden state. A dead target and a fogged one get
  the same `UnknownTarget`, so probing the fog with stale handles reveals nothing.
- A unit never acts on what its team cannot see. A standing `AttackUnit` order whose
  target left the team's vision degrades to attack-moving toward the last seen
  position; the unit does not track the hidden target, so its own path reveals
  nothing either.

## Game model v0.1

- Map 16384×16384 — Dota's scale, so speeds, ranges and vision keep their Dota
  absolute values. Symmetric along the diagonal. Three lanes: mid along the diagonal,
  top up the west edge and along the north edge, bottom its mirror; the diagonal
  mirror that swaps the sides also swaps top and bottom. Passability is a 256×256 bit
  grid.
- Teams Radiant / Dire, 1v1 (the architecture is sized for 5v5).
- Buildings: three towers per lane per side — tier one by the river, tier three by
  the base — plus a pair of tier fours by each Ancient. The Ancient is invulnerable
  until its last tier four falls. A fountain that heals and burns. Barracks and the
  rest come later.
- A lane's centerline runs through every tower of the lane, so a wave marches from
  tower to tower and can never wander past one outside its own acquisition range.
- The match opens with a 30 s pregame: the game clock counts up from -0:30 and the
  first wave walks out at 0:00, when passive gold starts. `MatchInfo` carries the
  pregame length so the clock renders without knowing the ruleset.
- The landmarks are the current Dota 2 map, extracted from the installed game's
  `dota.vpk` with ValveResourceFormat and shifted by half the map so Dota's
  origin sits at the center: `MAP_SIZE` 18432, both fountains, both Ancients,
  all 22 towers, all 6 lane spawners and all 28 neutral camps at their real
  positions. The two sides are not mirrors; each carries its own table, and
  `mirror()` survives only as a utility.
- The terrain is the same map's own ground, baked in `sim/terrain.rs`: the
  gridnav's static walkability (cliffs, pits, the map edge close their cells
  before trees and buildings do) and, from the physics mesh, an elevation tier
  per cell in 128-unit steps — river bed 0, lane ground 1, highground 2, bases
  3 — plus the water mask of the river and pools. A ranged attack landing on
  ground higher than it was fired from misses one time in four, from a
  match-global exact-ratio chance stream; abilities never miss. The terrain
  rides in `MatchStart` run-length encoded, and the client bakes it into the
  ground texture for the world and the minimap.
- Vision is a radius with sight lines walked over the terrain cells. A cell is
  opaque to a viewer when its ground is higher than the viewer's, when a tree
  stands on it, or when one of the map's own fog blocker walls crosses it —
  eleven named walls of `ent_fow_blocker_node` points, imported like
  everything else, which is what seals the Roshan pit even through its
  entrance. A named group holds several separate walls: only nodes within
  the blocker span of each other bridge a segment, so the far-apart jumps
  inside a group — the two pits share one name — are breaks, not walls. Buildings, water and units block nothing. The viewer's own cell
  and the target's cell never block, so standing beside a tree does not blind
  and a treeline's edge stays visible; a target on ground above the viewer is
  always dark. The opaque cells ride in `MatchStart`, and the client walks
  the same sight lines from its own units to shade unseen ground in the world
  view and on the minimap; spectators see everything. The map's
  `ent_fow_revealer` points wait for outposts.
- Trees are static blockers imported one for one from the same map: all 2475
  positions — the main entity lump plus the base layers of both sides — live as
  a table in `sim/trees.rs`. Two carves adapt them to this map: trees within
  the lane-clear band of a straightened lane centerline are dropped — the real
  forest follows the real curved roads, and these lanes walk tower-to-tower
  chords — and a small pad around each fountain stays clear. The
  full tree list rides in `MatchStart`, so the client draws without knowing the
  layout rules. Trees are closed into the passability grid at world build; they
  do not block vision yet and are indestructible until an axe exists.
- Roshan stands on the map's own spawner point in the south-east river pit —
  both pit enclosures came in with the terrain, and the north-west one stays
  empty until day and night exist. He behaves like a neutral: answers whoever
  comes close or hits him, leashes back to the pit and heals in full. His
  death pays the killing seat the bounty, every seat of the killing team the
  team gold, and experience around the pit; the grave lasts eight minutes plus
  up to three more on a hidden draw, then he returns. No Aegis until items can
  resurrect.
- The jungle belongs to `Team::Neutral`, hostile to both sides; seats never sit
  there. The twenty-eight camps stand where Dota's own neutral spawners stand. They
  fill with neutral creeps one minute past the horn and every minute after, but only while the camp box is empty — any body inside
  blocks the spawn, which is camp blocking. A neutral answers whoever comes into
  its aggro range or hits it, and dragged beyond its leash it goes home deaf and
  arrives at full health. Its bounty goes to the killer, its experience to the
  killer's team nearby.
- Creeps: 3 melee + 1 ranged every 900 ticks (30 s) on every lane, a siege creep
  every 5th wave. A wave marches its own lane's waypoints and is leashed to its own
  lane.
- Hero: Sylla (ranged carry). 3 abilities + an ultimate, levels 1–10.
- Economy: passive gold 1/sec, last hits, kill bounty with streaks.
- Items follow the Dota slot topology, engine in `sim/items.rs`. A seat owns
  fifteen slots: six inventory, where items work; three backpack, where they ride
  inert — and a stack leaving the backpack for the inventory is muted for six
  seconds before it works again; six stash. A purchase spends gold anywhere, but lands in the
  inventory only inside the home shop area (the fountain circle) — bought
  remotely it waits in the stash, and the stash itself opens only at that shop.
  Selling also happens at the shop: half price back, the full price for an
  untouched item within ten seconds of purchase. `MoveItem` swaps any two slots.
  Carried bonuses are flat and apply only from unmuted inventory slots; growing a
  pool keeps its filled fraction. Consumables (Healing Salve, Clarity) drip over
  thirty seconds and spill on any hit from a hero. Items survive the hero's
  death on the seat. No courier yet.
- Fog of war is mandatory: without it a bot learns to play with full information.
- Victory: the Ancient falls.

Aggro replicates Dota. Nobody ranks targets by kind: creeps and towers take the
closest enemy in reach — creeps fighting creeps and towers shooting creeps are
emergent, because creeps arrive first. On top of that sit the aggro calls:

- An attack order against an enemy hero, and every attack swing at one, calls the
  victim's creeps within a radius of the attacker — and the victim's towers the
  attacker stands in reach of — onto the attacker. Creeps hold the grudge for a
  couple of seconds; a tower holds its target for as long as it stays in reach.
- An order aimed at any ally calls enemy creeps and towers off the orderer: the
  classic last-hit-under-pressure trick. Against a healthy ally the order itself is a
  follow, turning into a deny once the ally is low enough.
- Each creep and tower can be called onto a target at most once per call cooldown. A
  call-off works at any time, and until that cooldown expires the called-off unit
  prefers any other target over the orderer, coming back only when nobody else is in
  reach: the trick redirects, it does not blind.
- A creep holds a non-hero target until it dies or the chase breaks: standing closer
  steals no attention, so last-hitting next to a busy wave is safe. A target inside
  the creep's own attack range is held no matter what it is — a ranged creep keeps
  firing at a hero who stays in its range. What the aggro window limits is chasing a
  hero beyond that range: when it closes, the creep re-assesses from the closest
  again, and a kited chase loses to whatever got closer on the way, a tower included.
  Last-hitting creeps calls nobody.
- A hero fights only when told to, but a fight it was told to have carries itself:
  when an attack order's target dies, the attack rolls onto the closest enemy in
  acquisition range. Rest never starts one — standing idle, arriving off a move
  order or stopping leaves the wave alone. A move order ignores enemies for its
  whole length, Hold attacks whatever is in range without moving, attack-move
  acquires along the way.
- Collision follows Dota's documented two-pather model. The long path is planned
  against static blockers only — structures on the grid; the short path steers the
  walker along it around standing bodies in continuous space: within the steer
  range the walker aims at a tangent point of the first standing circle across its
  segment, resolved over a few hops when one tangent uncovers the next body, so a
  stander is skirted along its hull at full speed — standing in front of a wave
  does not hold it, exactly as in Dota, where stationary units are avoidance
  obstacles for the short pather. Whoever occupies the walker's own goal is not
  steered around. All contact is solid: the distance between two units never drops
  below the sum of their radii, a step deeper into anybody's circle is refused,
  and a walker pressed right against a stander traces its circle by sidesteps.
  The static grid is a hard wall too: a step or sidestep into a cell closed by a
  structure or a tree is refused outright, while a step out of one is always
  allowed, so nothing ever wedges inside the forest. A
  unit that is walking is not avoided at all: whoever runs into it presses into
  the body, fully stopped, for the block wait, and only then starts sidestepping
  around. That stop, paid again on every new contact, is what makes creep-blocking
  work — a hero zigzagging across the wave's path re-stops every creep whose step
  his hull intersects, while a hero standing still is simply flowed around. Nobody
  is ever pushed: the unit standing its ground does not move a hair, and a unit
  stands still for its whole attack point and backswing.
- A calm creep dragged off the lane beyond its leash gives up, goes deaf to targets
  and walks straight back to the nearest point of the lane; an open aggro window
  overrides the leash.
- Units turn at a finite rate and only walk or swing once they face their current
  path leg, so corners cost time. Buildings do not turn.

Movement routes around structures with A* over the passability grid: structures close
cells when the world is built and reopen them when they fall. A unit walks straight
whenever the grid says the line is clear, and otherwise follows the corner waypoints
of a route; each leg is walked only after turning onto it, so corners cost time.
Standing units are hugged around at contact, walking units stop whoever runs into
them — and every swing ends in a backswing the unit stands through, which is the
pause a creep makes over its kill before marching on. A hero's order cancels its
backswing.

Abilities run on a shared engine in `sim/abilities.rs`: four slots per hero, each
with a level and a cooldown, held on the seat like items, so both survive the
hero's death — and cooldowns keep running while it is dead. A skill point arrives
with every hero level; basic ability level k needs hero level 2k-1, ultimate
levels open at 6, 8 and 10. A cast
order is validated (learned, off cooldown, mana, target kind, cast range) and
executes in the ability phase of the same tick, instantly — cast points and
channeling come later. Casts of the same tick run after cooldown ticking, so a
fresh cooldown surfaces at its full value. Sylla's kit: slot 0 a critical strike
passive fed by the hidden per-unit `Chance` stream (the stream is keyed by the
arena slot, so respawning continues the sequence); slot 1 an attack speed
self-buff that shortens the attack interval for its duration; slot 2 a magical
projectile that bounces to the closest unhit enemy in range, never a structure;
slot 3 an ultimate volley launching an attack projectile at every enemy unit in
its radius. A crit rolls once at windup completion and rides the projectile,
reported only in the `Damaged` event.

Hero roadmap (added as data + ability implementations; the engine does not change):

| Hero | Type | Abilities |
|---|---|---|
| Sylla | ranged carry | crit passive / attack speed buff / bouncing projectile / ult: multishot |
| Krag | melee tank | stun dash / cleave passive / armor aura / ult: shield + damage return |
| Vex | ranged nuker | nuke / slowing AoE / mana passive / ult: AoE burst |
| Grum | melee initiator | hook / DoT aura / slow / ult: AoE stun |
| Lira | support | heal / shield / wards / ult: team heal aura |

## Protocol

Frame: `u32 len (LE) | postcard payload`. The message kind is the postcard enum tag
inside the payload. TCP, `TCP_NODELAY`.

Until the first release there is no versioning and no compatibility: the wire carries
no version field and no ruleset fingerprint, and nothing — protocol, replay files,
hash baselines — promises to survive across pre-release commits. Mismatched builds
are not detected; they are simply not run against each other. Version and fingerprint
fields appear with the first release, when there is something to be compatible with.

### Client → server

```rust
enum ClientMsg {
    Hello { role: Role, name: String },    // Role: Player|Bot|Spectator
    PickHero { hero: HeroId },
    SetReady(bool),
    Order { seq: u32, order: Order },
    Ack { tick: u32 },                     // lockstep: "I am ready for the tick"
}

enum Order {
    Stop, HoldPosition,
    Move { pos: Vec2 }, AttackMove { pos: Vec2 }, AttackUnit { target: EntityId },
    CastAbility { slot: AbilitySlot, target: OrderTarget },
    UseItem { slot: ItemSlot, target: OrderTarget },
    LevelUpAbility { slot: AbilitySlot },
    BuyItem { item: ItemId }, SellItem { slot: ItemSlot },
}

enum OrderTarget { None, Point { pos: Vec2 }, Unit { target: EntityId } }
```

Casting is one variant, `CastAbility`, with the target kind expressed by
`OrderTarget`; whether the variant fits the ability is validated by the server
(`RejectReason::WrongTargetKind`).

There is no chat in the protocol. The server exists for local bot testing; a message
type that plays no part in the match is not worth its slot in the wire format.

### Server → client

```rust
enum ServerMsg {
    Welcome { player_id: PlayerId, slot: Option<SlotId>, tick_rate: u16, mode: TickMode },
    LobbyState { slots: Vec<LobbySlot> },
    MatchStart { info: MatchInfo },        // the type has no seed field
    Snapshot { view: WorldView },          // whole; the tick is inside the view itself
    Events { tick: u32, events: Vec<EventKind> },
    OrderRejected { seq: u32, reason: RejectReason },
    MatchOver { winner: Team, stats: MatchStats },
    ParticipantLeft { player_id: PlayerId, slot: Option<SlotId> },
}
```

A Player/Bot receives a whole `WorldView` on every tick, filtered through its team's
fog. A live Spectator receives the same stream unfiltered. There are no deltas: a
client can start rendering from any snapshot, so joining mid-match requires nothing
special. Measured: 1425 bytes per snapshot in 1v1 (25 entities), 41 KB/s at 30 Hz.
`diff`/`apply` will arrive together with 5v5, where it would reach about 150 KB/s.

Within a tick the order on the wire is fixed: `Snapshot(t)`, then `Events(t)`. An
event may name an entity the snapshot no longer carries — it died on that very tick;
the previous snapshot is what to render such an event against.

A slow consumer cannot stall the simulation: the tick thread never blocks on a
socket. Snapshots are coalescible by construction — each one is whole, so a
connection that fell behind is sent only the latest. Events cannot be skipped, so a
connection whose event queue overflows is closed instead of buffered without bound.

The vision mask does **not** go over the wire. It is derived from the positions and
`vision_radius` of the units already present in the view: your own units are always
visible, so the computation needs nothing beyond what the client already received.
Saves two kilobytes per snapshot.

There is no shared implementation in `proto`, and that is not an omission. The sides
need different things: the server — an exact answer to "does this team see this
point", the client — a soft gradient for rendering with fade-out and a memory of the
explored, the bot often nothing at all. One shared function would either spoil the
picture or coarsen the filter.

The server-side representation of vision is intentionally not pinned down in the
design. A grid is not the only option and probably not the best one: as soon as trees
and vision cones appear, region geometry becomes both more precise and cheaper. This
is decided when `sim/vision.rs` is implemented. For the same reason each side keeps
its own mask type and grid constants — they do not cross the wire and are not needed
to read it.

This rests on two conditions, and breaking either puts the mask back on the wire:

1. Vision is a pure radius. As soon as terrain starts occluding it, the computation
   becomes a game rule, and rules do not belong in `proto`.
2. Every source of vision is represented by an entity in the view. Wards already
   satisfy this; an ability granting vision without a unit must be modeled as an
   invisible source entity.

Divergence of the computations is safe: the server alone decides which units enter the
view, so a bug in the client's mask paints the ground a wrong shade and reveals
nothing.

A replay is a self-contained recording that plays back without a server. A `.brp`
file is a sequence of length-prefixed frames, framed exactly like the socket, each
holding a `ReplayRecord`:

```rust
enum ReplayRecord {
    Msg(ServerMsg),                                       // the fogless spectator stream
    Orders { tick: u32, orders: Vec<(SlotId, Order)> },   // what every seat asked for
}
```

The server records `MatchStart`, then per tick the fogless `Snapshot`, the `Events`
and the orders it accepted. `ReplayRecord` lives in `bota-proto`: a replay file is a
wire, and the client must read it. Replay mode in the client is reading frames from
the file instead of the socket; the order records can be skipped, or rendered — which
is what makes a replay more useful than a live spectator seat when debugging a bot.
Nothing in the file is re-simulated: a replay is coupled to the wire format only,
survives balance changes, and grants the client nothing beyond what a live spectator
already gets. Snapshots are whole, so rendering can resume from any point of the
file. In 1v1 the stream runs at about 2.5 MB per minute; archiving is an external
compressor's job, not the protocol's.

### Tick modes

- `Realtime` — 30 Hz on the wall clock, a late command applies on the next tick.
- `Lockstep` — the server waits for `Ack(tick)` from every agent. A bot thinks as long
  as it needs, the match reproduces bit-for-bit. `--ack-timeout` guards against a hung
  bot (empty order).

## Server algorithm

```
main:
  parse args (--mode realtime|lockstep, --tick-rate, --players, --replay out.brp)
  listen TCP; the accept thread queues connections
  state = Lobby

Lobby:
  Hello → Welcome + LobbyState
  PickHero / SetReady; when every slot is ready:
    world = World::new(&cfg, cfg.rng());
    broadcast MatchStart { info: cfg.info() }; state = Playing

Playing (simulation thread):
  loop {
    // 1. gather input
    realtime: drain incoming until deadline = tick_start + 1/rate
    lockstep: wait for Ack(t) from every agent (or ack-timeout)

    // 2. validation and PlayerId → SlotId translation through Roster
    cmds = incoming
        .filter(the slot belongs to this player_id)
        .filter(world.can_see(team, target))     // anti-cheat: no clicking into fog
        .sort_by(slot, seq)
        .dedup_by(slot)                          // 1 order per slot per tick, the last one wins
    // 3. append the accepted orders to the replay
    // 4. events = world.step(&cmds)
    // 5. broadcast: teams get their view; spectators get the fogless view;
    //    events go out by Event.visible_to. The fogless frames also go into the replay
    // 6. if world.winner().is_some() { broadcast MatchOver; flush; break }
    realtime: sleep until the next tick with drift compensation
  }
```

### Order inside `world.step()`

Fixed. Changing it invalidates every recorded replay and every hash baseline.

```
1.  tick += 1
2.  apply orders → unit.order
3.  scheduled events: creep wave, neutral and hero respawns, runes
4.  status tick: buff durations, DoT, cooldowns, hp/mana regen
5.  aggro: towers and creeps pick targets by deterministic priorities
6.  order execution: movement, collision separation
7.  attacks: attack point → projectile launch / instant hit; projectile movement
8.  abilities: cast point → effect, channeling
9.  damage queue resolution: armor, magic resist, crit, block → apply
10. deaths: gold/xp by radius, respawn timers, denies
11. vision recompute, per-team fog masks
12. victory condition check
13. return the accumulated Events
```

## bota-bot

```rust
pub trait Bot {
    fn seated(&mut self, slot: Option<SlotId>);
    fn match_started(&mut self, info: &MatchInfo);
    fn on_tick(&mut self, view: &WorldView) -> Option<Order>;
    fn on_events(&mut self, _tick: u32, _events: &[EventKind]) {}
    fn on_reject(&mut self, _seq: u32, _reason: RejectReason) {}
    fn finished(&mut self, _winner: Team, _stats: &MatchStats) {}
}

pub fn play<B: Bot>(bot: &mut B, seat: &Seat) -> io::Result<Outcome>;
```

The hero is picked when the connection is made rather than returned from a callback:
picking happens in the lobby, before there is a `MatchInfo` to decide from.

`on_events` earns its place. The attack cycle is not on the wire — a `UnitView` carries
`attack_interval` but not where in the interval a unit stands — so a bot cannot tell
whether it may swing now or in forty ticks. What is on the wire is every blow that
lands: one of the bot's own says the cycle began a wind-up ago and comes round again an
interval after that. Without this the bot orders last hits it cannot take for another
second and loses them all.

### One order a tick, and saying nothing

The server keeps one order per seat per tick and the last one wins, so a want is a
single `Order` and the policy ranks its wants rather than queueing them. Re-sending the
want already standing is not free: an order cancels the recovery after a swing and calls
the creeps onto whoever gave it. So a want equal to the one in hand is not sent again
for `resend_ticks`, and two walks to spots less than `resend_drift` apart count as one
want — otherwise following a moving wave throws away the route the server laid every
tick.

### The courier

A courier is what makes the shopping list past the first trip worth having: anything
bought away from the shop falls into a stash the hero cannot reach from the lane, so
without one the only way to spend gold is to walk home for it. With one, the bot buys
wherever it stands — and only while a courier of its own is standing, because gold in
hand is worth more than an item in a stash nothing is coming for.

The errands are abilities the courier carries, so an order for one names the courier in
`ClientMsg::Order`'s `unit` and casts the slot the errand sits in. Which slot that is, is
read off the courier rather than assumed: the order the server fills its book in is the
server's business. A `Want` therefore carries whom it is for, and what the bot answers
with each tick is an `Ask` — an order and a unit — rather than an order alone.

An errand outlives the tick it was given in. Saying it again changes nothing about what
the courier does and costs the one order the seat has that tick, which is an order the
hero did not get to give: the first cut of this sent five hundred and thirty-six errands
in one match, and the hero took a quarter more damage for want of the ticks. So an errand
already under way is not repeated until `courier_repeat` ticks have passed, which is a
safety net for an order that never arrived rather than a schedule.

A trip is not made for one item: it waits until `courier_batch` of them have piled up or
the first has waited `courier_patience` ticks. And it is held back entirely while the bot
is being shot at — a courier walks to where its owner stands, and where its owner stands
is what is shooting.

### The numbers held apart from the decisions

Every threshold the policy weighs — how low is low, how far is far, how many ticks a
swing takes to land — is a field of `Params` with a range, not a constant at the place
that reads it. Three things follow: a run can be handed a set from a file, a search can
walk over the set, and the numbers standing for what the wire does not carry (the wind-up,
the arrow's speed, what an ability reaches) are tuned the same way as the numbers standing
for taste. `Params` is `f32` throughout: the bot is allowed float, and uniform fields are
what let a search treat the set as a vector without a case per field.

`crates/bota-bot/params.txt` is where a trained set lives and the one thing training
writes. It is committed, and it is **baked into the binary** with `include_str!`: a bot
that had to find a file beside itself would play differently depending on where it was
copied to. `Params::PATH` is the same file as a path, from `CARGO_MANIFEST_DIR`, and only
training uses it — it reads that file to carry on from where the last run stopped and
writes it when it improves. So a run that improves the numbers takes a rebuild before
anything plays by them.

Three sets, and the difference matters:

| | What it is | Who plays by it |
|---|---|---|
| `Params::default()` | the numbers the code was written with | the tests, and `--plain` |
| `Params::learned()` | `params.txt` as baked in at build time | `Brain::new()`, and `play` |
| the file at `Params::PATH` | `params.txt` as it is on disk now | `train`, as its starting point |

The tests pin the policy against `default()`, never against the learned set: what a test
measures is the decision, and a trained set is data that moves under it. A test does
check that the committed file reads back, so a knob renamed or taken away fails the build
rather than silently dropping the bot to the plain numbers.

### Self-play

Training is `bota-bot train`, and it needs nothing the server does not already do. A
server plays one match and exits, so a bout starts one on a port the system picks, joins
both seats, and kills it with the bout. Joining waits for the seat: seats go out in the
order the server sees connections arrive, two connections made back to back arrive in
whichever order the threads behind them run, and a set that played the same side twice is
measured against a side rather than against an opponent. That was a real bug and it made
every measurement bimodal. Lockstep is what makes it
worth doing: the server advances as soon as every seat has acknowledged the tick, so a
match runs as fast as the two brains think — twelve thousand ticks, a little under seven
minutes of game, in about three seconds. A bot that does **not** acknowledge holds the
match at the straggler timeout, one tick at a time.

The search is a (1+λ). A round breeds several challengers out of the set in hand, each
differing in a few numbers, and measures every one of them the same way: two matches
against a champion, one from each side. Whichever came out furthest ahead of the champion
takes the set in hand, if it came out further ahead than that set did. Both sides are
played because the map is not symmetric to a search: left to one side it would learn the
side rather than the game. The matches of a round are independent, so they run at once — a
round costs two matches of wall clock, not fourteen.

Measuring against a champion rather than head to head against the set in hand is what
makes a round mean something. Head to head, the thing being climbed moves under the search
every time it takes a step: a challenger that beat the set in hand says nothing about the
round before it, the number in the journal drifts, and a run of them walks rather than
climbs. That was tried first and it is what it did — twenty rounds of taking challengers
left the set no better against a fixed opponent than it started. Against something frozen,
better is one number that means the same in the first round and the hundredth.

The champion is frozen, not fixed: it starts as the numbers the code was written with and
the best set replaces it every `champion_every` rounds, so the bar rises. A search
measured against one weak opponent forever learns to beat that opponent.

How far a nudge reaches is not fixed either. It widens while more than a fifth of the
challengers beat the set in hand and narrows while fewer do: a search that keeps failing
is reaching too far, and one that nearly always succeeds is not reaching far enough. A
fifth is the old rule of thumb. What matters about it is that both halves happen —
widening on any success at all only ever widens, which is the same as not adapting.

What is scored is mostly what the seat did — creeps taken, creeps denied, levels, gold,
damage put on the other hero and damage taken — because two even bots farm for twenty
minutes and neither Ancient falls; the win itself outweighs any of it when it does come.
The damage is counted from `EventKind::Damaged` rather than read from `MatchStats`: the
final numbers arrive only when a match runs to its end, a match played for a fixed span
never does, and the server reports `hero_damage` as zero besides. Without those two columns
the whole fighting half of the policy is unconstrained — every match ends nought kills to
nought, so nothing else in the score can tell a bot that harasses from a bot that does not.

Nothing about this reaches into `bota-server`: the trainer is a process that starts other
processes, and the bot still sees only what its own side may see. A search that could see
through the fog would learn to.

`Watched` writes a line a tick — where it stood, what it had, what was near, and the order
it gave. It is what a match is read back from without watching it, and it is the shape a
policy learned from recorded play would be trained on.

A forward model (rolling out hypothetical futures for planning) is not supported and
not needed now: the bot has no hidden state, so it could not roll forward with the
real engine anyway. If it becomes needed, the simulation moves out of `bota-server`
into its own crate, which is cheap since `sim/` is already a separate subtree with no
dependencies on the network layer.

## Stages

| # | Deliverable | Contents |
|---|---|---|
| 0 | ✅ workspace builds | Cargo.toml, `bota-proto`, workspace lint policy |
| 1 | ✅ `bota-proto` types | `Fixed`, `Angle`, `Vec2`, identifiers, `Order`, `EventKind`, `WorldView`, messages |
| 2 | ✅ codec | serde derive, postcard, framing, `FrameReader` |
| 3 | ✅ `bota-proto` tests | round-trip of every message, torn stream into `FrameReader`, snapshot size budget |
| 4 | ✅ `Fixed` arithmetic | Q16.16 ops through an intermediate `i64`, `Vec2`, `distance_squared` in raw Q32.32, tests in debug and release |
| 5 | ✅ `World` ticks | arenas, units, movement, orders, creeps, towers, Ancient; `rng.rs` with streams and Ratio/Chance |
| 6 | ✅ combat | attacks, projectiles, damage, deaths, gold/xp, victory |
| 7 | ✅ server networking | lobby, both tick modes, snapshot broadcast, replay recording |
| 8 | 🔄 `bota-bot` and `bota-client` | SDK + bot, bot-vs-bot match, self-play. Done: the client — macroquad: map, units, HP bars, orders, lobby, spectating, replay playback; the bot — lane policy over tunable numbers, lockstep acks, hill-climbing trainer |
| 9 | hero Sylla complete and determinism test | abilities, levels, items, shop; 20 000-tick hash baseline, run on musl/wasm32; mirror test: a diagonally mirrored match ends in the mirrored outcome |
