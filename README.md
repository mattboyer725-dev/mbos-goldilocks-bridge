# MBOS-GOLDILOCKS-BRIDGE

Cryptographic Hardware Abstraction Layer for Verifiable Microkernels.

Goldilocks prime field arithmetization, IOMMU-style staging, deterministic interrupt aggregation, and first-order side-channel masking.

## Build & Run

```bash
cargo run
```

For RISC-V bare-metal:

```bash
rustup target add riscv64gc-unknown-none-elf
cargo build --target riscv64gc-unknown-none-elf --release
```

## Architecture

- Layer 1: Physical isolation / Staging Ring
- Layer 2: zk-vDMA Arithmetization + Polynomial Commitments
- Layer 3: Asynchronous Interrupt Fusion
- Layer 4: Side-Channel Masking

Reference implementation matching the original engineering handoff.
