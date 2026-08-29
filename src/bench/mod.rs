pub mod bridge;
pub mod cases;
pub mod env;
pub mod fair;
pub mod harness;

pub use env::BenchEnv;
pub use harness::{BenchCfg, BenchSpec, CaseResult, Group, Stats, SubQuestion};

#[derive(Debug, Clone, serde::Serialize)]
pub struct BenchRun {
    pub framework: &'static str,
    pub schema_version: u32,
    pub env: BenchEnv,
    pub cfg: BenchCfg,
    pub results: Vec<CaseResult>,
}

pub fn run_suite(ids: &[u32], cfg: &BenchCfg, mut on_progress: impl FnMut(&CaseResult)) -> BenchRun {
    let mut results = Vec::new();
    for case in cases::core_cases() {
        if !ids.is_empty() && !ids.contains(&case.spec.id) {
            continue;
        }
        let r = harness::measure(case.spec, case.f, cfg);
        on_progress(&r);
        results.push(r);
    }
    BenchRun {
        framework: "artisan",
        schema_version: 2,
        env: BenchEnv::capture(),
        cfg: cfg.clone(),
        results,
    }
}

pub fn run_case(id: u32, cfg: &BenchCfg) -> Option<CaseResult> {
    cases::core_cases()
        .into_iter()
        .find(|c| c.spec.id == id)
        .map(|c| harness::measure(c.spec, c.f, cfg))
}

pub fn manifest() -> Vec<BenchSpec> {
    cases::core_cases().into_iter().map(|c| c.spec).collect()
}
