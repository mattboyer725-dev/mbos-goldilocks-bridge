#![cfg_attr(not(feature = "std"), no_std)]

pub const GOLDILOCKS_PRIME: u64 = 18446744069414584321;

pub struct GoldilocksBridgeKernel;

impl GoldilocksBridgeKernel {
    #[inline(always)]
    pub fn field_add(a: u64, b: u64) -> u64 {
        let sum = a.wrapping_add(b);
        let mask = ((sum >= GOLDILOCKS_PRIME) as u64).wrapping_neg();
        sum.wrapping_sub(GOLDILOCKS_PRIME & mask)
    }

    #[inline(always)]
    pub fn field_mul(a: u64, b: u64) -> u64 {
        let wide = (a as u128) * (b as u128);
        let mut lo = wide as u64;
        let hi = (wide >> 64) as u64;
        lo = lo.wrapping_sub(hi);
        let mask = ((lo > GOLDILOCKS_PRIME) as u64).wrapping_neg();
        lo.wrapping_sub(GOLDILOCKS_PRIME & mask)
    }
}

pub trait FieldSource {
    fn as_field_element(&self) -> u64;
}

impl FieldSource for u64 {
    fn as_field_element(&self) -> u64 { *self % GOLDILOCKS_PRIME }
}

pub trait LocalConnection {
    fn next_field_element(&mut self) -> Option<u64>;
}

pub struct StreamAccumulator {
    acc: u64,
    count: u64,
}

impl StreamAccumulator {
    pub fn new() -> Self { Self { acc: 0, count: 0 } }
    pub fn push<T: FieldSource>(&mut self, item: &T) {
        let el = item.as_field_element();
        let weight = GoldilocksBridgeKernel::field_mul(7, self.count.wrapping_add(1));
        self.acc = GoldilocksBridgeKernel::field_add(self.acc, GoldilocksBridgeKernel::field_mul(el, weight));
        self.count = self.count.wrapping_add(1);
    }
    pub fn current(&self) -> u64 { self.acc }
}

impl Default for StreamAccumulator {
    fn default() -> Self { Self::new() }
}

/// Local-only adaptive loop with dynamic sensitivity.
/// Large magnitude observations dampen the adaptation rate;
/// stable observations allow it to increase.
pub struct AdaptiveLoop {
    acc: StreamAccumulator,
    learned_bias: u64,
    adaptation_rate: u64,   // scaled integer: 100 = 1.0
    generation: u64,
    target: u64,
}

impl AdaptiveLoop {
    pub fn start() -> Self {
        Self::with_target(0)
    }

    pub fn with_target(target: u64) -> Self {
        Self {
            acc: StreamAccumulator::new(),
            learned_bias: 0,
            adaptation_rate: 10, // 0.10
            generation: 0,
            target: target % GOLDILOCKS_PRIME,
        }
    }

    /// Single step with dynamic sensitivity (ported from the Python heuristic).
    pub fn step<T: FieldSource>(&mut self, source: &T) {
        let raw = source.as_field_element();
        let magnitude = raw.wrapping_sub(self.learned_bias); // rough local delta

        // Error against target
        let error = self.target.wrapping_sub(self.acc.current());

        // Adjustment influenced by current adaptation rate
        let rate = self.adaptation_rate;
        let adjustment = GoldilocksBridgeKernel::field_add(
            GoldilocksBridgeKernel::field_mul(error, rate),
            GoldilocksBridgeKernel::field_mul(magnitude, 5),
        );

        // Update bias
        self.learned_bias = GoldilocksBridgeKernel::field_add(self.learned_bias, adjustment);
        self.learned_bias = self.learned_bias.wrapping_shr(4);

        // Dynamic sensitivity:
        // high magnitude -> dampen rate; low magnitude -> increase rate
        if magnitude > (GOLDILOCKS_PRIME >> 2) {
            // dampen
            self.adaptation_rate = (self.adaptation_rate * 9) / 10;
            if self.adaptation_rate < 1 { self.adaptation_rate = 1; }
        } else {
            // accelerate
            self.adaptation_rate = (self.adaptation_rate * 21) / 20;
            if self.adaptation_rate > 50 { self.adaptation_rate = 50; }
        }

        let biased = GoldilocksBridgeKernel::field_add(raw, self.learned_bias);
        self.acc.push(&biased);
        self.generation = self.generation.wrapping_add(1);
    }

    pub fn run_local<C: LocalConnection>(&mut self, conn: &mut C, max_steps: u64) {
        let mut steps = 0;
        while steps < max_steps {
            match conn.next_field_element() {
                Some(el) => {
                    self.step(&el);
                    steps += 1;
                }
                None => break,
            }
        }
    }

    pub fn current_state(&self) -> u64 { self.acc.current() }
    pub fn generation(&self) -> u64 { self.generation }
    pub fn learned_bias(&self) -> u64 { self.learned_bias }
    pub fn adaptation_rate(&self) -> u64 { self.adaptation_rate }
    pub fn is_local_only(&self) -> bool { true }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn dynamic_sensitivity_changes() {
        let mut loop_ = AdaptiveLoop::with_target(100);
        let start_rate = loop_.adaptation_rate();
        for i in 0..20u64 {
            loop_.step(&(i * 1000));
        }
        assert_eq!(loop_.generation(), 20);
        // rate should have moved from the initial value
        assert_ne!(loop_.adaptation_rate(), start_rate);
    }
}
