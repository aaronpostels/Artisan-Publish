const { execSync } = require('child_process');
const fs = require('fs');
const path = require('path');

console.log(`--- FLECS BUILD SYSTEM (NINJA + MSVC 19.50) OPTIMIZED ---`);

const vswherePath = 'C:\\Program Files (x86)\\Microsoft Visual Studio\\Installer\\vswhere.exe';
const installPath = execSync(`"${vswherePath}" -latest -products * -property installationPath`, { encoding: 'utf8' }).trim();
const vcvars = path.join(installPath, 'VC', 'Auxiliary', 'Build', 'vcvars64.bat');

const root = path.resolve(__dirname, '../..');
const flecsDir = path.join(root, 'benches/flecs_bench');
const buildDir = path.join(flecsDir, 'build');
const exeOut = path.join(buildDir, 'flecs_bench.exe');

console.log('[0/4] Cleaning locked build directory...');
try {
    execSync(`taskkill /f /im cmake.exe 2>nul`, { stdio: 'ignore' });
    execSync(`taskkill /f /im ninja.exe 2>nul`, { stdio: 'ignore' });
    execSync(`rmdir /s /q "${buildDir}" 2>nul || true`, { stdio: 'ignore' });
} catch (e) {
    console.log('Clean complete');
}

fs.mkdirSync(buildDir, { recursive: true });

const batchPath = path.join(root, 'ninja_build.bat');
const batchContent = `@echo off
call "${vcvars}" >nul 2>&1

rmdir /s /q "${buildDir}" >nul 2>&1

echo [1/4] Configuring Ninja + Max Optimization...
cmake -S "${flecsDir}" -B "${buildDir}" -G "Ninja" ^
  -DCMAKE_BUILD_TYPE=Release ^
  -DCMAKE_C_FLAGS_RELEASE="/O2 /GL /GS- /DNDEBUG /Oi /Gy /arch:AVX2" ^
  -DCMAKE_EXE_LINKER_FLAGS_RELEASE="/LTCG /OPT:REF /OPT:ICF /INCREMENTAL:NO" ^
  -DCMAKE_VERBOSE_MAKEFILE=ON

if %errorlevel% neq 0 (
    echo CMake configuration failed
    exit /b 1
)

echo [2/4] Building with Ninja (Max Speed)...
cmake --build "${buildDir}" --config Release --verbose

if %errorlevel% neq 0 (
    echo Ninja build failed
    exit /b 1
)

echo [3/4] Build Success! Running Flecs Benchmark...
if exist "${exeOut}" (
    "${exeOut}"
) else (
    echo ERROR: flecs_bench.exe not found
    exit /b 1
)

if %errorlevel% neq 0 exit /b 1
echo [4/4] SUCCESS - Optimal Flecs build complete!
exit /b 0
`;

fs.writeFileSync(batchPath, batchContent, 'utf8');

try {
    console.log('[3/4] Executing optimized build...');
    execSync(`"${batchPath}"`, { stdio: 'inherit' });
    console.log('\n🎉 FLECS BUILD COMPLETE - MAXIMUM PERFORMANCE ACHIEVED!');
} catch (e) {
    console.error('\n❌ Build failed - check logs above');
    process.exit(1);
} finally {
    if (fs.existsSync(batchPath)) fs.unlinkSync(batchPath);
}
