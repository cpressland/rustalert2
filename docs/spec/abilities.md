# Special abilities

**Status:** Started: engineers capturing and repairing, the IFV's weapon
by passenger, Tanya's C4, Yuri's mind control, Gattling weapons, the GI
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
- Ordered to attack, the building fires its first occupant's
  `OccupyWeapon=` from the frame after the order, its damage ×
  `[CombatDamage] OccupyDamageMultiplier=` (1.2) and its reload (`ROF=`
  plus 0 to 2) ÷ `OccupyROFMultiplier=` (1.2), truncated. `[observed]`
  (`allied-garrison-fire`: a GI's UCPara, 36, 9 a hit on heavy armour,
  every 13 to 15 frames) More occupants firing together, and
  `OccupyWeaponRange=`, aren't recorded yet.

### Spies

- A spy (`Infiltrate=yes`) ordered onto an enemy's building walks to its
  nearest cell and goes in, and is used up. `[observed]`
  (`allied-spy-barracks`) The order is the capture mission: an Enter
  order left the spy standing beside the building.
- Into a barracks (`Factory=InfantryType`): the house's infantry are
  built veterans from then on, with experience 1 from the start of their
  training; one begun before the spy went in stays a rookie. `[observed]`
- Into a war factory: its vehicles the same way. `[hypothesis]`
- Into a power plant: its owner makes no power from that frame for
  `SpyPowerBlackout=` (1000) frames less one. `[observed]`
  (`allied-spy-power`: a Neutral plant's 200 gone from the frame the spy
  went in, back 999 frames on; the spy's own house untouched)
- Into a refinery: `SpyMoneyStealPercent=` (50%) of its owner's money,
  truncated, goes to the spy's house. `[observed]` (`allied-spy-refinery`,
  recorded, not a shipped scenario since our engine has no AI yet: the AI
  had 42574 and kept 21287; checked by a unit test)
- Other buildings (the radar, the battle lab) aren't modelled yet.

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

### The GI deploying

- A GI (`Deployer=yes`) ordered to deploy does so where it stands, at
  once: an attack ordered two frames after hit with its deployed weapon.
  `[observed]` (`allied-gi-deploy`)
- Deployed, it fires its `Secondary=` (`Para`: 25, `ROF=15`, `SSA`): 10 a
  hit on heavy armour every 17 to 19 frames, where its M60 does 3.
  `[observed]` The elite one's `EliteSecondary=` isn't read yet.
- Infantry not under a player's control with `DeployFire=yes` deploy by
  themselves to use it (`[recomp 0x00522378]`): a Neutral GI on Guard hit
  with its M60 three times, then with Para from the next shot. We deploy
  it after its third shot; what the game waits for isn't traced.
  `[observed]` (`guard-mcv`)
- Undeploying, and its delay, aren't recorded yet.

### The Grinder (not working yet)

- An Initiate ordered to enter a Grinder (`MEGAMISSION`, mission Enter)
  walked onto its centre cell and stood there. It wasn't ground and paid
  nothing. `[observed]` A Grinder is fed some other way (its docking radio
  messages, likely); to trace.

## Open questions

- Spies in other buildings, garrisons of several, the Psychic
  Dominator's and Yuri Prime's control, Boris, the Magnetron, the Battle
  Fortress.
- Whether Tanya's C4 differs against units, and with `C4=` on other
  infantry (Navy SEALs).

## Parity

| Scenario | Checks | Tolerance |
|---|---|---|
| `allied-engineer-capture` | see repair.md | |
| `yuri-mind-control` | The Grizzly Neutral before Yuri is in range and his after; the Rhino his and the Grizzly Neutral again; both unhurt | exact at frames clear of the changes (ours come up to 21 frames sooner: infantry speed) |
| `guard-gattling` | The MCV's first damage and its health at 200, 400, 600 and 900 | exact at 200; then one pair (20), the stages coming 2 to 5 frames late |
| `allied-garrison` | The house Neutral, then the player's; unhurt; the GI still counted | exact, clear of the change (ours 30 frames sooner: infantry speed) |
| `allied-spy-barracks` | The spy going in; the GI trained after a veteran | exact; the spy ±40 frames (infantry speed) |
| `allied-spy-disguise` | The spy's look before and after the MakeupKit, that it didn't move, and that it survives beside the Neutral GI | exact |
| `allied-spy-undisguised` | The spy alive before the GI, dead after | exact clear of the timing (the original's GI deployed) |
| `allied-spy-dog` | The spy's look, alive before the dog, dead after; the dog unhurt | exact clear of the timing |
| `allied-spy-power` | The spy going in; the Neutral house's power during and after the blackout, the player's untouched | exact clear of the changes; the spy ±40 frames |
| `allied-garrison-fire` | The house's first hit, the Grizzly's health as it fires, and the Grizzly gone | first hit exact; health one hit (9) |
| `yuri-cloning` | How many Initiates there are after each comes out, and where the first copy appears | exact |
| `allied-gi-deploy` | The Grizzly untouched before the attack order, the deployed GI's first hit and the Grizzly's health after, the GI unmoved | first hit ±2 frames; health one hit (10) |
| `allied-chrono-legion` | Where it warps to attack and to move, the Grizzly untouched while frozen, and when it's gone | positions exact; the erasure ±5 frames |
| `allied-tanya-c4` | The plant destroyed, no crew, the Neutral house's power after, Tanya unhurt | the plant's end ±30 frames (infantry speed) |
