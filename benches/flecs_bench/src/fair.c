#include <flecs.h>
#include <stdio.h>
#include <stdlib.h>
#include <string.h>
#include <stdbool.h>
#include <stdint.h>
#include <math.h>

#ifdef _WIN32
#include <windows.h>
static double now_ms(void) {
    LARGE_INTEGER t, f;
    QueryPerformanceCounter(&t);
    QueryPerformanceFrequency(&f);
    return ((double)t.QuadPart * 1000.0) / (double)f.QuadPart;
}
#else
#include <time.h>
static double now_ms(void) {
    struct timespec ts;
    clock_gettime(CLOCK_MONOTONIC, &ts);
    return ((double)ts.tv_sec * 1000.0) + ((double)ts.tv_nsec * 1e-6);
}
#endif

typedef struct { float x, y, z, w; } C0;
typedef struct { float x, y, z, w; } C1;
typedef struct { float x, y, z, w; } C2;
typedef struct { float x, y, z, w; } C3;
typedef struct { float x, y, z, w; } C4;
typedef struct { float x, y, z, w; } C5;
typedef struct { float x, y, z, w; } C6;
typedef struct { float x, y, z, w; } C7;

ECS_COMPONENT_DECLARE(C0); ECS_COMPONENT_DECLARE(C1);
ECS_COMPONENT_DECLARE(C2); ECS_COMPONENT_DECLARE(C3);
ECS_COMPONENT_DECLARE(C4); ECS_COMPONENT_DECLARE(C5);
ECS_COMPONENT_DECLARE(C6); ECS_COMPONENT_DECLARE(C7);

static ecs_entity_t T[9];
static ecs_entity_t TagId;

static ecs_world_t* make_world(void) {
    ecs_world_t *w = ecs_init();
    ECS_COMPONENT_DEFINE(w, C0); ECS_COMPONENT_DEFINE(w, C1);
    ECS_COMPONENT_DEFINE(w, C2); ECS_COMPONENT_DEFINE(w, C3);
    ECS_COMPONENT_DEFINE(w, C4); ECS_COMPONENT_DEFINE(w, C5);
    ECS_COMPONENT_DEFINE(w, C6); ECS_COMPONENT_DEFINE(w, C7);
    for (int i = 0; i < 9; i++) {
        char name[16];
        snprintf(name, sizeof name, "T%d", i);
        T[i] = ecs_entity(w, { .name = name });
    }
    TagId = ecs_entity(w, { .name = "Tag" });
    return w;
}

static void shuffled_indices(int *v, int n) {
    for (int i = 0; i < n; i++) v[i] = i;
    uint64_t state = 0x2545F4914F6CDD1DULL;
    for (int i = n - 1; i >= 1; i--) {
        state ^= state << 13;
        state ^= state >> 7;
        state ^= state << 17;
        int j = (int)(state % (uint64_t)(i + 1));
        int tmp = v[i]; v[i] = v[j]; v[j] = tmp;
    }
}

static uint64_t f64_bits(double d) {
    uint64_t u;
    memcpy(&u, &d, sizeof u);
    return u;
}

static uint64_t checksum_c0(ecs_world_t *w) {
    ecs_query_t *q = ecs_query(w, { .terms = {{ .id = ecs_id(C0) }} });
    double sum = 0.0;
    ecs_iter_t it = ecs_query_iter(w, q);
    while (ecs_query_next(&it)) {
        C0 *c = ecs_field(&it, C0, 0);
        for (int i = 0; i < it.count; i++) sum += (double)c[i].x;
    }
    ecs_query_fini(q);
    return f64_bits(sum);
}

static uint64_t live_entities(ecs_world_t *w) {

    ecs_query_t *q = ecs_query(w, { .terms = {{ .id = ecs_id(C0) }} });
    uint64_t n = 0;
    ecs_iter_t it = ecs_query_iter(w, q);
    while (ecs_query_next(&it)) n += (uint64_t)it.count;
    ecs_query_fini(q);
    return n;
}

typedef struct { double ms; uint64_t checksum; } Measured;

#define ITER_PASSES 20

static void spawn_a(ecs_world_t *w, int n, int width) {
    for (int i = 0; i < n; i++) {
        ecs_entity_t e = ecs_new(w);
        ecs_set(w, e, C0, { (float)i, 0, 0, 0 });
        if (width >= 2) ecs_set(w, e, C1, { 1, 1, 1, 1 });
        if (width >= 4) { ecs_set(w, e, C2, {1,1,1,1}); ecs_set(w, e, C3, {1,1,1,1}); }
        if (width >= 8) {
            ecs_set(w, e, C4, {1,1,1,1}); ecs_set(w, e, C5, {1,1,1,1});
            ecs_set(w, e, C6, {1,1,1,1}); ecs_set(w, e, C7, {1,1,1,1});
        }
    }
}

static Measured case_iter_write_1(int n, int passes) {
    ecs_world_t *w = make_world();
    spawn_a(w, n, 1);
    ecs_query_t *q = ecs_query(w, { .terms = {{ .id = ecs_id(C0) }} });
    double t0 = now_ms();
    for (int p = 0; p < passes; p++) {
        ecs_iter_t it = ecs_query_iter(w, q);
        while (ecs_query_next(&it)) {
            C0 *c = ecs_field(&it, C0, 0);
            for (int i = 0; i < it.count; i++) c[i].x += 1.0f;
        }
    }
    double ms = now_ms() - t0;
    ecs_query_fini(q);
    Measured m = { ms, checksum_c0(w) };
    ecs_fini(w);
    return m;
}

static Measured case_iter_rw_2(int n, int passes) {
    ecs_world_t *w = make_world();
    spawn_a(w, n, 2);
    ecs_query_t *q = ecs_query(w, { .terms = {{ .id = ecs_id(C0) }, { .id = ecs_id(C1), .inout = EcsIn }} });
    double t0 = now_ms();
    for (int p = 0; p < passes; p++) {
        ecs_iter_t it = ecs_query_iter(w, q);
        while (ecs_query_next(&it)) {
            C0 *a = ecs_field(&it, C0, 0);
            const C1 *b = ecs_field(&it, C1, 1);
            for (int i = 0; i < it.count; i++) a[i].x += b[i].x;
        }
    }
    double ms = now_ms() - t0;
    ecs_query_fini(q);
    Measured m = { ms, checksum_c0(w) };
    ecs_fini(w);
    return m;
}

static Measured case_iter_rw_4(int n, int passes) {
    ecs_world_t *w = make_world();
    spawn_a(w, n, 4);
    ecs_query_t *q = ecs_query(w, { .terms = {
        { .id = ecs_id(C0) }, { .id = ecs_id(C1), .inout = EcsIn },
        { .id = ecs_id(C2), .inout = EcsIn }, { .id = ecs_id(C3), .inout = EcsIn } } });
    double t0 = now_ms();
    for (int p = 0; p < passes; p++) {
        ecs_iter_t it = ecs_query_iter(w, q);
        while (ecs_query_next(&it)) {
            C0 *a = ecs_field(&it, C0, 0);
            const C1 *b = ecs_field(&it, C1, 1);
            const C2 *c = ecs_field(&it, C2, 2);
            const C3 *d = ecs_field(&it, C3, 3);
            for (int i = 0; i < it.count; i++) a[i].x += b[i].x + c[i].x + d[i].x;
        }
    }
    double ms = now_ms() - t0;
    ecs_query_fini(q);
    Measured m = { ms, checksum_c0(w) };
    ecs_fini(w);
    return m;
}

static Measured case_iter_rw_8(int n, int passes) {
    ecs_world_t *w = make_world();
    spawn_a(w, n, 8);
    ecs_query_t *q = ecs_query(w, { .terms = {
        { .id = ecs_id(C0) },
        { .id = ecs_id(C1), .inout = EcsIn }, { .id = ecs_id(C2), .inout = EcsIn },
        { .id = ecs_id(C3), .inout = EcsIn }, { .id = ecs_id(C4), .inout = EcsIn },
        { .id = ecs_id(C5), .inout = EcsIn }, { .id = ecs_id(C6), .inout = EcsIn },
        { .id = ecs_id(C7), .inout = EcsIn } } });
    double t0 = now_ms();
    for (int p = 0; p < passes; p++) {
        ecs_iter_t it = ecs_query_iter(w, q);
        while (ecs_query_next(&it)) {
            C0 *a = ecs_field(&it, C0, 0);
            const C1 *b = ecs_field(&it, C1, 1);
            const C2 *c = ecs_field(&it, C2, 2);
            const C3 *d = ecs_field(&it, C3, 3);
            const C4 *e = ecs_field(&it, C4, 4);
            const C5 *f = ecs_field(&it, C5, 5);
            const C6 *g = ecs_field(&it, C6, 6);
            const C7 *h = ecs_field(&it, C7, 7);
            for (int i = 0; i < it.count; i++)
                a[i].x += b[i].x + c[i].x + d[i].x + e[i].x + f[i].x + g[i].x + h[i].x;
        }
    }
    double ms = now_ms() - t0;
    ecs_query_fini(q);
    Measured m = { ms, checksum_c0(w) };
    ecs_fini(w);
    return m;
}

static Measured case_iter_read_2(int n, int passes) {
    ecs_world_t *w = make_world();
    spawn_a(w, n, 2);
    ecs_query_t *q = ecs_query(w, { .terms = {
        { .id = ecs_id(C0), .inout = EcsIn }, { .id = ecs_id(C1), .inout = EcsIn } } });
    double acc = 0.0;
    double t0 = now_ms();
    for (int p = 0; p < passes; p++) {
        ecs_iter_t it = ecs_query_iter(w, q);
        while (ecs_query_next(&it)) {
            const C0 *a = ecs_field(&it, C0, 0);
            const C1 *b = ecs_field(&it, C1, 1);
            for (int i = 0; i < it.count; i++) acc += (double)(a[i].x + b[i].x);
        }
    }
    double ms = now_ms() - t0;
    ecs_query_fini(q);
    Measured m = { ms, f64_bits(acc) };
    ecs_fini(w);
    return m;
}

static Measured case_topology(int k, int passes) {
    ecs_world_t *w = make_world();
    const int n = 100000;
    for (int i = 0; i < n; i++) {
        ecs_entity_t e = ecs_new(w);
        ecs_set(w, e, C0, { (float)i, 0, 0, 0 });
        ecs_set(w, e, C1, { 1, 1, 1, 1 });
        int bits = i % k;
        for (int b = 0; b < 9; b++) {
            if (bits & (1 << b)) ecs_add_id(w, e, T[b]);
        }
    }
    ecs_query_t *q = ecs_query(w, { .terms = {{ .id = ecs_id(C0) }, { .id = ecs_id(C1), .inout = EcsIn }} });
    double t0 = now_ms();
    for (int p = 0; p < passes; p++) {
        ecs_iter_t it = ecs_query_iter(w, q);
        while (ecs_query_next(&it)) {
            C0 *a = ecs_field(&it, C0, 0);
            const C1 *b = ecs_field(&it, C1, 1);
            for (int i = 0; i < it.count; i++) a[i].x += b[i].x;
        }
    }
    double ms = now_ms() - t0;
    ecs_query_fini(q);
    Measured m = { ms, checksum_c0(w) };
    ecs_fini(w);
    return m;
}

#define LIFECYCLE_N 200000

static Measured case_spawn_empty(int n) {
    ecs_world_t *w = make_world();
    double t0 = now_ms();
    for (int i = 0; i < n; i++) ecs_new(w);
    double ms = now_ms() - t0;

    Measured m = { ms, (uint64_t)n };
    ecs_fini(w);
    return m;
}

static Measured case_spawn_2comp(int n) {
    ecs_world_t *w = make_world();
    double t0 = now_ms();
    for (int i = 0; i < n; i++) {
        ecs_entity_t e = ecs_new(w);
        ecs_set(w, e, C0, { (float)i, 0, 0, 0 });
        ecs_set(w, e, C1, { 1, 1, 1, 1 });
    }
    double ms = now_ms() - t0;
    Measured m = { ms, live_entities(w) };
    ecs_fini(w);
    return m;
}

static Measured case_despawn(int n) {
    ecs_world_t *w = make_world();
    ecs_entity_t *ents = malloc(sizeof(ecs_entity_t) * (size_t)n);
    for (int i = 0; i < n; i++) {
        ecs_entity_t e = ecs_new(w);
        ecs_set(w, e, C0, { (float)i, 0, 0, 0 });
        ecs_set(w, e, C1, { 1, 1, 1, 1 });
        ents[i] = e;
    }
    double t0 = now_ms();
    for (int i = 0; i < n; i++) ecs_delete(w, ents[i]);
    double ms = now_ms() - t0;
    Measured m = { ms, live_entities(w) };
    free(ents);
    ecs_fini(w);
    return m;
}

#define STRUCT_N 100000

static uint64_t count_tagged(ecs_world_t *w) {
    ecs_query_t *q = ecs_query(w, { .terms = {{ .id = TagId }} });
    uint64_t n = 0;
    ecs_iter_t it = ecs_query_iter(w, q);
    while (ecs_query_next(&it)) n += (uint64_t)it.count;
    ecs_query_fini(q);
    return n;
}

static Measured case_add_component(int n) {
    ecs_world_t *w = make_world();
    ecs_entity_t *ents = malloc(sizeof(ecs_entity_t) * (size_t)n);
    for (int i = 0; i < n; i++) {
        ecs_entity_t e = ecs_new(w);
        ecs_set(w, e, C0, { (float)i, 0, 0, 0 });
        ents[i] = e;
    }
    double t0 = now_ms();
    for (int i = 0; i < n; i++) ecs_add_id(w, ents[i], TagId);
    double ms = now_ms() - t0;
    Measured m = { ms, count_tagged(w) };
    free(ents);
    ecs_fini(w);
    return m;
}

static Measured case_remove_component(int n) {
    ecs_world_t *w = make_world();
    ecs_entity_t *ents = malloc(sizeof(ecs_entity_t) * (size_t)n);
    for (int i = 0; i < n; i++) {
        ecs_entity_t e = ecs_new(w);
        ecs_set(w, e, C0, { (float)i, 0, 0, 0 });
        ecs_add_id(w, e, TagId);
        ents[i] = e;
    }
    double t0 = now_ms();
    for (int i = 0; i < n; i++) ecs_remove_id(w, ents[i], TagId);
    double ms = now_ms() - t0;
    Measured m = { ms, (live_entities(w) << 32) | count_tagged(w) };
    free(ents);
    ecs_fini(w);
    return m;
}

static Measured case_add_remove_cycle(int cycles) {
    ecs_world_t *w = make_world();
    const int n = 20000;
    ecs_entity_t *ents = malloc(sizeof(ecs_entity_t) * (size_t)n);
    for (int i = 0; i < n; i++) {
        ecs_entity_t e = ecs_new(w);
        ecs_set(w, e, C0, { (float)i, 0, 0, 0 });
        ents[i] = e;
    }
    double t0 = now_ms();
    for (int c = 0; c < cycles; c++) {
        for (int i = 0; i < n; i++) ecs_add_id(w, ents[i], TagId);
        for (int i = 0; i < n; i++) ecs_remove_id(w, ents[i], TagId);
    }
    double ms = now_ms() - t0;
    Measured m = { ms, live_entities(w) };
    free(ents);
    ecs_fini(w);
    return m;
}

#define RANDOM_N 100000

static Measured case_random_get(int passes) {
    ecs_world_t *w = make_world();
    ecs_entity_t *ents = malloc(sizeof(ecs_entity_t) * RANDOM_N);
    for (int i = 0; i < RANDOM_N; i++) {
        ecs_entity_t e = ecs_new(w);
        ecs_set(w, e, C0, { (float)i, 0, 0, 0 });
        ecs_set(w, e, C1, { 1, 1, 1, 1 });
        ents[i] = e;
    }
    int *order = malloc(sizeof(int) * RANDOM_N);
    shuffled_indices(order, RANDOM_N);
    double acc = 0.0;
    double t0 = now_ms();
    for (int p = 0; p < passes; p++) {
        for (int i = 0; i < RANDOM_N; i++) {
            const C0 *c = ecs_get(w, ents[order[i]], C0);
            if (c) acc += (double)c->x;
        }
    }
    double ms = now_ms() - t0;
    Measured m = { ms, f64_bits(acc) };
    free(order); free(ents);
    ecs_fini(w);
    return m;
}

static Measured case_random_write(int passes) {
    ecs_world_t *w = make_world();
    ecs_entity_t *ents = malloc(sizeof(ecs_entity_t) * RANDOM_N);
    for (int i = 0; i < RANDOM_N; i++) {
        ecs_entity_t e = ecs_new(w);
        ecs_set(w, e, C0, { (float)i, 0, 0, 0 });
        ecs_set(w, e, C1, { 1, 1, 1, 1 });
        ents[i] = e;
    }
    int *order = malloc(sizeof(int) * RANDOM_N);
    shuffled_indices(order, RANDOM_N);
    double t0 = now_ms();
    for (int p = 0; p < passes; p++) {
        for (int i = 0; i < RANDOM_N; i++) {
            C0 *c = ecs_get_mut(w, ents[order[i]], C0);
            if (c) c->x += 1.0f;
        }
    }
    double ms = now_ms() - t0;
    Measured m = { ms, checksum_c0(w) };
    free(order); free(ents);
    ecs_fini(w);
    return m;
}

static void sched_a(ecs_iter_t *it) {
    C0 *a = ecs_field(it, C0, 0);
    const C1 *b = ecs_field(it, C1, 1);
    for (int i = 0; i < it->count; i++) a[i].x += b[i].x;
}
static void sched_b(ecs_iter_t *it) {
    C1 *a = ecs_field(it, C1, 0);
    const C2 *b = ecs_field(it, C2, 1);
    for (int i = 0; i < it->count; i++) a[i].y += b[i].y;
}
static void sched_c(ecs_iter_t *it) {
    C2 *a = ecs_field(it, C2, 0);
    const C3 *b = ecs_field(it, C3, 1);
    for (int i = 0; i < it->count; i++) a[i].z += b[i].z;
}

static Measured case_schedule_3sys(int passes) {
    ecs_world_t *w = make_world();
    ECS_SYSTEM(w, sched_a, EcsOnUpdate, C0, [in] C1);
    ECS_SYSTEM(w, sched_b, EcsOnUpdate, C1, [in] C2);
    ECS_SYSTEM(w, sched_c, EcsOnUpdate, C2, [in] C3);
    for (int i = 0; i < 100000; i++) {
        ecs_entity_t e = ecs_new(w);
        ecs_set(w, e, C0, { (float)i, 0, 0, 0 });
        ecs_set(w, e, C1, { 1, 1, 1, 1 });
        ecs_set(w, e, C2, { 1, 1, 1, 1 });
        ecs_set(w, e, C3, { 1, 1, 1, 1 });
    }
    double t0 = now_ms();
    for (int p = 0; p < passes; p++) ecs_progress(w, 0.016f);
    double ms = now_ms() - t0;
    Measured m = { ms, checksum_c0(w) };
    ecs_fini(w);
    return m;
}

typedef struct {
    const char *id;
    const char *group;
    char label[64];
    const char *sweep_key;
    uint64_t sweep_value;
    int entity_count;
    const char *description;
    int kind;
    int param;
} CaseDef;

enum {
    K_A1, K_A2, K_A3, K_A4, K_A5, K_B1,
    K_C1, K_C2, K_C3, K_D1, K_D2, K_D3, K_E1, K_E2, K_G1
};

static int scaled_i(int base, double scale) {
    int v = (int)floor((double)base * scale + 0.5);
    return v < 1 ? 1 : v;
}

static Measured run_case(const CaseDef *c, double scale) {
    int passes = scaled_i(ITER_PASSES, scale);
    int rpasses = scaled_i(10, scale);
    int n = scaled_i(c->param, scale);
    switch (c->kind) {
        case K_A1: return case_iter_write_1(c->param, passes);
        case K_A2: return case_iter_rw_2(c->param, passes);
        case K_A3: return case_iter_rw_4(c->param, passes);
        case K_A4: return case_iter_rw_8(c->param, passes);
        case K_A5: return case_iter_read_2(c->param, passes);
        case K_B1: return case_topology(c->param, passes);
        case K_C1: return case_spawn_empty(n);
        case K_C2: return case_spawn_2comp(n);
        case K_C3: return case_despawn(n);
        case K_D1: return case_add_component(n);
        case K_D2: return case_remove_component(n);
        case K_D3: return case_add_remove_cycle(passes);
        case K_E1: return case_random_get(rpasses);
        case K_E2: return case_random_write(rpasses);
        case K_G1: return case_schedule_3sys(passes);
    }
    Measured z = { 0, 0 };
    return z;
}

static int cmp_double(const void *a, const void *b) {
    double x = *(const double *)a, y = *(const double *)b;
    return (x > y) - (x < y);
}

static double percentile(const double *sorted, int n, double p) {
    if (n == 1) return sorted[0];
    double rank = p * (n - 1);
    int lo = (int)floor(rank), hi = (int)ceil(rank);
    if (lo == hi) return sorted[lo];
    return sorted[lo] + (sorted[hi] - sorted[lo]) * (rank - lo);
}

static void fmt_n(char *buf, size_t cap, int n) {
    if (n >= 1000000) snprintf(buf, cap, "%dM", n / 1000000);
    else if (n >= 1000) snprintf(buf, cap, "%dk", n / 1000);
    else snprintf(buf, cap, "%d", n);
}

int main(int argc, char **argv) {
    int reps = 15, warmup = 3;
    double scale = 1.0;
    const char *out_path = "../../results/fair_flecs.json";
    const char *only = NULL;

    for (int i = 1; i < argc; i++) {
        if (!strcmp(argv[i], "--reps") && i + 1 < argc) reps = atoi(argv[++i]);
        else if (!strcmp(argv[i], "--warmup") && i + 1 < argc) warmup = atoi(argv[++i]);
        else if (!strcmp(argv[i], "--scale") && i + 1 < argc) scale = atof(argv[++i]);
        else if (!strcmp(argv[i], "--only") && i + 1 < argc) only = argv[++i];
        else if (!strcmp(argv[i], "--out") && i + 1 < argc) out_path = argv[++i];
        else { fprintf(stderr, "unknown argument: %s\n", argv[i]); return 2; }
    }

    CaseDef cases[64];
    int nc = 0;
    const int ns[4] = { 1000, 10000, 100000, 1000000 };
    for (int s = 0; s < 4; s++) {
        int n = ns[s];
        char nb[16]; fmt_n(nb, sizeof nb, n);
        struct { const char *id; int kind; const char *fmt; const char *desc; } defs[5] = {
            { "A1", K_A1, "write 1 component", "20 passes of read-modify-write over a single component column" },
            { "A2", K_A2, "read 1 / write 1", "20 passes of the canonical position/velocity loop" },
            { "A3", K_A3, "4 components", "20 passes touching four component columns" },
            { "A4", K_A4, "8 components", "20 passes touching eight component columns" },
            { "A5", K_A5, "read-only 2", "20 read-only passes - isolates the cost change tracking adds to writes" },
        };
        for (int d = 0; d < 5; d++) {
            CaseDef c = { defs[d].id, "Iteration", {0}, "entities", (uint64_t)n, n, defs[d].desc, defs[d].kind, n };
            snprintf(c.label, sizeof c.label, "%s \xc2\xb7 %s", defs[d].fmt, nb);
            cases[nc++] = c;
        }
    }
    const int ks[4] = { 1, 8, 64, 512 };
    for (int i = 0; i < 4; i++) {
        CaseDef c = { "B1", "Topology", {0}, "archetypes", (uint64_t)ks[i], 100000,
                      "100k entities spread over k archetypes, then iterated", K_B1, ks[i] };
        snprintf(c.label, sizeof c.label, "%d archetypes \xc2\xb7 100k", ks[i]);
        cases[nc++] = c;
    }
    {
        CaseDef c1 = { "C1", "Lifecycle", {0}, "entities", 200000, 200000, "identifier allocation with no component data", K_C1, LIFECYCLE_N };
        snprintf(c1.label, sizeof c1.label, "spawn empty \xc2\xb7 200k"); cases[nc++] = c1;
        CaseDef c2 = { "C2", "Lifecycle", {0}, "entities", 200000, 200000, "allocation plus archetype placement and column writes", K_C2, LIFECYCLE_N };
        snprintf(c2.label, sizeof c2.label, "spawn 2 components \xc2\xb7 200k"); cases[nc++] = c2;
        CaseDef c3 = { "C3", "Lifecycle", {0}, "entities", 200000, 200000, "removal, row backfill and identifier recycling", K_C3, LIFECYCLE_N };
        snprintf(c3.label, sizeof c3.label, "despawn \xc2\xb7 200k"); cases[nc++] = c3;
        CaseDef d1 = { "D1", "Structural", {0}, "entities", 100000, 100000, "one archetype move per entity", K_D1, STRUCT_N };
        snprintf(d1.label, sizeof d1.label, "add component \xc2\xb7 100k"); cases[nc++] = d1;
        CaseDef d2 = { "D2", "Structural", {0}, "entities", 100000, 100000, "the reverse archetype move", K_D2, STRUCT_N };
        snprintf(d2.label, sizeof d2.label, "remove component \xc2\xb7 100k"); cases[nc++] = d2;
        CaseDef d3 = { "D3", "Structural", {0}, "entities", 20000, 20000, "repeated moves, exercising any archetype-transition cache", K_D3, 20 };
        snprintf(d3.label, sizeof d3.label, "add/remove cycle \xc2\xb7 20k \xc3\x97 20"); cases[nc++] = d3;
        CaseDef e1 = { "E1", "Random access", {0}, "entities", 100000, 100000, "component lookup by entity handle in shuffled order - the case archetype layouts are weakest at", K_E1, 10 };
        snprintf(e1.label, sizeof e1.label, "random get \xc2\xb7 100k \xc3\x97 10"); cases[nc++] = e1;
        CaseDef e2 = { "E2", "Random access", {0}, "entities", 100000, 100000, "the same lookup, mutating", K_E2, 10 };
        snprintf(e2.label, sizeof e2.label, "random write \xc2\xb7 100k \xc3\x97 10"); cases[nc++] = e2;
        CaseDef g1 = { "G1", "Scheduling", {0}, "entities", 100000, 100000, "three registered systems over the same data through the engine's scheduler", K_G1, 20 };
        snprintf(g1.label, sizeof g1.label, "3 systems \xc2\xb7 100k"); cases[nc++] = g1;
    }

    printf("\n=== FLECS %d.%d.%d - NEUTRAL ECS SUITE (schema v3, single-threaded) ===\n",
        FLECS_VERSION_MAJOR, FLECS_VERSION_MINOR, FLECS_VERSION_PATCH);
    printf("  warmup/reps : %d/%d\n\n", warmup, reps);

    FILE *f = fopen(out_path, "w");
    if (!f) { fprintf(stderr, "cannot open %s\n", out_path); return 1; }
    fprintf(f, "{\n  \"framework\": \"flecs\",\n  \"schema_version\": 3,\n");
    fprintf(f, "  \"suite\": \"neutral-ecs\",\n  \"threading\": \"single\",\n");
    fprintf(f, "  \"env\": { \"platform\": \"native\", \"flecs_version\": \"%d.%d.%d\", \"compiler\": \"%s\" },\n",
        FLECS_VERSION_MAJOR, FLECS_VERSION_MINOR, FLECS_VERSION_PATCH,
#ifdef _MSC_VER
        "msvc"
#else
        "cc"
#endif
    );
    fprintf(f, "  \"cfg\": { \"warmup\": %d, \"reps\": %d, \"parallel\": false, \"work_scale\": %.4f },\n",
        warmup, reps, scale);
    fprintf(f, "  \"results\": [\n");

    const char *group = "";
    int written = 0;
    double *samples = malloc(sizeof(double) * (size_t)reps);

    for (int i = 0; i < nc; i++) {
        CaseDef *c = &cases[i];
        if (only && strcmp(only, c->id) && strcmp(only, c->group)) continue;

        for (int wi = 0; wi < warmup; wi++) { Measured m = run_case(c, scale); (void)m; }

        uint64_t checksum = 0;
        int stable = 1;
        for (int r = 0; r < reps; r++) {
            Measured m = run_case(c, scale);
            samples[r] = m.ms;
            if (r == 0) checksum = m.checksum;
            else if (m.checksum != checksum) stable = 0;
        }
        qsort(samples, (size_t)reps, sizeof(double), cmp_double);

        double mean = 0; for (int r = 0; r < reps; r++) mean += samples[r];
        mean /= reps;
        double var = 0;
        for (int r = 0; r < reps; r++) var += (samples[r] - mean) * (samples[r] - mean);
        double sd = reps > 1 ? sqrt(var / (reps - 1)) : 0.0;
        double med = percentile(samples, reps, 0.5);
        double *devs = malloc(sizeof(double) * (size_t)reps);
        for (int r = 0; r < reps; r++) devs[r] = fabs(samples[r] - med);
        qsort(devs, (size_t)reps, sizeof(double), cmp_double);
        double mad = percentile(devs, reps, 0.5);
        free(devs);
        int k = (int)floor((double)reps / 2.0 - 1.96 * sqrt((double)reps) / 2.0);
        if (k < 0) k = 0;
        int hi = reps - 1 - k; if (hi > reps - 1) hi = reps - 1;

        if (strcmp(group, c->group)) { group = c->group; printf("  %s\n", group); }
        printf("    %-32s %10.3f ms   ci95 [%8.3f, %8.3f]   rsd %5.1f %%%s\n",
            c->label, med, samples[k], samples[hi],
            mean > 0 ? (sd / mean) * 100.0 : 0.0, stable ? "" : "   [!] UNSTABLE");

        if (written++) fprintf(f, ",\n");
        fprintf(f, "    {\n");
        fprintf(f, "      \"id\": \"%s\",\n      \"group\": \"%s\",\n      \"label\": \"%s\",\n",
            c->id, c->group, c->label);
        fprintf(f, "      \"sweep_key\": \"%s\",\n      \"sweep_value\": %llu,\n",
            c->sweep_key, (unsigned long long)c->sweep_value);
        fprintf(f, "      \"entity_count\": %d,\n      \"description\": \"%s\",\n",
            c->entity_count, c->description);
        fprintf(f, "      \"checksum\": \"%llu\",\n      \"checksum_stable\": %s,\n",
            (unsigned long long)checksum, stable ? "true" : "false");
        fprintf(f, "      \"stats\": { \"median\": %.6f, \"min\": %.6f, \"max\": %.6f, \"mean\": %.6f, "
                   "\"stddev\": %.6f, \"mad\": %.6f, \"p05\": %.6f, \"p95\": %.6f, "
                   "\"ci95_median\": [%.6f, %.6f], \"rsd\": %.6f, \"samples\": [",
            med, samples[0], samples[reps - 1], mean, sd, mad,
            percentile(samples, reps, 0.05), percentile(samples, reps, 0.95),
            samples[k], samples[hi], mean > 0 ? sd / mean : 0.0);
        for (int r = 0; r < reps; r++) fprintf(f, "%s%.6f", r ? ", " : "", samples[r]);
        fprintf(f, "] }\n    }");
    }

    if (!only) {
        if (written++) fprintf(f, ",\n");
        fprintf(f, "    {\n      \"id\": \"F1\",\n      \"group\": \"Change detection\",\n");
        fprintf(f, "      \"label\": \"sparse changes \xc2\xb7 200k\",\n");
        fprintf(f, "      \"sweep_key\": \"entities\",\n      \"sweep_value\": 200000,\n");
        fprintf(f, "      \"entity_count\": 200000,\n");
        fprintf(f, "      \"description\": \"1 %% of rows mutated per pass, then queried by change filter\",\n");
        fprintf(f, "      \"unsupported\": true,\n");
        fprintf(f, "      \"unsupported_reason\": \"flecs tracks changes per table via ecs_query_changed(), not per row; emulating a per-row filter would compare a workaround against a native mechanism\"\n");
        fprintf(f, "    }");
    }

    fprintf(f, "\n  ]\n}\n");
    fclose(f);
    free(samples);
    printf("\nWrote %s\n", out_path);
    return 0;
}
