pub const STATE_DIM: usize = 8;
pub const GOLDILOCKS_PRIME: u64 = 0xFFFF_FFFF_0000_0001;

#[derive(Debug, Clone, Copy)]
pub struct HardenedEngine {
    pub state_vector: [u64; STATE_DIM],
    pub iteration: u64,
    learning_rate: u64,
}

impl HardenedEngine {
    #[inline(always)]
    pub const fn new(learning_rate: u64) -> Self {
        Self {
            state_vector: [0; STATE_DIM],
            iteration: 0,
            learning_rate,
        }
    }

    #[inline(always)]
    pub fn step(&mut self, target: u64, load: u64, drift: u64) {
        self.iteration += 1;

        let mut sum: u64 = 0;
        let mut i = 0;
        while i < STATE_DIM {
            sum = (sum + self.state_vector[i]) % GOLDILOCKS_PRIME;
            i += 1;
        }

        let mean = sum / (STATE_DIM as u64);
        let error = if target >= mean {
            target - mean
        } else {
            GOLDILOCKS_PRIME - (mean - target)
        };

        let load_dampener = if load < 100 { 100 - load } else { 1 };
        let delta = (error * self.learning_rate * load_dampener / 100) % GOLDILOCKS_PRIME;

        i = 0;
        while i < STATE_DIM {
            let divisor = (i as u64) + 1;
            let adjustment = delta / divisor;
            let combined = self.state_vector[i]
                + adjustment
                + GOLDILOCKS_PRIME
                - (drift % GOLDILOCKS_PRIME);
            self.state_vector[i] = combined % GOLDILOCKS_PRIME;
            i += 1;
        }
    }
}
