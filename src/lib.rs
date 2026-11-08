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
        if lo > GOLDILOCKS_PRIME {
            lo.wrapping_sub(GOLDILOCKS_PRIME)
        } else {
            lo
        }
    }

    pub fn field_pow(base: u64, mut exp: u64) -> u64 {
        let mut res = 1u64;
        let mut val = base % GOLDILOCKS_PRIME;
        while exp > 0 {
            if exp & 1 == 1 {
                res = Self::field_mul(res, val);
            }
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
            let word = u64::from_le_bytes(bytes);
            elements[i] = word % GOLDILOCKS_PRIME;
        }
        elements
    }

    pub fn generate_io_commitment(&self, elements: &[u64; 8], descriptor: &StagingDescriptor) -> u64 {
        let mut commitment = descriptor.allocated_slot % GOLDILOCKS_PRIME;
        for (idx, &el) in elements.iter().enumerate() {
            let alpha = Self::field_pow(FIELD_GENERATOR, (idx + 1) as u64);
            let term = Self::field_mul(el, alpha);
            commitment = Self::field_add(commitment, term);
        }
        commitment
    }

    pub fn aggregate_interrupt_pool(&self, pool: &[HardwareInterrupt]) -> u64 {
        let mut delta = 0u64;
        for (idx, interrupt) in pool.iter().enumerate() {
            let vid = interrupt.vector_id % GOLDILOCKS_PRIME;
            let ds = interrupt.data_signal % GOLDILOCKS_PRIME;
            let hash = Self::field_mul(vid, ds);
            let weight = Self::field_pow(FIELD_GENERATOR, idx as u64);
            let term = Self::field_mul(hash, weight);
            delta = Self::field_add(delta, term);
        }
        delta
    }

    pub fn generate_masked_shares(&self, secret: u64, entropy: u64) -> (u64, u64) {
        let s2 = entropy % GOLDILOCKS_PRIME;
        let s1 = Self::field_sub(secret % GOLDILOCKS_PRIME, s2);
        (s1, s2)
    }
}

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

pub struct BridgeAdapter;

impl BridgeAdapter {
    pub fn fold_sources(sources: &[&dyn FieldSource]) -> u64 {
        let mut acc = 0u64;
        for (idx, src) in sources.iter().enumerate() {
            let el = src.as_field_element();
            let weight = GoldilocksBridgeKernel::field_pow(FIELD_GENERATOR, idx as u64);
            let term = GoldilocksBridgeKernel::field_mul(el, weight);
            acc = GoldilocksBridgeKernel::field_add(acc, term);
        }
        acc
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
        let term = GoldilocksBridgeKernel::field_mul(el, weight);
        self.acc = GoldilocksBridgeKernel::field_add(self.acc, term);
        self.count = self.count.wrapping_add(1);
    }
    pub fn current(&self) -> u64 { self.acc }
    pub fn items_seen(&self) -> u64 { self.count }
}

impl Default for StreamAccumulator {
    fn default() -> Self { Self::new() }
}

/// An agent that can be spawned on demand and carries its own accumulator.
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

    pub fn state(&self) -> u64 {
        self.accumulator.current()
    }
}

/// Registry that can create agents and skills as needed.
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

    /// Create a skill as a named folding function over FieldSources.
    pub fn create_skill(&self, name_hash: u64) -> impl Fn(&[&dyn FieldSource]) -> u64 {
        move |sources| {
            let base = GoldilocksBridgeKernel::field_add(name_hash, BridgeAdapter::fold_sources(sources));
            base
        }
    }
}

impl Default for AgentRegistry {
    fn default() -> Self { Self::new() }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn agents_can_be_created_and_absorb() {
        let mut registry = AgentRegistry::new();
        let mut agent = registry.create_agent();
        agent.absorb(&42u64);
        agent.absorb(&HardwareInterrupt { vector_id: 1, data_signal: 2 });
        assert!(agent.state() > 0);
    }

    #[test]
    fn skills_can_be_created() {
        let registry = AgentRegistry::new();
        let skill = registry.create_skill(0xSKILL);
        let sources: [&dyn FieldSource; 1] = [&99u64];
        let result = skill(&sources);
        assert!(result > 0);
    }
}
