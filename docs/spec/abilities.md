# Special abilities

**Status:** Started: engineers capturing and repairing, the IFV's weapon
by passenger, Tanya's C4, Yuri's mind control, Gattling weapons, the
Cloning Vats. Traced but not modelled: the GI deploying on
Guard. Tried: the Grinder.
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

### The Cloning Vats

- With Cloning Vats (`Cloning=yes`), each infantryman the house trains
  comes with a copy, on the frame he comes out, at the centre of the cell
  holding the Vats' centre. `[observed]` (`yuri-cloning`: two Initiates,
  out on 1140 and 1249, each with a copy at (35,39) of a 2×2 Vats at
  (34,38))
- The copies then walk to a cell nearby (one south-west, one north-east);
  which isn't traced, so ours stand.

### The GI deploying (not modelled)

- Infantry not under human control, with `Deployer=yes` and
  `DeployFire=yes`, deploy to use `DeployFireWeapon=`. `[recomp
  0x00522378]` A Neutral GI on Guard hit with its M60 three times, then
  with its deployed `Para` from 117 on. `[observed]` (`guard-mcv`) The
  deploy's own timing, from the infantry's sequences, isn't traced.

### The Grinder (not working yet)

- An Initiate ordered to enter a Grinder (`MEGAMISSION`, mission Enter)
  walked onto its centre cell and stood there. It wasn't ground and paid
  nothing. `[observed]` A Grinder is fed some other way (its docking radio
  messages, likely); to trace.

## Open questions

- Spies, garrisons, the Chrono Legionnaire, the Psychic Dominator's and
  Yuri Prime's control, Boris, the
  Magnetron, Gattling weapons, the Cloning Vats, the Battle Fortress.
- Whether Tanya's C4 differs against units, and with `C4=` on other
  infantry (Navy SEALs).

## Parity

| Scenario | Checks | Tolerance |
|---|---|---|
| `allied-engineer-capture` | see repair.md | |
| `yuri-mind-control` | The Grizzly Neutral before Yuri is in range and his after; the Rhino his and the Grizzly Neutral again; both unhurt | exact at frames clear of the changes (ours come up to 21 frames sooner: infantry speed) |
| `guard-gattling` | The MCV's first damage and its health at 200, 400, 600 and 900 | exact at 200; then one pair (20), the stages coming 2 to 5 frames late |
| `yuri-cloning` | How many Initiates there are after each comes out, and where the first copy appears | exact |
| `allied-tanya-c4` | The plant destroyed, no crew, the Neutral house's power after, Tanya unhurt | the plant's end ±30 frames (infantry speed) |
