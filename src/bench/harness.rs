pub struct Timer {
    #[cfg(not(target_arch = "wasm32"))]
    start: std::time::Instant,
    #[cfg(target_arch = "wasm32")]
    start: f64,
}

#[cfg(target_arch = "wasm32")]
#[wasm_bindgen::prelude::wasm_bindgen]
extern "C" {
    #[wasm_bindgen(js_namespace = performance, js_name = now)]
    fn performance_now() -> f64;
}

impl Timer {
    #[inline]
    pub fn start() -> Self {
        #[cfg(not(target_arch = "wasm32"))]
        {
            Self { start: std::time::Instant::now() }
        }
        #[cfg(target_arch = "wasm32")]
        {
            Self { start: performance_now() }
        }
    }

    #[inline]
    pub fn elapsed_ms(&self) -> f64 {
        #[cfg(not(target_arch = "wasm32"))]
        {
            self.start.elapsed().as_secs_f64() * 1000.0
        }
        #[cfg(target_arch = "wasm32")]
        {
            performance_now() - self.start
        }
    }
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct Stats {
    pub samples: Vec<f64>,
    pub min: f64,
    pub max: f64,
    pub mean: f64,
    pub median: f64,

    pub stddev: f64,

    pub mad: f64,
    pub p05: f64,
    pub p95: f64,

    pub ci95_median: [f64; 2],

    pub rsd: f64,
}

fn u64_as_string<S: serde::Serializer>(v: &u64, s: S) -> Result<S::Ok, S::Error> {
    s.serialize_str(&v.to_string())
}

fn percentile(sorted: &[f64], p: f64) -> f64 {
    if sorted.is_empty() {
        return f64::NAN;
    }
    if sorted.len() == 1 {
        return sorted[0];
    }
    let rank = p * (sorted.len() - 1) as f64;
    let lo = rank.floor() as usize;
    let hi = rank.ceil() as usize;
    if lo == hi {
        sorted[lo]
    } else {
        sorted[lo] + (sorted[hi] - sorted[lo]) * (rank - lo as f64)
    }
}

impl Stats {
    pub fn from_samples(samples: Vec<f64>) -> Self {
        assert!(!samples.is_empty(), "cannot summarise an empty sample vector");
        let mut sorted = samples.clone();
        sorted.sort_by(|a, b| a.partial_cmp(b).unwrap());

        let n = sorted.len();
        let mean = sorted.iter().sum::<f64>() / n as f64;
        let median = percentile(&sorted, 0.5);

        let stddev = if n > 1 {
            let var = sorted.iter().map(|s| (s - mean) * (s - mean)).sum::<f64>() / (n - 1) as f64;
            var.sqrt()
        } else {
            0.0
        };

        let mut devs: Vec<f64> = sorted.iter().map(|s| (s - median).abs()).collect();
        devs.sort_by(|a, b| a.partial_cmp(b).unwrap());
        let mad = percentile(&devs, 0.5);

        let k = ((n as f64) / 2.0 - 1.96 * (n as f64).sqrt() / 2.0).floor();
        let lo_idx = k.max(0.0) as usize;
        let hi_idx = (n - 1 - lo_idx).min(n - 1);

        Self {
            min: sorted[0],
            max: sorted[n - 1],
            mean,
            median,
            stddev,
            mad,
            p05: percentile(&sorted, 0.05),
            p95: percentile(&sorted, 0.95),
            ci95_median: [sorted[lo_idx], sorted[hi_idx]],
            rsd: if mean > 0.0 { stddev / mean } else { 0.0 },
            samples: sorted,
        }
    }
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct BenchCfg {

    pub warmup: usize,

    pub reps: usize,

    pub parallel: bool,

    pub work_scale: f64,
}

impl Default for BenchCfg {
    fn default() -> Self {
        Self { warmup: 3, reps: 15, parallel: true, work_scale: 1.0 }
    }
}

pub struct Measured {

    pub ms: f64,

    pub checksum: u64,
}

pub type CaseFn = fn(&BenchCfg) -> Measured;

#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub enum SubQuestion {

    EcsProperties,

    NativeComparison,

    Supporting,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub enum Group {

    Core,

    Extended,
}

#[derive(Debug, Clone, serde::Serialize)]
pub struct BenchSpec {
    pub id: u32,
    pub name: &'static str,

    pub entity_count: usize,

    pub inner_iters: usize,
    pub group: Group,
    pub subquestion: SubQuestion,

    pub measures: &'static str,

    pub excludes: &'static str,

    pub checksum_of: &'static str,
}

#[derive(Debug, Clone, serde::Serialize)]
pub struct CaseResult {
    #[serde(flatten)]
    pub spec: BenchSpec,
    pub parallel: bool,
    pub work_scale: f64,
    pub warmup: usize,
    pub reps: usize,

    #[serde(serialize_with = "u64_as_string")]
    pub checksum: u64,

    pub checksum_stable: bool,
    pub stats: Stats,

    pub min_time_ms: f64,
}

pub fn measure(spec: BenchSpec, f: CaseFn, cfg: &BenchCfg) -> CaseResult {
    for _ in 0..cfg.warmup {
        let m = f(cfg);
        std::hint::black_box(m.checksum);
    }

    let mut samples = Vec::with_capacity(cfg.reps);
    let mut checksum = 0u64;
    let mut stable = true;

    for i in 0..cfg.reps {
        let m = f(cfg);
        samples.push(m.ms);
        if i == 0 {
            checksum = m.checksum;
        } else if m.checksum != checksum {
            stable = false;
        }
    }

    let stats = Stats::from_samples(samples);
    CaseResult {
        min_time_ms: stats.min,
        spec,
        parallel: cfg.parallel,
        work_scale: cfg.work_scale,
        warmup: cfg.warmup,
        reps: cfg.reps,
        checksum,
        checksum_stable: stable,
        stats,
    }
}

#[inline]
pub fn scaled(base: usize, cfg: &BenchCfg) -> usize {
    ((base as f64 * cfg.work_scale).round() as usize).max(1)
}
