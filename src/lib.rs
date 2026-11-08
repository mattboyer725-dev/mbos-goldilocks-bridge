use std::collections::HashMap;
use std::fs::{self, File};
use std::io::{self, Read};
use std::path::{Path, PathBuf};
use serde::{Serialize, Deserialize};
use sha2::{Sha256, Digest};

pub const GOLDILOCKS_PRIME: u64 = 18446744069414584321;

pub struct GoldilocksBridgeKernel;

impl GoldilocksBridgeKernel {
    pub fn field_add(a: u64, b: u64) -> u64 {
        let sum = a.wrapping_add(b);
        let mask = ((sum >= GOLDILOCKS_PRIME) as u64).wrapping_neg();
        sum.wrapping_sub(GOLDILOCKS_PRIME & mask)
    }
    pub fn field_mul(a: u64, b: u64) -> u64 {
        let wide = (a as u128) * (b as u128);
        let mut lo = wide as u64;
        let hi = (wide >> 64) as u64;
        lo = lo.wrapping_sub(hi);
        let mask = ((lo > GOLDILOCKS_PRIME) as u64).wrapping_neg();
        lo.wrapping_sub(GOLDILOCKS_PRIME & mask)
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct NodeState {
    pub node_id: String,
    pub state: u64,
    pub generation: u64,
    pub adaptation_rate: u64,
}

pub struct AdaptiveLoop {
    pub node_id: String,
    acc: u64,
    learned_bias: u64,
    adaptation_rate: u64,
    generation: u64,
    target: u64,
}

impl AdaptiveLoop {
    pub fn new(node_id: impl Into<String>, target: u64) -> Self {
        Self {
            node_id: node_id.into(),
            acc: 0,
            learned_bias: 0,
            adaptation_rate: 10,
            generation: 0,
            target: target % GOLDILOCKS_PRIME,
        }
    }

    pub fn step(&mut self, raw: u64) {
        let magnitude = raw.wrapping_sub(self.learned_bias);
        let error = self.target.wrapping_sub(self.acc);
        let rate = self.adaptation_rate;
        let adjustment = GoldilocksBridgeKernel::field_add(
            GoldilocksBridgeKernel::field_mul(error, rate),
            GoldilocksBridgeKernel::field_mul(magnitude, 5),
        );
        self.learned_bias = GoldilocksBridgeKernel::field_add(self.learned_bias, adjustment);
        self.learned_bias = self.learned_bias.wrapping_shr(4);

        if magnitude > (GOLDILOCKS_PRIME >> 2) {
            self.adaptation_rate = (self.adaptation_rate * 9) / 10;
            if self.adaptation_rate < 1 { self.adaptation_rate = 1; }
        } else {
            self.adaptation_rate = (self.adaptation_rate * 21) / 20;
            if self.adaptation_rate > 50 { self.adaptation_rate = 50; }
        }

        let biased = GoldilocksBridgeKernel::field_add(raw, self.learned_bias);
        self.acc = GoldilocksBridgeKernel::field_add(self.acc, biased);
        self.generation = self.generation.wrapping_add(1);
    }

    pub fn current_state(&self) -> NodeState {
        NodeState {
            node_id: self.node_id.clone(),
            state: self.acc,
            generation: self.generation,
            adaptation_rate: self.adaptation_rate,
        }
    }

    pub fn run_local_steps(&mut self, steps: u64, seed: u64) {
        for i in 0..steps {
            self.step(seed.wrapping_add(i));
        }
    }
}

pub fn sanitize_loader_env() {
    std::env::remove_var("LD_PRELOAD");
    std::env::remove_var("LD_LIBRARY_PATH");
    std::env::remove_var("LD_AUDIT");
    std::env::remove_var("LD_DEBUG");
}

pub struct IntegrityAuditor {
    target_dir: PathBuf,
    baseline_path: PathBuf,
}

impl IntegrityAuditor {
    pub fn new(target_dir: impl AsRef<Path>, baseline_path: impl AsRef<Path>) -> Self {
        Self {
            target_dir: target_dir.as_ref().to_path_buf(),
            baseline_path: baseline_path.as_ref().to_path_buf(),
        }
    }

    fn hash_file(path: &Path) -> io::Result<String> {
        let mut file = File::open(path)?;
        let mut hasher = Sha256::new();
        let mut buf = [0u8; 65536];
        loop {
            let n = file.read(&mut buf)?;
            if n == 0 { break; }
            hasher.update(&buf[..n]);
        }
        Ok(format!("{:x}", hasher.finalize()))
    }

    fn scan(&self) -> io::Result<HashMap<String, String>> {
        let mut map = HashMap::new();
        if !self.target_dir.exists() {
            return Ok(map);
        }
        for entry in fs::read_dir(&self.target_dir)? {
            let entry = entry?;
            let path = entry.path();
            if path.is_file() {
                if let Some(name) = path.file_name().and_then(|n| n.to_str()) {
                    if let Ok(hash) = Self::hash_file(&path) {
                        map.insert(name.to_string(), hash);
                    }
                }
            }
        }
        Ok(map)
    }

    pub fn create_baseline(&self) -> io::Result<()> {
        let map = self.scan()?;
        let json = serde_json::to_string_pretty(&map)?;
        fs::write(&self.baseline_path, json)?;
        Ok(())
    }

    pub fn audit(&self) -> io::Result<bool> {
        if !self.baseline_path.exists() {
            self.create_baseline()?;
            return Ok(true);
        }
        let baseline: HashMap<String, String> =
            serde_json::from_str(&fs::read_to_string(&self.baseline_path)?)?;
        let current = self.scan()?;

        let baseline_keys: std::collections::HashSet<_> = baseline.keys().cloned().collect();
        let current_keys: std::collections::HashSet<_> = current.keys().cloned().collect();

        let new_files = current_keys.difference(&baseline_keys).count();
        let deleted = baseline_keys.difference(&current_keys).count();
        let modified = current_keys.intersection(&baseline_keys)
            .filter(|k| current.get(*k) != baseline.get(*k))
            .count();

        Ok(new_files == 0 && deleted == 0 && modified == 0)
    }

    pub fn assert_read_only(&self) -> io::Result<()> {
        let meta = fs::metadata(&self.target_dir)?;
        if meta.permissions().readonly() {
            Ok(())
        } else {
            Err(io::Error::new(io::ErrorKind::PermissionDenied, "target directory is writable"))
        }
    }
}

/// Hardened boot: sanitize env, require read-only directory, then audit, then run.
pub fn boot_and_run(node_id: &str, target: u64, dir: &Path) -> Result<(), String> {
    sanitize_loader_env();

    let manifest = dir.join("baseline_manifest.json");
    let auditor = IntegrityAuditor::new(dir, &manifest);

    // Hard check: refuse to start if directory is writable
    auditor.assert_read_only()
        .map_err(|e| format!("directory not read-only: {e}"))?;

    match auditor.audit() {
        Ok(true) => {}
        Ok(false) => return Err("integrity check failed".into()),
        Err(e) => return Err(format!("audit I/O error: {e}")),
    }

    let mut node = AdaptiveLoop::new(node_id, target);
    node.run_local_steps(10, 1);
    let _ = node.current_state();
    Ok(())
}
