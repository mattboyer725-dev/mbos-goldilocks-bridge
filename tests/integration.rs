use mbos_goldilocks_bridge::*;

#[test]
fn full_pipeline_test() {
    let kernel = GoldilocksBridgeKernel;
    let mut buffer = [0u8; 64];
    for i in 0..64 { buffer[i] = i as u8; }
    let desc = StagingDescriptor { allocated_slot: 0x5500A1, payload_len: 64, buffer };
    let elements = kernel.arithmetize_stream(&desc);
    let commitment = kernel.generate_io_commitment(&elements, &desc);
    assert!(commitment > 0);

    let interrupts = [HardwareInterrupt { vector_id: 1, data_signal: 42 }];
    let delta = kernel.aggregate_interrupt_pool(&interrupts);
    assert!(delta > 0);

    let (s1, s2) = kernel.generate_masked_shares(0xBADCAFE, 0x12345678);
    let recovered = GoldilocksBridgeKernel::field_add(s1, s2);
    assert_eq!(recovered, 0xBADCAFE % GOLDILOCKS_PRIME);
}
