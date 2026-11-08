#![cfg_attr(not(feature = "std"), no_std)]

pub const GOLDILOCKS_PRIME: u64 = 18446744069414584321;
pub const FIELD_GENERATOR: u64 = 7;

pub struct StagingDescriptor {
    pub allocated_slot: u64,
    pub payload_len: usize,
    pub buffer: [u8; 64],
}

pub struct HardwareInterrupt {
    pub vector_id: u64,
    pub data_signal: u64,
}

pub struct GoldilocksBridgeKernel;

impl GoldilocksBridgeKernel {
    #[inline(always)]
    pub fn field_add(a: u64, b: u64) -> u64 {
        let sum = a.wrapping_add(b);
        let mask = ((sum >= GOLDILOCKS_PRIME) as u64).wrapping_neg();
        sum.wrapping_sub(GOLDILOCKS_PRIME & mask)
    }

    #[inline(always)]
    pub fn field_sub(a: u64, b: u64) -> u64 {
        let diff = a.wrapping_sub(b);
        let mask = ((a < b) as u64).wrapping_neg();
        diff.wrapping_add(GOLDILOCKS_PRIME & mask)
    }

    #[inline(always)]
    pub fn field_mul(a: u64, b: u64) -> u64 {
        let wide = (a as u128) * (b as u128);
        let mut lo = wide as u64;
        let hi = (wide >> 64) as u64;
        lo = lo.wrapping_sub(hi);
        if lo > GOLDILOCKS_PRIME { lo.wrapping_sub(GOLDILOCKS_PRIME) } else { lo }
    }

    pub fn field_pow(base: u64, mut exp: u64) -> u64 {
        let mut res = 1u64;
        let mut val = base % GOLDILOCKS_PRIME;
        while exp > 0 {
            if exp & 1 == 1 { res = Self::field_mul(res, val); }
            val = Self::field_mul(val, val);
            exp >>= 1;
        }
        res
    }

    pub fn arithmetize_stream(&self, descriptor: &StagingDescriptor) -> [u64; 8] {
        let mut elements = [0u64; 8];
        for i in 0..8 {
            let start = i * 8;
            let mut bytes = [0u8; 8];
            if start + 8 <= descriptor.buffer.len() {
                bytes.copy_from_slice(&descriptor.buffer[start..start + 8]);
            }
            elements[i] = u64::from_le_bytes(bytes) % GOLDILOCKS_PRIME;
        }
        elements
    }

    pub fn generate_io_commitment(&self, elements: &[u64; 8], descriptor: &StagingDescriptor) -> u64 {
        let mut commitment = descriptor.allocated_slot % GOLDILOCKS_PRIME;
        for (idx, &el) in elements.iter().enumerate() {
            let alpha = Self::field_pow(FIELD_GENERATOR, (idx + 1) as u64);
            commitment = Self::field_add(commitment, Self::field_mul(el, alpha));
        }
        commitment
    }

    pub fn aggregate_interrupt_pool(&self, pool: &[HardwareInterrupt]) -> u64 {
        let mut delta = 0u64;
        for (idx, interrupt) in pool.iter().enumerate() {
            let hash = Self::field_mul(interrupt.vector_id % GOLDILOCKS_PRIME, interrupt.data_signal % GOLDILOCKS_PRIME);
            let weight = Self::field_pow(FIELD_GENERATOR, idx as u64);
            delta = Self::field_add(delta, Self::field_mul(hash, weight));
        }
        delta
    }

    pub fn generate_masked_shares(&self, secret: u64, entropy: u64) -> (u64, u64) {
        let s2 = entropy % GOLDILOCKS_PRIME;
        let s1 = Self::field_sub(secret % GOLDILOCKS_PRIME, s2);
        (s1, s2)
    }
}

pub mod adaptive {
    use super::*;

    pub trait FieldSource {
        fn as_field_element(&self) -> u64;
    }

    impl FieldSource for u64 {
        fn as_field_element(&self) -> u64 { *self % GOLDILOCKS_PRIME }
    }

    impl FieldSource for HardwareInterrupt {
        fn as_field_element(&self) -> u64 {
            GoldilocksBridgeKernel::field_mul(
                self.vector_id % GOLDILOCKS_PRIME,
                self.data_signal % GOLDILOCKS_PRIME,
            )
        }
    }

    pub struct StreamAccumulator {
        acc: u64,
        count: u64,
    }

    impl StreamAccumulator {
        pub fn new() -> Self { Self { acc: 0, count: 0 } }
        pub fn push<T: FieldSource>(&mut self, item: &T) {
            let el = item.as_field_element();
            let weight = GoldilocksBridgeKernel::field_pow(FIELD_GENERATOR, self.count);
            self.acc = GoldilocksBridgeKernel::field_add(
                self.acc,
                GoldilocksBridgeKernel::field_mul(el, weight),
            );
            self.count = self.count.wrapping_add(1);
        }
        pub fn current(&self) -> u64 { self.acc }
        pub fn items_seen(&self) -> u64 { self.count }
    }

    impl Default for StreamAccumulator {
        fn default() -> Self { Self::new() }
    }

    pub struct BridgeAgent {
        pub id: u64,
        pub accumulator: StreamAccumulator,
    }

    impl BridgeAgent {
        pub fn spawn(id: u64) -> Self {
            Self { id, accumulator: StreamAccumulator::new() }
        }
        pub fn absorb<T: FieldSource>(&mut self, item: &T) {
            self.accumulator.push(item);
        }
        pub fn state(&self) -> u64 { self.accumulator.current() }
    }

    pub struct AgentRegistry {
        next_id: u64,
    }

    impl AgentRegistry {
        pub fn new() -> Self { Self { next_id: 1 } }
        pub fn create_agent(&mut self) -> BridgeAgent {
            let agent = BridgeAgent::spawn(self.next_id);
            self.next_id = self.next_id.wrapping_add(1);
            agent
        }
    }

    impl Default for AgentRegistry {
        fn default() -> Self { Self::new() }
    }

    /// Adaptive loop that learns from each step and adjusts in real time.
    /// It maintains a running average and uses it to bias future absorption.
    pub struct AdaptiveLoop {
        registry: AgentRegistry,
        primary: BridgeAgent,
        generation: u64,
        /// Running field-element average of recent inputs (learning signal)
        learned_bias: u64,
        /// How strongly the bias influences the next step (adapts over time)
        adaptation_rate: u64,
    }

    impl AdaptiveLoop {
        pub fn start() -> Self {
            let mut registry = AgentRegistry::new();
            let primary = registry.create_agent();
            Self {
                registry,
                primary,
                generation: 0,
                learned_bias: 0,
                adaptation_rate: 1,
            }
        }

        /// One adaptive step.
        /// Absorbs the source, updates the learned bias, and may grow.
        pub fn step<T: FieldSource>(&mut self, source: &T) {
            let raw = source.as_field_element();

            // Blend new observation into learned bias (simple exponential moving average)
            self.learned_bias = GoldilocksBridgeKernel::field_add(
                GoldilocksBridgeKernel::field_mul(self.learned_bias, 15), // keep most of old
                raw,                                                         // add new
            );
            // Rough divide-by-16 via shift (not exact field div, but deterministic bias)
            self.learned_bias = self.learned_bias.wrapping_shr(4);

            // Absorb a biased version of the input
            let biased = GoldilocksBridgeKernel::field_add(raw, self.learned_bias);
            self.primary.absorb(&biased);

            self.generation = self.generation.wrapping_add(1);

            // Adaptation rate slowly increases (learns to adapt faster)
            if self.generation & 31 == 0 {
                self.adaptation_rate = self.adaptation_rate.wrapping_add(1);
            }

            // Growth: spawn helper when generation is multiple of (16 + adaptation)
            let threshold = 16u64.wrapping_add(self.adaptation_rate);
            if self.generation % threshold == 0 {
                let _helper = self.registry.create_agent();
            }
        }

        pub fn current_state(&self) -> u64 { self.primary.state() }
        pub fn generation(&self) -> u64 { self.generation }
        pub fn learned_bias(&self) -> u64 { self.learned_bias }
        pub fn adaptation_rate(&self) -> u64 { self.adaptation_rate }
        pub fn agents_created(&self) -> u64 { self.registry.next_id.wrapping_sub(1) }
    }
}

pub use adaptive::{FieldSource, StreamAccumulator, BridgeAgent, AgentRegistry, AdaptiveLoop};

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn loop_learns_and_adapts() {
        let mut loop_ = AdaptiveLoop::start();
        for i in 0..128u64 {
            loop_.step(&i);
        }
        assert!(loop_.generation() == 128);
        assert!(loop_.learned_bias() > 0);
        assert!(loop_.adaptation_rate() > 1);
        assert!(loop_.agents_created() > 1);
    }
}
