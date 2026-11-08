use mbos_goldilocks_bridge::*;

fn main() {
    let kernel = GoldilocksBridgeKernel;
    let mut desc = StagingDescriptor {
        allocated_slot: 0x5500,
        payload_len: 16,
        buffer: [0u8; 64],
    };
    desc.buffer[0..8].copy_from_slice(&0x1122334455667788u64.to_le_bytes());

    let elements = kernel.arithmetize_stream(&desc);
    let commitment = kernel.generate_io_commitment(&elements, &desc);
    println!("commitment = {commitment:#x}");

    let pool = [HardwareInterrupt { vector_id: 1, data_signal: 42 }];
    let delta = kernel.aggregate_interrupt_pool(&pool);
    println!("delta       = {delta:#x}");
}
