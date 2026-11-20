# <System name>

> Copy this file to `docs/spec/<system>.md` and add it to the index in
> [README.md](README.md).

**Status:** Not started | Spec'd | Implemented | Parity-checked
**Crate:** `ra2-…`
**Issue:** #…

## Scope

What this note covers, and what it leaves to other notes.

## Sources

- The recomp build the addresses refer to (`gamemd.exe` 1.001 unless stated).
- Community pages used (link them).
- How observations were made (scenario, map, settings).

## Behaviour

Plain-language claims, one idea each, with an evidence tag:

- A unit's `Speed=` is in leptons per frame … `[recomp 0x00000000 FootClass::…]`
- On low power, production runs at … `[observed]` (scenario `power-low`)
- The pathfinder gives up after … `[hypothesis]`

Use tables for value mappings, and pseudocode in our own words for
algorithms. Never paste lifted C or disassembly.

## Quirks we match

Behaviour that looks like a bug but that players rely on, and our decision
on each one.

## Open questions

What we don't know yet, and how we plan to find out.

## Parity

The scenarios that cover this system, and their tolerances.

| Scenario | Checks | Tolerance |
|---|---|---|
| `…` | … | … |
