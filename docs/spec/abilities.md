# Special abilities

**Status:** Started: engineers capturing and repairing, the IFV's weapon
by passenger, the Battle Fortress's passengers, crushing infantry, Tanya's C4, Yuri's mind control, Gattling weapons, the GI
deploying, the Chrono Legionnaire, the Cloning Vats, garrisoning, spies
(disguise, dogs, barracks, power plants, refineries). Tried: the Grinder.
**Crate:** `ra2-sim` (`command`, `hunt`)
**Issue:** #44

## Scope

What particular units can do beyond moving and shooting. Repair by
engineers and the IFV is in repair.md; this note links to it.

## Sources

- The recomp of `gamemd.exe` 1.001, as cited.
- Scenarios `allied-engineer-capture`, `yuri-engineer-repair`,
  `allied-ifv-repair`, `allied-tanya-c4` and `guard-mcv`.

## Behaviour

### Engineers

- An engineer ordered onto a building walks to its nearest cell and goes
  in. Its own side's building is made whole, at no cost; an enemy's
  becomes its house's, as it stands, with both houses' power counted again
  on that frame. Either way the engineer is used up. `[observed]` (see
  repair.md, "Engineers")

### The IFV

- A gunner (`Gunner=yes`) fires `Weapon<n>`, n its first passenger's
  `IFVMode=` + 1; an engineer aboard gives the IFV `RepairBullet`. See
  repair.md, "The IFV's repair turret". `[observed]`

### The Battle Fortress

- Passengers of an open-topped transport (`OpenTopped=yes`: in Yuri's
  Revenge only the Battle Fortress) fire out of it. An attack order to the
  transport goes to them too, from the frame after it. `[observed]`
  (`allied-battle-fortress`)
- Each fires its `OpenTransportWeapon=` (0 its primary, 1 its secondary,
  else its usual weapon: a GI's is 1, the Para), from where the
  transport is. Its range has `[CombatDamage] OpenToppedRangeBonus=` (2)
  more cells and its damage is × `OpenToppedDamageMultiplier=` (1.2). It
  reloads as it would outside. `[observed]` (a GI's Para, 30, 12 a hit
  on heavy armour, every 17 to 19 frames, beside the Fortress's own
  20mmRapid, 6 a hit every 20 to 22)
- The passenger's first shot landed a frame after the Fortress's, four
  frames after the order (fitted, one recording). `[observed]`
- Only passengers that reach the target at the order take it. One out of
  reach doesn't fire at all, even once the transport has closed in.
  `[observed]` (`allied-battle-fortress-far`: the Grizzly 7.6 cells off,
  beyond the Para's 5 + 2; the Fortress drove into its own range and
  fired, the GI never did; also a recording with crushing on, 7.2 cells)
- A passenger keeps its last position while inside, until the transport
  moves; then it's carried along. `[observed]`
- A GI ordered into the Fortress didn't get in when the Fortress was
  ordered to move while the GI was on its way. `[observed]` Ours isn't
  modelled.
- The Battle Fortress is an `OmniCrusher=`. Ordered to attack a Grizzly,
  it fired as it drove up and then drove over it, killing it with 270
  health left. `[observed]` Crushing vehicles isn't modelled; the
  scenarios take crushing away.
- The Fortress came out of the war factory a cell north of ours, (46, 36)
  against our (46, 37), after first backing into the factory. `[observed]`
  The scenarios move it to a set cell first.

### Crushing

- A `Crusher=yes` vehicle that reaches the centre of a cell kills the
  `Crushable=yes` infantry anywhere in that cell, at once: they're gone
  that frame, not lying there for 15 frames as shot infantry do.
  `[observed]` (`grizzly-crush`: a Grizzly driving east. A GI in the
  middle of its cell went the frame the Grizzly reached it. One in a
  corner of the next cell went the frame the Grizzly reached that cell's
  middle, short of the GI.)
- The kill is the crusher's: the Grizzly was 0.19 a veteran after two GIs
  (2 × 200 ÷ (700 × 3)). `[observed]`
- Its own house's infantry aren't crushed. `[hypothesis]` Infantry are
  `Crushable=` unless set, other things not. `[hypothesis]`
- Infantry don't move aside: GIs on Guard, firing at the Grizzly, were
  crushed as sleeping ones were. `[observed]` (`grizzly-crush-guard`)
- An `OmniCrusher=yes` vehicle (the Battle Fortress) crushes vehicles
  the same way, unless they're `OmniCrushResistant=yes`. A crushed
  vehicle is gone that frame, leaving no wreck's blast and no crew, and
  the crusher gets the kill's experience. `[observed]`
  (`allied-battle-fortress-crush`: the Grizzly gone on the frame the
  Fortress reached the middle of its cell; the Fortress unhurt, and 0.23
  a veteran: 700 ÷ (1000 × 3))
- Ordered to attack a vehicle it can crush, an omni-crusher drives onto
  that vehicle's cell instead of stopping in range, and fires on the way.
  `[observed]` (the Fortress, 3 cells off and within its 4-cell range,
  drove the 3 cells) Ours sends any omni-crusher after a crushable
  vehicle this way, hunting as well as ordered. `[hypothesis]` The code
  that decides this isn't traced.
- On the way, the original's Fortress first fired 23 frames after the
  order; standing still, it fires after 3 (`allied-battle-fortress`).
  Ours fires at once either way. What holds it back while moving isn't
  traced.

### Tanya's C4

- A unit with `C4=yes` ordered to attack a building walks into its nearest
  cell, as an engineer does, and plants C4. The building is destroyed
  `C4Delay=` × 900 frames later, on the x87 and truncated: 0.03 gives
  26.99…, so 26. It leaves no crew. `[observed]` (`allied-tanya-c4`: Tanya
  stopped in a Neutral power plant's corner cell on 1677; the plant went on
  1703)
- Meanwhile she stands on Guard and fires her pistols at it (1 a shot on
  concrete). `[observed]`
- Her `Secondary=Sapper` (2500, `Range=1.5`) isn't what does it; we don't
  fire it. `[observed]` (the plant lost 1 hit point a shot, not 2500)

### Mind control

- A warhead with `MindControl=yes` (`Controller`: Yuri's `MindControl`,
  `Range=7`) does no damage: what it hits becomes the shooter's house's,
  unless it's a building. `[observed]` (`yuri-mind-control`: Yuri stopped
  at the first spot within 7 cells of a Neutral Grizzly, fired the frame
  after, and the Grizzly was his 5 frames later, unhurt, where it stood)
- The weapon's `Damage=` is how many it can hold (Yuri: 1); taking another
  lets the oldest go back to its house on the same frame. `[observed]`
  (Yuri then took a Neutral Rhino and the Grizzly was Neutral again) A
  dead controller's go back too. `[hypothesis]`
- What it takes keeps its mission: the sleeping Grizzly stayed asleep and
  didn't fire on the Rhino beside it. `[observed]`
- The link's flight is the 5 frames above; its projectile is invisible
  (`PsychicControl`) and its speed isn't traced.
- Yuri Prime's `SuperMindControl` (warhead `ControllerBuilding`, whose
  `Verses=` reach building armour, where Yuri's `Controller`'s are 0) takes
  buildings too: a Neutral power plant was his house's 8 frames after the
  order, unhurt, and its 200 power with it. `[observed]`
  (`yuri-prime-building`) So a building is taken when the warhead's
  `Verses=` on its armour aren't 0. `[hypothesis]`
- Deployed (`Deployer=`, `DeployFire=`), Yuri Prime fires his
  `SuperPsiWave` (`AreaFire=`) at his feet once: it goes off 8 frames after
  the order, `Damage=250` with `SuperPsiPulse` (`CellSpread=5`,
  `PercentAtMax=.85`, half on vehicles), sparing his own house
  (`AffectsAllies=no`). GIs one and two cells off died and were gone that
  frame; a Grizzly two cells off took 117. `[observed]` (`yuri-prime-wave`)
  Undeploying (`UndeployDelay=75`) isn't recorded.

### Gattling weapons

- A Gattling (`IsGattling=yes`, the Gattling Tank) keeps a rate: it
  climbs `RateUp=` (1) a frame while it has a target, up to its last
  `Stage<n>=`, and falls `RateDown=` (50) a frame while it hasn't. Past
  `Stage1=` (200) it fires its second pair of weapons, past `Stage2=`
  (400) its third: on the ground `Weapon1=`, `Weapon3=`, `Weapon5=`.
  `[observed]` (`guard-gattling`: on an MCV, 2 a hit from the first shot,
  6 from about 200 frames on, 10 from about 400; the warheads GattWH, SA,
  SSA on heavy armour)
- Each fires `Burst=2`, the two 3 to 5 frames apart, every 16 to 18
  frames (see combat.md). `[observed]`
- Its fall when it loses its target isn't recorded yet.

### Garrisons

- Infantry with `Occupier=yes` ordered onto a garrisonable building
  (`CanBeOccupied=yes`, up to `MaxNumberOccupants=`) walk to its nearest
  cell and go inside; the building is then their house's. `[observed]`
  (`allied-garrison`: a GI into a Neutral civilian house, CAHSE01)
- The order is the capture mission, as an engineer's: an Enter order on
  a Neutral civilian building is taken as an attack (the GI shot the
  house, 11 a hit). `[observed]`
- The civilian house a map gives to `Special` the original filed under
  Neutral. `[observed]`
- The garrison didn't fire by itself on a sleeping Neutral Grizzly, 3
  cells off or 6: a player's units don't take on civilians by themselves
  (combat.md). `[observed]`
- Ordered to attack, the building fires from the frame after the order.
  Its occupants take turns, the next one each shot (the second first),
  each with its `OccupyWeapon=` at `[CombatDamage] OccupyDamageMultiplier=`
  (1.2), and a kill's experience is the firing occupant's. `[observed]`
- The reload is `ROF=` plus 0 to 2, over the number of occupants (whole
  frames), over `OccupyROFMultiplier=` (1.2), truncated. `[recomp
  0x006FD15E]` The building only tries again every other frame from the
  frame after it fires, so the gap is that rounded up to an odd number.
  `[observed]` (a GI's `UCPara`, 36, 9 a hit on heavy armour: one GI 13 or
  15 frames apart, two 5 or 7, three always 5)
- A GI inside the building keeps its last position, off the map (the
  object's on-map byte, +0x74, is clear). `[observed]`
- Its range is `[CombatDamage] OccupyWeaponRange=` (5) plus half its
  foundation's shorter side, truncated, in cells, whatever its occupants'
  weapons. `[recomp 0x006F7288, 0x00458E00]` (`allied-garrison-range`:
  CAHSE01, 3x2, reaches 6 cells; a Grizzly 7 cells from its centre was
  never shot, a Rhino 5.5 off was from the frame after the order, though
  `UCPara` has `Range=6`) `[observed]`

### Spies

- A spy (`Infiltrate=yes`) ordered onto an enemy's building walks to its
  nearest cell and goes in, and is used up. `[observed]`
  (`allied-spy-barracks`) The order is the capture mission: an Enter
  order left the spy standing beside the building.
- Into a barracks (`Factory=InfantryType`): the house's infantry are
  built veterans from then on, with experience 1 from the start of their
  training; one begun before the spy went in stays a rookie. `[observed]`
- Into a war factory: its vehicles the same way. `[observed]`
  (`allied-spy-war-factory`: a Grizzly begun after the spy went into a
  Neutral war factory came out with experience 1)
- Into a power plant: its owner makes no power from that frame for
  `SpyPowerBlackout=` (1000) frames less one. `[observed]`
  (`allied-spy-power`: a Neutral plant's 200 gone from the frame the spy
  went in, back 999 frames on; the spy's own house untouched)
- Into a refinery: `SpyMoneyStealPercent=` (50%) of its owner's money,
  truncated, goes to the spy's house. `[observed]` (`allied-spy-refinery`,
  recorded, not a shipped scenario since our engine has no AI yet: the AI
  had 42574 and kept 21287; checked by a unit test)
- Into a battle lab (one of `[AI] BuildTech=`: NATECH, GATECH, YATECH):
  the spy's house gets that lab's side's tech, by the lab's
  `AIBasePlanningSide=` (0 Allied, 1 Soviet, else Third). `[recomp
  0x0045734C]` Units with `RequiresStolenAlliedTech=` (the Chrono
  Commando), `RequiresStolenSovietTech=` or `RequiresStolenThirdTech=` need
  it. `[recomp 0x004F7976]`
  - Without it, an order for one is still taken: it's built and paid for,
    then stays in the factory, held. Ordered again once the house has the
    tech, it comes out two frames after the order, and no second one is
    built. Never ordered again, it stays in. `[observed]`
    (`allied-spy-battle-lab`, and a recording without the second order)
- Other buildings (the radar) aren't modelled yet.

#### Disguise

- A spy (`CanDisguise=yes`, `PermaDisguise=yes`) shows to other houses as
  infantry of some house. It starts as its own side's default
  (`AlliedDisguise=E1`, `SovietDisguise=E2`, `ThirdDisguise=INIT`) of its
  own house. `[observed]` The game keeps it at `TechnoClass +0x1D8` (a
  flag), `+0x518` (the type) and `+0x51C` (the house). `[recomp
  0x006F4222]`
- Ordered to attack infantry, it fires its `MakeupKit` (`Range=-2`, any
  range; `FireOnce=yes`) and takes the target's type and house as its
  look, without moving. `[observed]` (`allied-spy-disguise`: from eight
  cells, two frames after the order landed)
- Units take it for one of their own house's when its look is of their
  house, and leave it alone: a Neutral GI let a spy in a Neutral GI's look
  stand beside it; in its own house's look it was shot dead.
  `[observed]` (`allied-spy-disguise`, `allied-spy-undisguised`)
- So disguised as one of the AI's Conscripts, a spy walked through the
  AI's base, past its Conscripts, flak troopers and a bunker, and into its
  refinery. `[observed]` (`allied-spy-refinery`)
- Units with `DetectDisguise=yes` (the attack dogs `DOG`, `ADOG`, `YDOG`,
  `YADOG`; the Psi Corps trooper; Yuri) see through it: a Neutral dog on
  Guard killed a spy in a Neutral GI's look as it came beside it.
  `[observed]` (`allied-spy-dog`) The dog's bite (`BadTeeth`, a
  `Parasite=yes` warhead) kills infantry outright. The original's dog
  waited until the spy was about 190 leptons off; ours bites at its
  weapon's range (1.5 cells), so sooner.

### The Chrono Legionnaire

- A teleporter (`Teleporter=yes`) doesn't walk: the frame after its order
  lands it's at its destination, on a sub-cell spot (192, 64), and it then
  waits the distance in leptons over `ChronoDistanceFactor=` (48) frames,
  at least `ChronoMinimumDelay=` (16), before it can act. `[observed]`
  (`allied-chrono-legion`: 2304 leptons, 48 frames) Sent to attack, it
  warps to its weapon's range less a cell (3 cells from a Grizzly, range
  5). `[observed]` (one case)
- Its `NeutronRifle` (warhead `ChronoBeam`, `Temporal=yes`) freezes what
  it hits, unhurt, and erases it: the target's `Strength=` × 10 to begin
  with (`[recomp 0x0071AFB2]`), less the beam's `Damage=` (8) a frame;
  at nothing it's gone, leaving neither wreck nor crew. `[observed]` (a
  Grizzly, 3000: gone 375 frames after the first hit) If the Legionnaire
  dies the erasure stops. `[hypothesis]`
- The Legionnaire comes out of the barracks onto a different spot from
  other infantry in the original ((192, 64) of the cell, where ours uses
  (64, 192)); not modelled.

### The Cloning Vats

- With Cloning Vats (`Cloning=yes`), each infantryman the house trains
  comes with a copy, on the frame he comes out, at the centre of the cell
  holding the Vats' centre. `[observed]` (`yuri-cloning`: two Initiates,
  out on 1140 and 1249, each with a copy at (35,39) of a 2×2 Vats at
  (34,38))
- The copies then walk to a cell nearby (one south-west, one north-east);
  which isn't traced, so ours stand.

### The Siege Chopper

Deploys by landing, then fires its secondary: see locomotors.md,
"Jumpjets". `[observed]` (`soviet-siege-chopper`)

### The GI deploying

- A GI (`Deployer=yes`) ordered to deploy does so where it stands, at
  once: an attack ordered two frames after hit with its deployed weapon.
  `[observed]` (`allied-gi-deploy`)
- Deployed, it fires its `Secondary=` (`Para`: 25, `ROF=15`, `SSA`): 10 a
  hit on heavy armour every 17 to 19 frames, where its M60 does 3.
  `[observed]` The elite one's `EliteSecondary=` isn't read yet.
- A computer's GI deploys by itself once it has been on Guard for
  `AIAutoDeployFrameDelay=` frames, by its house's difficulty (15 hard,
  25 medium, 100 easy). The check is in the infantry's Guard mission:
  it needs `Deployer=yes`, `DeployFire=yes`, no `DeployFireWeapon=`, an
  owner that isn't human, and the GI standing still. The delay counts
  from when its current mission began. `[recomp 0x0052150D]`
  - The civilian houses wait the easy 100 frames. `[observed]`
    (`guard-mcv`: a Neutral GI on Guard from the start hit with its M60
    on 48, 71 and 94, then with Para from 117; `grizzly-crush-guard`: a
    Neutral GI that had stood 2,000 frames hit with Para from its first
    shot) Where a house's difficulty is held (`HouseClass` + 0x184, an
    index into the rules' list) is traced; what sets it for the civilian
    houses isn't.
- Undeploying, and its delay, aren't recorded yet.

### Boris's airstrike

- Ordered to attack a building, Boris fires his `Secondary=` flare
  (`Flare`, warhead `AirstrikeFlare`, `Airstrike=yes`), though his AKM does
  as little to buildings (1%). `[observed]` (`soviet-boris-airstrike`)
  Against other targets, and how the game picks the weapon, isn't traced.
- The flare calls in `AirstrikeTeam=` (2) planes of `AirstrikeTeamType=`
  (BPLN, the MiG), `Elite…` for an elite Boris. They appear together on the
  frame after the flare, on a cell of the map's top edge, at
  `FlightLevel=` (1500), facing north. `[observed]` (six seeds)
  - The top edge is the row x + y = map width + 1 + 2 × `LocalSize=`'s top,
    across its width. `[observed]` (every plane in and out)
  - Which cell is random. A strike's first pair tended to come in near the
    target's column, later ones anywhere; ours picks uniformly along the
    edge. How the game picks isn't traced (`AirstrikeClass`, around
    `0x0041D860`).
- Each plane stands 3 frames, then gains 4 leptons a frame up to about 40
  (`Speed=16`). `[observed]`
- Each fires its weapon's burst (`Maverick3`, 750, `Burst=2`) once
  (`Ammo=1`) and flies back off the top edge, where it's removed. A pair's
  four missiles hit within 6 frames: 3000 off a wood building.
  `[observed]`
  - How they fly between (circling, overshooting before a pass) isn't
    traced: ours turn toward the target at `ROT=` × 256 a frame and fire
    within range and pointed at it. Where they come in decides when they
    hit: 1380 to 1481 over six seeds, ours 1555.
- Boris keeps firing his flare at its reload (`ROF=60` plus 0 to 2) while
  he keeps the order. A flare calls a new pair once the last plane of the
  previous one has gone and `AirstrikeRechargeTime=` (100) has passed.
  `[observed]` (the next pair came 100 to 159 frames after the last plane
  left, on a flare frame, in every recording)
- Boris fires the frame after he stops, as other infantry, so our planes
  come in a frame after the game's.

### The Magnetron

- Its beam (`MagneticBeam`, warhead `LocomotorBeam`, `IsLocomotor=yes`,
  `Locomotor=` the jumpjet's) doesn't hurt: it lifts what it hits, a
  vehicle (its `Verses=` are 0 against everything else), and carries it as
  a jumpjet, by `[JumpjetControls]`. `[observed]` (`yuri-magnetron`: a
  sleeping Neutral Grizzly 8 cells off)
  - It rises `Climb=` (5) a frame from 2 frames after the beam hits, up to
    `CruiseHeight=` (500), from where it wobbles by `WobbleDeviation=`
    (40), `WobblesPerSecond=` (0.15) times a second of 15 frames, so over
    100 frames.
  - Once higher than half the cruise height it sets off in a straight line
    toward a cell beside the Magnetron, gaining `Acceleration=` (2) a frame
    while that keeps it under `Speed=` (14): 1, 3, … 13.
  - Within about 180 leptons of the cell it slows (10, 7, 4, then 6 a
    frame) and sinks 5 a frame, then drops onto the cell's centre: 10 a
    frame, one more each frame up to 28. It sinks into the ground by its
    last step and comes back up 5 a frame. `[observed]` (the approach is
    fitted, the rest exact)
  - It isn't hurt (`FallingDamageMultiplier=`, `CurrentStrengthDamage=` did
    nothing from this height), and lands awake: the Grizzly, asleep before,
    fired back.
- Which cell: in the one lift, from due west of the Magnetron, the cell
  south-west of it; ours takes the cell 45° counter-clockwise of the way
  to the target. `[hypothesis]`
- Then too close for the beam (`MinimumRange=3`), the original's Magnetron
  backed off and lifted the Grizzly again, three times in the recording.
  Ours stands; the backing off isn't modelled.
- `MagneShake` (the Magnetron's weapon against buildings) isn't recorded.

### The Grinder (not working yet)

- An Initiate ordered to enter a Grinder (`MEGAMISSION`, mission Enter)
  walked onto its centre cell and stood there. It wasn't ground and paid
  nothing. `[observed]` The same with the capture mission (he stopped
  beside it), with the Grinder powered (its `Power=-50` drawn), and over
  1,100 frames.
- The cursor over one's own Grinder gives action 11, the service depot's,
  whose Enter order works for the depot. `[recomp 0x0051F0E3]` The
  building takes him through its radio if it has room and his `Size=` is
  within its `SizeLimit=`. `[recomp 0x0043C4F8]`
- So he docks, and the grind itself doesn't come: possibly it waits on
  the building's animation, which the headless observer may not advance.
  To trace (`Grinding=`, `BuildingType +0x16AD`, read at 0x00441283,
  0x004487E6, 0x0044E9AA, 0x00464DB4).

## Open questions

- A spy in a radar, the Magnetron's backing off and
  MagneShake.
- How airstrike planes fly and pick their way in (the flying locomotor,
  the aircraft attack mission, `AirstrikeClass`).
- What holds back a moving Battle Fortress's first shot.
- The skirmish computer's difficulty, for its GIs' deploy delay.
- Whether Tanya's C4 differs against units, and with `C4=` on other
  infantry (Navy SEALs).

## Parity

| Scenario | Checks | Tolerance |
|---|---|---|
| `allied-engineer-capture` | see repair.md | |
| `yuri-mind-control` | The Grizzly Neutral before Yuri is in range and his after; the Rhino his and the Grizzly Neutral again; both unhurt | exact at frames clear of the changes (ours come up to 21 frames sooner: infantry speed) |
| `yuri-prime-building` | The power plant his house's from the frame, unhurt, its power with it | exact |
| `yuri-prime-wave` | Both GIs gone the frame the wave lands, the Grizzly's damage, Yuri Prime unhurt | exact |
| `guard-gattling` | The MCV's first damage and its health at 200, 400, 600 and 900 | exact at 200; then one pair (20), the stages coming 2 to 5 frames late |
| `allied-garrison` | The house Neutral, then the player's; unhurt; the GI still counted | exact, clear of the change (ours 30 frames sooner: infantry speed) |
| `allied-garrison-range` | The Grizzly out of range unhurt; the Rhino's first damage and its health | exact; the health ±9 (one hit) |
| `allied-spy-barracks` | The spy going in; the GI trained after a veteran | exact; the spy ±40 frames (infantry speed) |
| `allied-spy-war-factory` | The spy going in; the Grizzly built after a veteran | exact; the spy ±40 frames (infantry speed) |
| `allied-spy-battle-lab` | The spy going in; the Chrono Commando ordered before built, paid for and held; out on the map two frames after the second order, and only one | exact; the spy ±40 frames (infantry speed) |
| `soviet-boris-airstrike` | The MiGs coming in, two of them; one run's damage done by 1600 and three by 2350 | ±1 frame on the planes; the damage exact (true of six seeds; when each run hits isn't checked) |
| `yuri-magnetron` | The Grizzly lifted (exact at 1850), carried (within 10 and 20 leptons), dropped on the cell, unhurt; one hit back on the Magnetron | exact clear of the approach |
| `allied-spy-disguise` | The spy's look before and after the MakeupKit, that it didn't move, and that it survives beside the Neutral GI | exact |
| `allied-spy-undisguised` | The spy alive before the GI, dead after | exact clear of the timing (the original's GI deployed) |
| `allied-spy-dog` | The spy's look, alive before the dog, dead after; the dog unhurt | exact clear of the timing |
| `allied-spy-power` | The spy going in; the Neutral house's power during and after the blackout, the player's untouched | exact clear of the changes; the spy ±40 frames |
| `allied-garrison-fire` | The house's first hit, the Grizzly's health as it fires, and the Grizzly gone | first hit exact; health one hit (9) |
| `allied-garrison-two` | As above with two GIs inside, and the kill's experience | first hit exact; health one hit (9) |
| `allied-garrison-three` | As above with three GIs inside | exact |
| `grizzly-crush` | Both GIs there, the first one's end, each gone in turn, and the Grizzly's experience | removal ±4 frames (drive start-up, #40); experience exact |
| `grizzly-crush-guard` | The GI's health as the Grizzly fires back, the Grizzly's health, each GI's end, the gap between them | health one hit (16); removal ±5 frames (drive start-up) |
| `allied-battle-fortress-crush` | The Grizzly there before the order, gone after; the Fortress's experience and health after | removal ±10 frames (drive start-up and turns, #40); the rest exact |
| `allied-battle-fortress-far` | The Fortress's first hit and the Grizzly's health with only the Fortress firing | first hit ±2 frames; health one hit (6) |
| `allied-battle-fortress` | The Grizzly's first hit and health as the Fortress and the GI inside fire, the Grizzly gone, the GI alive | first hit ±1 frame (the Fortress's); health ±18 (a hit of each) |
| `yuri-cloning` | How many Initiates there are after each comes out, and where the first copy appears | exact |
| `allied-gi-deploy` | The Grizzly untouched before the attack order, the deployed GI's first hit and the Grizzly's health after, the GI unmoved | first hit ±2 frames; health one hit (10) |
| `allied-chrono-legion` | Where it warps to attack and to move, the Grizzly untouched while frozen, and when it's gone | positions exact; the erasure ±5 frames |
| `allied-tanya-c4` | The plant destroyed, no crew, the Neutral house's power after, Tanya unhurt | the plant's end ±30 frames (infantry speed) |
