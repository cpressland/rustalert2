/* rustalert2 parity observer: per game frame, what the simulation holds.
 * Written for github.com/cpressland/rustalert2's parity lab (M2); this file
 * and the two-line hook in hdvox.c are its whole footprint here.
 *
 *   RA2_OBSERVE=path        write observations (JSON lines) to path
 *   RA2_OBSERVE_EVERY=N     log every Nth frame (default every frame)
 *   RA2_OBSERVE_END=N       at game frame N, log it and exit the game
 *   RA2_OBSERVE_DUMP=N      at game frame N, also dump raw house and object
 *                           memory to path.dump, to find field offsets
 */
#include <stdint.h>
#include <stdio.h>
#include <stdlib.h>
#include <string.h>

#ifdef _WIN32
#include <windows.h>
#define GUEST_PTR(va) ((void *)(uintptr_t)(uint32_t)(va))
#else
#include "win32hle.h"
#define GUEST_PTR(va) hle_ptr((uint32_t)(va))
#endif

#define FRAME_VA       0x00A8ED84u   /* the game frame counter */
#define HOUSES_VA      0x00A80228u   /* DynamicVectorClass<HouseClass*> */
#define TECHNOS_VA     0x00A8EC78u   /* DynamicVectorClass<TechnoClass*> */
#define DVC_ITEMS      0x04u
#define DVC_COUNT      0x10u

static uint32_t rd32(uint32_t va) { uint32_t v; memcpy(&v, GUEST_PTR(va), 4); return v; }

/* The RTTI name of an object's class: vtable -> COL -> type descriptor. */
static const char *class_name(uint32_t obj) {
    uint32_t vt = rd32(obj);
    if (vt < 0x401000u || vt > 0x7FFFFFu) return "?";
    uint32_t col = rd32(vt - 4);
    if (col < 0x401000u || col > 0x8FFFFFu) return "?";
    uint32_t td = rd32(col + 12);
    if (td < 0x401000u || td > 0x8FFFFFu) return "?";
    return (const char *)GUEST_PTR(td + 8);
}

static FILE *g_out;
static long g_dump_frame = -1;
static long g_dump_every = 0;
static int g_inited;
static uint32_t g_last_frame = 0xFFFFFFFFu;

static void dump_vector(FILE *f, const char *what, uint32_t vec, uint32_t bytes) {
    uint32_t items = rd32(vec + DVC_ITEMS), count = rd32(vec + DVC_COUNT);
    fprintf(f, "# %s count %u\n", what, count);
    for (uint32_t i = 0; i < count && i < 4096; i++) {
        uint32_t obj = rd32(items + 4 * i);
        fprintf(f, "%s %u %08X %s\n", what, i, obj, class_name(obj));
        const uint8_t *p = GUEST_PTR(obj);
        for (uint32_t k = 0; k < bytes; k += 4) {
            uint32_t v; memcpy(&v, p + k, 4);
            fprintf(f, "%08X%c", v, (k / 4) % 16 == 15 ? '\n' : ' ');
        }
    }
}

/* Field offsets, found from a memory dump of a running skirmish (see
 * rustalert2's docs/spec/parity.md) and matching the community's YRpp. */
#define HOUSE_INDEX    0x30u        /* int */
#define HOUSE_POWER_OUT   0x53A4u   /* int: power produced (0x004FCE30) */
#define HOUSE_POWER_DRAIN 0x53A8u   /* int: power drawn */
#define HOUSE_SUPERS   0x254u       /* DynamicVectorClass<SuperClass*>, one per type */
#define SUPER_TIMER    0x30u        /* CDTimer: start (-1 when stopped), ?, frames */
#define SUPER_PRESENT  0x6Du        /* byte: the house has it */
#define SUPER_CHARGED  0x6Fu        /* byte: ready to fire */
#define SUPER_ON_HOLD  0x70u        /* byte: charging held (low power) */
#define HOUSE_TYPE     0x34u        /* HouseTypeClass* */
#define HOUSE_BALANCE  0x30Cu       /* int: credits */
#define OBJ_HEALTH     0x6Cu        /* int */
#define OBJ_LOCATION   0x9Cu        /* CoordStruct: three ints, leptons */
#define TECHNO_OWNER   0x21Cu       /* HouseClass* */
#define TECHNO_STORAGE 0x33Cu       /* float[4], bails of each ore kind */
#define MAP_CELLS      (0x87F7E8u + 0x13Cu) /* CellClass*[512 * 512] */
#define CELL_OVERLAY   0x44u        /* int, -1 for none */
#define CELL_OVERLAY_DATA 0x11Eu    /* byte: ore stage */
#define TYPE_ID        0x24u        /* AbstractTypeClass: char[25] rules ID */

static void json_id(FILE *f, uint32_t type) {
    char id[26];
    if (!type) { fputs("\"\"", f); return; }
    memcpy(id, GUEST_PTR(type + TYPE_ID), 25); id[25] = 0;
    fputc('"', f);
    for (char *c = id; *c; c++) if (*c >= 0x20 && *c != '"' && *c != '\\' && *c < 0x7f) fputc(*c, f);
    fputc('"', f);
}

/* Per class: its kind's name in the log, and where its type pointer is. */
static int kind_of(const char *cls, const char **kind, uint32_t *type_off) {
    if (!strcmp(cls, ".?AVBuildingClass@@")) { *kind = "Building"; *type_off = 0x520u; return 1; }
    if (!strcmp(cls, ".?AVUnitClass@@"))     { *kind = "Unit";     *type_off = 0x6C4u; return 1; }
    if (!strcmp(cls, ".?AVInfantryClass@@")) { *kind = "Infantry"; *type_off = 0x6C0u; return 1; }
    if (!strcmp(cls, ".?AVAircraftClass@@")) { *kind = "Aircraft"; *type_off = 0x6C4u; return 1; }
    return 0;
}

static void write_frame(uint32_t frame) {
    uint32_t hv = rd32(HOUSES_VA + DVC_ITEMS), hn = rd32(HOUSES_VA + DVC_COUNT);
    uint32_t tv = rd32(TECHNOS_VA + DVC_ITEMS), tn = rd32(TECHNOS_VA + DVC_COUNT);
    fprintf(g_out, "{\"frame\":%u,\"houses\":[", frame);
    for (uint32_t i = 0; i < hn && i < 256; i++) {
        uint32_t h = rd32(hv + 4 * i);
        fprintf(g_out, "%s{\"index\":%u,\"country\":", i ? "," : "", rd32(h + HOUSE_INDEX));
        json_id(g_out, rd32(h + HOUSE_TYPE));
        fprintf(g_out, ",\"credits\":%d,\"power_output\":%d,\"power_drain\":%d",
                (int32_t)rd32(h + HOUSE_BALANCE), (int32_t)rd32(h + HOUSE_POWER_OUT),
                (int32_t)rd32(h + HOUSE_POWER_DRAIN));
        {
            /* Superweapons the house has: type index, charged, on hold,
               frames left to charge. */
            uint32_t items = rd32(h + HOUSE_SUPERS + DVC_ITEMS), n = rd32(h + HOUSE_SUPERS + DVC_COUNT);
            int any = 0;
            for (uint32_t k = 0; items && k < n && k < 64; k++) {
                uint32_t sw = rd32(items + 4 * k);
                if (!*(const uint8_t *)GUEST_PTR(sw + SUPER_PRESENT)) continue;
                int32_t start = (int32_t)rd32(sw + SUPER_TIMER), dur = (int32_t)rd32(sw + SUPER_TIMER + 8);
                int32_t left = start == -1 ? dur : dur - ((int32_t)frame - start);
                if (left < 0) left = 0;
                fprintf(g_out, "%s[%u,%u,%u,%d]", any ? "," : ",\"supers\":[", k,
                        *(const uint8_t *)GUEST_PTR(sw + SUPER_CHARGED),
                        *(const uint8_t *)GUEST_PTR(sw + SUPER_ON_HOLD), left);
                any = 1;
            }
            if (any) fputs("]", g_out);
        }
        fputs("}", g_out);
    }
    fputs("],\"objects\":[", g_out);
    int first = 1;
    for (uint32_t i = 0; i < tn && i < 65536; i++) {
        uint32_t o = rd32(tv + 4 * i);
        const char *kind; uint32_t type_off;
        if (!kind_of(class_name(o), &kind, &type_off)) continue;
        uint32_t owner = rd32(o + TECHNO_OWNER);
        fprintf(g_out, "%s{\"id\":%u,\"kind\":\"%s\",\"type\":", first ? "" : ",", o, kind);
        json_id(g_out, rd32(o + type_off));
        fprintf(g_out, ",\"owner\":%u,\"x\":%d,\"y\":%d,\"z\":%d,\"hp\":%d",
                owner ? rd32(owner + HOUSE_INDEX) : 0xFFFFFFFFu,
                (int32_t)rd32(o + OBJ_LOCATION), (int32_t)rd32(o + OBJ_LOCATION + 4),
                (int32_t)rd32(o + OBJ_LOCATION + 8), (int32_t)rd32(o + OBJ_HEALTH));
        {
            /* Cargo: the four ore kinds a techno carries, floats in bails. */
            float sum = 0;
            for (int k = 0; k < 4; k++) {
                uint32_t bits = rd32(o + TECHNO_STORAGE + 4u * (uint32_t)k);
                float v; memcpy(&v, &bits, 4); sum += v;
            }
            if (sum != 0) fprintf(g_out, ",\"cargo\":%d", (int)sum);
        }
        fputs("}", g_out);
        first = 0;
    }
    fputs("],\"ore\":[", g_out);
    {
        /* Every cell holding ore or gems: x, y, overlay, its data. */
        uint32_t arr = rd32(MAP_CELLS);
        int any = 0;
        for (uint32_t idx = 0; arr && idx < 0x40000u; idx++) {
            uint32_t c = rd32(arr + 4u * idx);
            if (!c) continue;
            int32_t ov = (int32_t)rd32(c + CELL_OVERLAY);
            if (!((ov >= 27 && ov <= 38) || (ov >= 102 && ov <= 121) || (ov >= 127 && ov <= 166))) continue;
            fprintf(g_out, "%s[%u,%u,%d,%u]", any ? "," : "", idx % 512, idx / 512, ov,
                    *(const uint8_t *)GUEST_PTR(c + CELL_OVERLAY_DATA));
            any = 1;
        }
    }
    fputs("]}\n", g_out);
}

/* Orders: RA2_OBSERVE_ORDERS is a list of `frame kind args...`, separated
   by `;`. Each becomes the event the human player's click would queue
   (EventClass::OutList, 0xA802C8: count, head, tail, then 128 events of
   0x6F bytes; send times at 0xA83A54), for house 0:
     D <type>                     deploy house 0's first object of <type>
     S <type>                     sell it
     P <rtti> <index> <naval>     produce
     A <rtti> <index> <naval>     abandon (cancel) that production
     H <rtti> <index> <naval>     hold (suspend) it; produce again resumes
     F <super> <x> <y>            fire superweapon <super> (its type's
                                  index) at a cell: SPECIAL_PLACE, layout
                                  from 0x004C6B60
     L <rtti> <index> <naval> <x> <y>   place at a cell
   Layout from the constructors at 0x004C65E0, 0x004C6970, 0x004C6AE0. */
#define OUTLIST        0xA802C8u
#define EVENT_SIZE     0x6Fu
static void queue_event(const uint8_t *ev) {
    if ((int32_t)rd32(OUTLIST) >= 128) return;
    uint32_t tail = rd32(OUTLIST + 8);
    memcpy(GUEST_PTR(OUTLIST + 0xC + tail * EVENT_SIZE), ev, EVENT_SIZE);
    *(uint32_t *)GUEST_PTR(0xA83A54u + 4u * tail) = 0;
    *(uint32_t *)GUEST_PTR(OUTLIST + 8) = (tail + 1) & 0x7F;
    *(uint32_t *)GUEST_PTR(OUTLIST) = rd32(OUTLIST) + 1;
}

static void run_orders(uint32_t frame) {
    const char *spec = getenv("RA2_OBSERVE_ORDERS");
    if (!spec) return;
    char buf[4096];
    snprintf(buf, sizeof buf, "%s", spec);
    for (char *save = NULL, *o = strtok_r(buf, ";", &save); o; o = strtok_r(NULL, ";", &save)) {
        unsigned at; char kind; char arg[64];
        if (sscanf(o, "%u %c %63s", &at, &kind, arg) != 3 || at != frame) continue;
        uint8_t ev[0x6F]; memset(ev, 0, sizeof ev);
        ev[2] = 0; /* house 0 */
        uint32_t now = frame;
        memcpy(ev + 3, &now, 4);
        if (kind == 'D' || kind == 'S') {
            uint32_t tv = rd32(TECHNOS_VA + DVC_ITEMS), tn = rd32(TECHNOS_VA + DVC_COUNT);
            for (uint32_t i = 0; i < tn; i++) {
                uint32_t t = rd32(tv + 4 * i), owner = rd32(t + TECHNO_OWNER);
                const char *kname; uint32_t type_off;
                if (!owner || rd32(owner + HOUSE_INDEX) != 0) continue;
                if (!kind_of(class_name(t), &kname, &type_off)) continue;
                if (strcmp((const char *)GUEST_PTR(rd32(t + type_off) + 0x24), arg)) continue;
                uint32_t id = rd32(t + 0x10);   /* AbstractClass::UniqueID */
                ev[0] = kind == 'D' ? 9 : 22;   /* DEPLOY, SELL */
                memcpy(ev + 7, &id, 4);
                ev[0xB] = 0x34;                 /* a target that is an object */
                queue_event(ev);
                fprintf(g_out, "# order %u %s %s id %u\n", frame, kind == 'D' ? "deploy" : "sell", arg, id);
                break;
            }
        } else if (kind == 'F') {
            int sw, x, y;
            if (sscanf(o, "%u %c %d %d %d", &at, &kind, &sw, &x, &y) != 5) continue;
            int16_t cx = (int16_t)x, cy = (int16_t)y;
            ev[0] = 18;                         /* SPECIAL_PLACE */
            memcpy(ev + 7, &sw, 4);
            memcpy(ev + 0xB, &cx, 2);
            memcpy(ev + 0xD, &cy, 2);
            queue_event(ev);
            fprintf(g_out, "# order %u fire %d at %d,%d\n", frame, sw, x, y);
        } else if (kind == 'P' || kind == 'L' || kind == 'A' || kind == 'H') {
            int rtti, index, naval, x = 0, y = 0;
            int n = sscanf(o, "%u %c %d %d %d %d %d", &at, &kind, &rtti, &index, &naval, &x, &y);
            if (n < 5) continue;
            ev[0] = kind == 'P' ? 14 : kind == 'A' ? 16 : kind == 'H' ? 15 : 11; /* PRODUCE, ABANDON, SUSPEND, PLACE */
            memcpy(ev + 7, &rtti, 4);
            memcpy(ev + 0xB, &index, 4);
            memcpy(ev + 0xF, &naval, 4);
            if (kind == 'L') {
                int16_t cx = (int16_t)x, cy = (int16_t)y;
                memcpy(ev + 0x13, &cx, 2);
                memcpy(ev + 0x15, &cy, 2);
            }
            queue_event(ev);
            fprintf(g_out, "# order %u %s %d %d\n", frame, kind == 'P' ? "produce" : kind == 'A' ? "abandon" : kind == 'H' ? "hold" : "place", rtti, index);
        }
    }
}

static long g_every = 1;
static long g_end = -1;

void ra2_observe_tick(void) {
    if (!g_inited) {
        const char *path = getenv("RA2_OBSERVE"), *dump = getenv("RA2_OBSERVE_DUMP"), *every = getenv("RA2_OBSERVE_EVERY");
        g_inited = 1;
        if (path) g_out = fopen(path, "w");
        if (dump) g_dump_frame = strtol(dump, NULL, 0);
        if (getenv("RA2_OBSERVE_DUMP_EVERY")) g_dump_every = strtol(getenv("RA2_OBSERVE_DUMP_EVERY"), NULL, 0);
        if (every && strtol(every, NULL, 0) > 0) g_every = strtol(every, NULL, 0);
        if (getenv("RA2_OBSERVE_END")) g_end = strtol(getenv("RA2_OBSERVE_END"), NULL, 0);
    }
    if (!g_out) return;
    uint32_t frame = rd32(FRAME_VA);
    if (frame == g_last_frame) return;
    g_last_frame = frame;
    if (rd32(HOUSES_VA + DVC_COUNT) == 0) return;          /* not in a game yet */
    if (frame % (uint32_t)g_every == 0) write_frame(frame);
    run_orders(frame);
    if (g_dump_frame >= 0 && (long)frame >= g_dump_frame) {
        char name[1024];
        if (g_dump_every > 0)
            snprintf(name, sizeof name, "%s.dump.%u", getenv("RA2_OBSERVE"), frame);
        else
            snprintf(name, sizeof name, "%s.dump", getenv("RA2_OBSERVE"));
        FILE *f = fopen(name, "w");
        if (f) {
            fprintf(f, "# frame %u\n", frame);
            dump_vector(f, "house", HOUSES_VA, 0x1000);
            {
                /* Where a house keeps its superweapons: a vector whose
                   items are SuperClass (vtable 0x7F3FE8). */
                uint32_t hv = rd32(HOUSES_VA + DVC_ITEMS), hn = rd32(HOUSES_VA + DVC_COUNT);
                for (uint32_t i = 0; i < hn && i < 8; i++) {
                    uint32_t h = rd32(hv + 4 * i);
                    for (uint32_t off = 0; off + 0x14 < 0x5800; off += 4) {
                        /* DynamicVectorClass: vtable, items, capacity,
                           flag, count. Only follow heap pointers. */
                        uint32_t vt = rd32(h + off), items = rd32(h + off + 4);
                        uint32_t cnt = rd32(h + off + 0x10);
                        if (vt < 0x7E0000u || vt > 0x800000u) continue;
                        if (items < 0x10000000u || items > 0x30000000u) continue;
                        if (cnt == 0 || cnt > 64) continue;
                        uint32_t first = rd32(items);
                        if (first < 0x10000000u || first > 0x30000000u) continue;
                        if (rd32(first) != 0x7F3FE8u) continue;
                        uint32_t count = cnt;
                        fprintf(f, "supers house %u at +%X count %u\n", i, off, count);
                        for (uint32_t k = 0; k < count && k < 16; k++) {
                            uint32_t sw = rd32(items + 4 * k);
                            fprintf(f, "super %u %u", i, k);
                            for (uint32_t w = 0; w < 0x80; w += 4) fprintf(f, " %08X", rd32(sw + w));
                            fprintf(f, "\n");
                        }
                    }
                }
            }
            dump_vector(f, "techno", TECHNOS_VA, 0x800);
            {
                /* The map's per-cell records (10 bytes: zone ids by level,
                   then two more words). Cell index = y * stride + x. */
                uint32_t map = 0x87F7E8u, tbl = rd32(map + 0x70), n = rd32(map + 0x6C);
                uint32_t a = rd32(map + 0xF4), b = rd32(map + 0xF8);
                fprintf(f, "# zones table %08X cells %u f4 %u f8 %u\n", tbl, n, a, b);
                {
                    /* The pathfinder (0x87E8B8): the latest search's stamp,
                       the zones it stamped at each level (the corridor),
                       and the cells its cell search reached, with g. */
                    uint32_t pf = 0x87E8B8u, stamp = rd32(pf + 0x28);
                    int maxz[3] = {0, 0, 0};
                    for (uint32_t i = 0; i < n && tbl; i++) {
                        int16_t z[3]; memcpy(z, GUEST_PTR(tbl + 10 * i), 6);
                        for (int l = 0; l < 3; l++) if (z[l] > maxz[l]) maxz[l] = z[l];
                    }
                    fprintf(f, "# pathfinder stamp %u\n", stamp);
                    for (int l = 0; l < 3; l++) {
                        uint32_t arr = rd32(pf + 0x40 + 4 * l);
                        if (!arr) continue;
                        fprintf(f, "corridor %d", l);
                        for (int zid = 0; zid <= maxz[l]; zid++)
                            if (rd32(arr + 4 * zid) == stamp) fprintf(f, " %d", zid);
                        fprintf(f, "\n");
                    }
                    uint32_t seen = rd32(pf + 0x18), cost = rd32(pf + 0x24);
                    for (uint32_t i = 0; i < n && seen; i++)
                        if (rd32(seen + 4 * i) == stamp)
                            fprintf(f, "reached %u %08X\n", i, cost ? rd32(cost + 4 * i) : 0);
                }
                for (uint32_t i = 0; i < n && tbl; i++) {
                    const uint8_t *r = GUEST_PTR(tbl + 10 * i);
                    int16_t z[5]; memcpy(z, r, 10);
                    if (z[0] || z[1] || z[2] || z[3] || z[4])
                        fprintf(f, "zone %u %d %d %d %d %d\n", i, z[0], z[1], z[2], z[3], z[4]);
                }
            }
            {
                /* RA2_OBSERVE_CELLS=x0,y0,x1,y1: raw CellClass bytes. */
                const char *cs = getenv("RA2_OBSERVE_CELLS");
                int x0, y0, x1, y1;
                if (cs && sscanf(cs, "%d,%d,%d,%d", &x0, &y0, &x1, &y1) == 4) {
                    uint32_t arr = rd32(MAP_CELLS);
                    for (int y = y0; y <= y1; y++)
                        for (int x = x0; x <= x1; x++) {
                            uint32_t c = rd32(arr + 4u * (uint32_t)(y * 512 + x));
                            if (!c) continue;
                            fprintf(f, "cell %d %d", x, y);
                            const uint8_t *b = GUEST_PTR(c);
                            for (int k = 0; k < 0x180; k++) fprintf(f, " %02X", b[k]);
                            fprintf(f, "\n");
                        }
                }
            }
            fclose(f);
        }
        g_dump_frame = g_dump_every > 0 ? (long)frame + g_dump_every : -1;
    }
    fflush(g_out);
    if (g_end >= 0 && (long)frame >= g_end) {
        /* The scenario is over: always log its last frame, then leave. */
        if (frame % (uint32_t)g_every != 0) write_frame(frame);
        fprintf(g_out, "# end at frame %u\n", frame);
        fclose(g_out);
        exit(0);
    }
}
