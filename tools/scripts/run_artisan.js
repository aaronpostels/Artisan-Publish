const { execSync } = require('child_process');
const path = require('path');

console.log(`--- ARTISAN BUILD SYSTEM (RUSTC 2024 OPTIMIZED) ---`);

const root = path.resolve(__dirname, '../..');
const args = process.argv.slice(2).join(' ');

const env = {
    ...process.env,
    RUSTFLAGS: "-C target-cpu=native -C codegen-units=1",
};

try {
    console.log('[1/2] Compiling Artisan Native Benchmark...');
    execSync(`cargo build --release --bin artisan_bin`, {
        stdio: 'inherit',
        env
    });

    console.log('\n[2/2] Build Success! Running Artisan Benchmark...\n');
    execSync(`cargo run --release --bin artisan_bin -- ${args}`, {
        stdio: 'inherit',
        env
    });

} catch (e) {
    console.error('\n❌ Artisan build/run failed');
    process.exit(1);
}
