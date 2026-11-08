#![cfg_attr(not(feature = "std"), no_std)]

/// Goldilocks prime: 2^64 - 2^32 + 1
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
    /// Constant-time addition modulo p
    #[inline(always)]
    pub fn field_add(a: u64, b: u64) -> u64 {
        let sum = a.wrapping_add(b);
        let mask = ((sum >= GOLDILOCKS_PRIME) as u64).wrapping_neg();
        sum.wrapping_sub(GOLDILOCKS_PRIME & mask)
    }

    /// Constant-time subtraction modulo p
    #[inline(always)]
    pub fn field_sub(a: u64, b: u64) -> u64 {
        let diff = a.wrapping_sub(b);
        let mask = ((a < b) as u64).wrapping_neg();
        diff.wrapping_add(GOLDILOCKS_PRIME & mask)
    }

    /// Constant-time multiplication modulo p (wide multiply)
    #[inline(always)]
    pub fn field_mul(a: u64, b: u64) -> u64 {
        let wide = (a as u128) * (b as u128);
        (wide % GOLDILOCKS_PRIME as u128) as u64
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn field_arithmetic_roundtrip() {
        let a = 123456789u64;
        let b = 987654321u64;
        let sum = GoldilocksBridgeKernel::field_add(a, b);
        let recovered = GoldilocksBridgeKernel::field_sub(sum, b);
        assert_eq!(recovered, a % GOLDILOCKS_PRIME);
    }

    #[test]
    fn masking_reconstitution() {
        let kernel = GoldilocksBridgeKernel;
        let secret = 0xBADCAFE123456789u64 % GOLDILOCKS_PRIME;
        let (s1, s2) = kernel.generate_masked_shares(secret, 0x44A21B90);
        let recovered = GoldilocksBridgeKernel::field_add(s1, s2);
        assert_eq!(recovered, secret);
    }
}
