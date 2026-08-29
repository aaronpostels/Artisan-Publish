const { execSync } = require("child_process");
const path = require("path");

const root = path.resolve(__dirname, "../..");
const bevyDir = path.join(root, "benches/bevy_bench");
const args = process.argv.slice(2).join(" ");

const env = {
  ...process.env,
  RUSTFLAGS: "-C target-cpu=native -C codegen-units=1",
};

try {
  execSync(
    `cargo build --release --bin bevy_bench --manifest-path "${path.join(bevyDir, "Cargo.toml")}"`,
    {
      stdio: "inherit",
      env,
    },
  );

  execSync(
    `cargo run --release --bin bevy_bench --manifest-path "${path.join(bevyDir, "Cargo.toml")}" -- ${args}`,
    {
      stdio: "inherit",
      env,
    },
  );
} catch (e) {
  process.exit(1);
}
