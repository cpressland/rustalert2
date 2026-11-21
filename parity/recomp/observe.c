/* rustalert2 parity observer: per game frame, what the simulation holds.
 * Written for github.com/cpressland/rustalert2's parity lab (M2); this file
 * and the two-line hook in hdvox.c are its whole footprint here.
 *
 *   RA2_OBSERVE=path        write observations (JSON lines) to path
 *   RA2_OBSERVE_EVERY=N     log every Nth frame (default every frame)
 *   RA2_OBSERVE_END=N       at game frame N, log it and exit the game
 *   RA2_OBSERVE_DUMP=N      at game frame N, also dump raw house and object
 *                           memory, and the map's per-cell zone records, to
 *                           path.dump
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
#define HOUSE_TYPE     0x34u        /* HouseTypeClass* */
#define HOUSE_BALANCE  0x30Cu       /* int: credits */
#define OBJ_HEALTH     0x6Cu        /* int */
#define OBJ_LOCATION   0x9Cu        /* CoordStruct: three ints, leptons */
#define TECHNO_OWNER   0x21Cu       /* HouseClass* */
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
        fprintf(g_out, ",\"credits\":%d}", (int32_t)rd32(h + HOUSE_BALANCE));
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
        fprintf(g_out, ",\"owner\":%u,\"x\":%d,\"y\":%d,\"z\":%d,\"hp\":%d}",
                owner ? rd32(owner + HOUSE_INDEX) : 0xFFFFFFFFu,
                (int32_t)rd32(o + OBJ_LOCATION), (int32_t)rd32(o + OBJ_LOCATION + 4),
                (int32_t)rd32(o + OBJ_LOCATION + 8), (int32_t)rd32(o + OBJ_HEALTH));
        first = 0;
    }
    fputs("]}\n", g_out);
}

static long g_every = 1;
static long g_end = -1;

void ra2_observe_tick(void) {
    if (!g_inited) {
        const char *path = getenv("RA2_OBSERVE"), *dump = getenv("RA2_OBSERVE_DUMP"), *every = getenv("RA2_OBSERVE_EVERY");
        g_inited = 1;
        if (path) g_out = fopen(path, "w");
        if (dump) g_dump_frame = strtol(dump, NULL, 0);
        if (every && strtol(every, NULL, 0) > 0) g_every = strtol(every, NULL, 0);
        if (getenv("RA2_OBSERVE_END")) g_end = strtol(getenv("RA2_OBSERVE_END"), NULL, 0);
    }
    if (!g_out) return;
    uint32_t frame = rd32(FRAME_VA);
    if (frame == g_last_frame) return;
    g_last_frame = frame;
    if (rd32(HOUSES_VA + DVC_COUNT) == 0) return;          /* not in a game yet */
    if (frame % (uint32_t)g_every == 0) write_frame(frame);
    if (g_dump_frame >= 0 && (long)frame >= g_dump_frame) {
        char name[1024];
        snprintf(name, sizeof name, "%s.dump", getenv("RA2_OBSERVE"));
        FILE *f = fopen(name, "w");
        if (f) {
            fprintf(f, "# frame %u\n", frame);
            dump_vector(f, "house", HOUSES_VA, 0x1000);
            dump_vector(f, "techno", TECHNOS_VA, 0x800);
            {
                /* The map's per-cell records (10 bytes: zone ids by level,
                   then two more words). Cell index = y * stride + x. */
                uint32_t map = 0x87F7E8u, tbl = rd32(map + 0x70), n = rd32(map + 0x6C);
                uint32_t a = rd32(map + 0xF4), b = rd32(map + 0xF8);
                fprintf(f, "# zones table %08X cells %u f4 %u f8 %u\n", tbl, n, a, b);
                for (uint32_t i = 0; i < n && tbl; i++) {
                    const uint8_t *r = GUEST_PTR(tbl + 10 * i);
                    int16_t z[5]; memcpy(z, r, 10);
                    if (z[0] || z[1] || z[2] || z[3] || z[4])
                        fprintf(f, "zone %u %d %d %d %d %d\n", i, z[0], z[1], z[2], z[3], z[4]);
                }
            }
            fclose(f);
        }
        g_dump_frame = -1;
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
