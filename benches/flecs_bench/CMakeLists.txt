cmake_minimum_required(VERSION 3.20)
project(flecs_bench C)

set(CMAKE_C_STANDARD 99)
set(CMAKE_C_STANDARD_REQUIRED ON)

add_executable(flecs_bench src/main.c src/flecs.c)

# Neutral cross-engine suite (docs/FAIR_BENCH_CHARTER.md).
add_executable(flecs_fair src/fair.c src/flecs.c)

foreach(target flecs_bench flecs_fair)
    target_include_directories(${target} PRIVATE src)
    target_compile_definitions(${target} PRIVATE FLECS_STATIC)

    if(MSVC)
        target_compile_options(${target} PRIVATE /O2 /GL /GS- /DNDEBUG /Oi /Gy /arch:AVX2)
        target_link_options(${target} PRIVATE /LTCG /OPT:REF /OPT:ICF /INCREMENTAL:NO)
    endif()

    if(UNIX AND NOT APPLE)
        target_link_libraries(${target} PRIVATE m)
    endif()
endforeach()
