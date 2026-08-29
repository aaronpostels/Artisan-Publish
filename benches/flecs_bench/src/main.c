#include <flecs.h>
#include <stdio.h>
#include <stdlib.h>
#include <stdbool.h>
#include <math.h>

#ifdef _WIN32
#include <windows.h>
double get_time(void) {
    LARGE_INTEGER t, f;
    QueryPerformanceCounter(&t);
    QueryPerformanceFrequency(&f);
    return ((double)t.QuadPart * 1000.0) / (double)f.QuadPart;
}
#else
#include <time.h>
double get_time(void) {
    struct timespec ts;
    clock_gettime(CLOCK_MONOTONIC, &ts);
    return ((double)ts.tv_sec * 1000.0) + ((double)ts.tv_nsec * 1e-6);
}
#endif

typedef struct { float x, y, z; } Pos;
typedef struct { float x, y, z; } Vel;

typedef struct { int d; } Marker;
typedef struct { int d; } MarkerA;
typedef struct { int d; } MarkerB;

ECS_COMPONENT_DECLARE(Pos);
ECS_COMPONENT_DECLARE(Vel);
ECS_COMPONENT_DECLARE(Marker);
ECS_COMPONENT_DECLARE(MarkerA);
ECS_COMPONENT_DECLARE(MarkerB);

ecs_entity_t M[8];

void write_json_start(FILE* f) {
    fprintf(f, "{\n  \"framework\": \"flecs\",\n  \"benchmarks\":[\n");
}
void write_json_bench(FILE* f, const char* name, int count, int iters, double min_ms, bool is_last) {
    fprintf(f, "    {\n      \"name\": \"%s\",\n      \"entity_count\": %d,\n      \"iterations\": %d,\n      \"min_time_ms\": %.3f\n    }%s\n",
        name, count, iters, min_ms, is_last ? "" : ",");
}
void write_json_end(FILE* f) {
    fprintf(f, "  ]\n}\n");
}

void sys1_cb(ecs_iter_t *it) {
    Pos *p = ecs_field(it, Pos, 0); Vel *v = ecs_field(it, Vel, 1);
    for(int j=0; j<it->count; j++) p[j].x += v[j].x;
}
void sys2_cb(ecs_iter_t *it) {
    Pos *p = ecs_field(it, Pos, 0); Vel *v = ecs_field(it, Vel, 1);
    for(int j=0; j<it->count; j++) p[j].y += v[j].y;
}
void sys3_cb(ecs_iter_t *it) {
    Pos *p = ecs_field(it, Pos, 0); Vel *v = ecs_field(it, Vel, 1);
    for(int j=0; j<it->count; j++) p[j].z += v[j].z;
}

int main(void) {
    printf("\n=== FLECS BENCHMARK RUNNER ===\n");

    FILE* result_file = fopen("../results/flecs_results.json", "w");
    if(!result_file) result_file = fopen("results/flecs_results.json", "w");
    if(!result_file) {
        printf("ERROR: Could not open JSON file for writing.\n");
        return 1;
    }
    write_json_start(result_file);

    int IT = 10;
    double t0, t1, min_t;

    printf("  -> Running 1. Entity Spawn... "); fflush(stdout);
    min_t = 999999.0;
    for (int r = 0; r < IT; r++) {
        ecs_world_t *w = ecs_init();
        ECS_COMPONENT_DEFINE(w, Pos); ECS_COMPONENT_DEFINE(w, Vel);
        t0 = get_time();
        for (int i = 0; i < 1000000; i++) {
            ecs_entity_t e = ecs_new(w);
            ecs_add_id(w, e, ecs_id(Pos));
            ecs_add_id(w, e, ecs_id(Vel));
        }
        t1 = get_time();
        if ((t1-t0) < min_t) min_t = (t1-t0);
        ecs_fini(w);
    }
    printf("Done! (%.2f ms)\n", min_t);
    write_json_bench(result_file, "1. Entity Spawn", 1000000, IT, min_t, false);

    printf("  -> Running 2. Entity Despawn... "); fflush(stdout);
    min_t = 999999.0;
    for (int r = 0; r < IT; r++) {
        ecs_world_t *w = ecs_init();
        ecs_entity_t* ents = malloc(1000000 * sizeof(ecs_entity_t));
        for (int i = 0; i < 1000000; i++) ents[i] = ecs_new(w);
        t0 = get_time();
        for (int i = 0; i < 1000000; i++) ecs_delete(w, ents[i]);
        t1 = get_time();
        if ((t1-t0) < min_t) min_t = (t1-t0);
        free(ents);
        ecs_fini(w);
    }
    printf("Done! (%.2f ms)\n", min_t);
    write_json_bench(result_file, "2. Entity Despawn", 1000000, IT, min_t, false);

    printf("  -> Running 3. Dense Iteration... "); fflush(stdout);
    min_t = 999999.0;
    for (int r = 0; r < IT; r++) {
        ecs_world_t *w = ecs_init();
        ECS_COMPONENT_DEFINE(w, Pos); ECS_COMPONENT_DEFINE(w, Vel);
        for (int i = 0; i < 1000000; i++) {
            ecs_entity_t e = ecs_new(w);
            ecs_add_id(w, e, ecs_id(Pos)); ecs_add_id(w, e, ecs_id(Vel));
        }
        ecs_query_t *q = ecs_query(w, { .terms = { { ecs_id(Pos) }, { ecs_id(Vel) } } });

        t0 = get_time();
        for (int k = 0; k < 100; k++) {
            ecs_iter_t it = ecs_query_iter(w, q);
            while (ecs_query_next(&it)) {
                Pos *p = ecs_field(&it, Pos, 0); Vel *v = ecs_field(&it, Vel, 1);
                for (int j = 0; j < it.count; j++) { p[j].x += v[j].x; }
            }
        }
        t1 = get_time();
        if ((t1-t0) < min_t) min_t = (t1-t0);
        ecs_fini(w);
    }
    printf("Done! (%.2f ms)\n", min_t);
    write_json_bench(result_file, "3. Dense Iteration", 1000000, IT, min_t, false);

    printf("  -> Running 4. Fragmented Iteration... "); fflush(stdout);
    min_t = 999999.0;
    for (int r = 0; r < IT; r++) {
        ecs_world_t *w = ecs_init();
        ECS_COMPONENT_DEFINE(w, Pos); ECS_COMPONENT_DEFINE(w, Vel);
        ecs_entity_t M_arr[26];
        for (int m=0; m<26; m++) M_arr[m] = ecs_new(w);

        for (int i = 0; i < 100000; i++) {
            ecs_entity_t e = ecs_new(w);
            ecs_add_id(w, e, ecs_id(Pos)); ecs_add_id(w, e, ecs_id(Vel));
            ecs_add_id(w, e, M_arr[i % 26]);
        }
        ecs_query_t *q = ecs_query(w, { .terms = { { ecs_id(Pos) }, { ecs_id(Vel) } } });

        t0 = get_time();
        for (int k = 0; k < 100; k++) {
            ecs_iter_t it = ecs_query_iter(w, q);
            while (ecs_query_next(&it)) {
                Pos *p = ecs_field(&it, Pos, 0); Vel *v = ecs_field(&it, Vel, 1);
                for (int j = 0; j < it.count; j++) { p[j].x += v[j].x; }
            }
        }
        t1 = get_time();
        if ((t1-t0) < min_t) min_t = (t1-t0);
        ecs_fini(w);
    }
    printf("Done! (%.2f ms)\n", min_t);
    write_json_bench(result_file, "4. Fragmented Iteration", 100000, IT, min_t, false);

    printf("  -> Running 5. Add/Remove Churn... "); fflush(stdout);
    min_t = 999999.0;
    for (int r = 0; r < IT; r++) {
        ecs_world_t *w = ecs_init();
        ECS_COMPONENT_DEFINE(w, Marker);
        ecs_entity_t* ents = malloc(100000 * sizeof(ecs_entity_t));
        for (int i = 0; i < 100000; i++) ents[i] = ecs_new(w);

        t0 = get_time();
        for (int k = 0; k < 100; k++) {
            for (int i = 0; i < 100000; i++) ecs_add_id(w, ents[i], ecs_id(Marker));
            for (int i = 0; i < 100000; i++) ecs_remove_id(w, ents[i], ecs_id(Marker));
        }
        t1 = get_time();
        if ((t1-t0) < min_t) min_t = (t1-t0);
        free(ents);
        ecs_fini(w);
    }
    printf("Done! (%.2f ms)\n", min_t);
    write_json_bench(result_file, "5. Add/Remove Churn", 100000, IT, min_t, false);

    printf("  -> Running 6. Query Filtering... "); fflush(stdout);
    min_t = 999999.0;
    for (int r = 0; r < IT; r++) {
        ecs_world_t *w = ecs_init();
        ECS_COMPONENT_DEFINE(w, Pos); ECS_COMPONENT_DEFINE(w, Vel);
        ECS_COMPONENT_DEFINE(w, MarkerA); ECS_COMPONENT_DEFINE(w, MarkerB);

        for (int i = 0; i < 200000; i++) {
            ecs_entity_t e = ecs_new(w);
            ecs_add_id(w, e, ecs_id(Pos)); ecs_add_id(w, e, ecs_id(Vel));
            if (i % 2 == 0) ecs_add_id(w, e, ecs_id(MarkerA));
            else ecs_add_id(w, e, ecs_id(MarkerB));
        }

        ecs_query_t *q = ecs_query(w, {
            .terms = {
                { ecs_id(Pos) }, { ecs_id(Vel) },
                { ecs_id(MarkerA) }, { ecs_id(MarkerB), .oper = EcsNot }
            }
        });

        t0 = get_time();
        for (int k = 0; k < 100; k++) {
            ecs_iter_t it = ecs_query_iter(w, q);
            while (ecs_query_next(&it)) {
                Pos *p = ecs_field(&it, Pos, 0); Vel *v = ecs_field(&it, Vel, 1);
                for (int j = 0; j < it.count; j++) { p[j].x += v[j].x; }
            }
        }
        t1 = get_time();
        if ((t1-t0) < min_t) min_t = (t1-t0);
        ecs_fini(w);
    }
    printf("Done! (%.2f ms)\n", min_t);
    write_json_bench(result_file, "6. Query Filtering", 200000, IT, min_t, false);

    printf("  -> Running 7. Mixed Density... "); fflush(stdout);
    min_t = 999999.0;
    for (int r = 0; r < IT; r++) {
        ecs_world_t *w = ecs_init();
        ECS_COMPONENT_DEFINE(w, Pos); ECS_COMPONENT_DEFINE(w, Vel);
        for(int m=0; m<8; m++) M[m] = ecs_new(w);

        for (int i = 0; i < 255; i++) {
            ecs_entity_t e = ecs_new(w);
            ecs_add_id(w, e, ecs_id(Pos)); ecs_add_id(w, e, ecs_id(Vel));
            for(int bit=0; bit<8; bit++) if ((i & (1 << bit))) ecs_add_id(w, e, M[bit]);
        }
        for (int i = 0; i < 100000; i++) {
            ecs_entity_t e = ecs_new(w);
            ecs_add_id(w, e, ecs_id(Pos)); ecs_add_id(w, e, ecs_id(Vel));
            for(int bit=0; bit<8; bit++) ecs_add_id(w, e, M[bit]);
        }

        ecs_query_t *q = ecs_query(w, { .terms = { { ecs_id(Pos) }, { ecs_id(Vel) } } });

        t0 = get_time();
        for (int k = 0; k < 100; k++) {
            ecs_iter_t it = ecs_query_iter(w, q);
            while (ecs_query_next(&it)) {
                Pos *p = ecs_field(&it, Pos, 0); Vel *v = ecs_field(&it, Vel, 1);
                for (int j = 0; j < it.count; j++) { p[j].x += v[j].x; }
            }
        }
        t1 = get_time();
        if ((t1-t0) < min_t) min_t = (t1-t0);
        ecs_fini(w);
    }
    printf("Done! (%.2f ms)\n", min_t);
    write_json_bench(result_file, "7. Mixed Density", 100255, IT, min_t, false);

    printf("  -> Running 8. Scheduling... "); fflush(stdout);
    min_t = 999999.0;
    for (int r = 0; r < IT; r++) {
        ecs_world_t *w = ecs_init();
        ECS_COMPONENT_DEFINE(w, Pos); ECS_COMPONENT_DEFINE(w, Vel);

        for (int i = 0; i < 100000; i++) {
            ecs_entity_t e = ecs_new(w);
            ecs_add_id(w, e, ecs_id(Pos)); ecs_add_id(w, e, ecs_id(Vel));
        }

        ecs_system(w, {
            .query = { .terms = {{ ecs_id(Pos) }, { ecs_id(Vel) }} },
            .callback = sys1_cb
        });
        ecs_system(w, {
            .query = { .terms = {{ ecs_id(Pos) }, { ecs_id(Vel) }} },
            .callback = sys2_cb
        });
        ecs_system(w, {
            .query = { .terms = {{ ecs_id(Pos) }, { ecs_id(Vel) }} },
            .callback = sys3_cb
        });

        t0 = get_time();
        for (int k = 0; k < 100; k++) { ecs_progress(w, 0.016f); }
        t1 = get_time();
        if ((t1-t0) < min_t) min_t = (t1-t0);
        ecs_fini(w);
    }
    printf("Done! (%.2f ms)\n", min_t);
    write_json_bench(result_file, "8. Scheduling", 100000, IT, min_t, true);

    write_json_end(result_file);
    fclose(result_file);
    printf("=== FLECS BENCHMARK COMPLETE ===\n\n");
    return 0;
}
