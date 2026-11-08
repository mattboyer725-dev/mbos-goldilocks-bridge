//! Reference no_std entry stub.
//! Not a complete UEFI application; shows the freestanding shape.
//! Build with a freestanding target if you later wire a real loader.

#![cfg_attr(not(test), no_std)]

use core::panic::PanicInfo;

#[repr(C)]
pub struct SystemContext {
    pub cr3: u64,
    pub stack_pointer: u64,
    pub flags: u32,
}

/// Placeholder entry. A real UEFI binary would use the efiapi ABI
/// and a proper system table. This is a shape reference only.
pub fn initialize_hardware_context() -> SystemContext {
    SystemContext {
        cr3: 0x0010_0000,
        stack_pointer: 0x7FFF_FFFF,
        flags: 0x0200,
    }
}

pub fn execute_bare_metal_loop(ctx: &mut SystemContext) -> ! {
    let mut acc: u64 = 0;
    let rate: u64 = 1;
    loop {
        #[cfg(target_arch = "x86_64")]
        unsafe {
            core::arch::asm!("pause");
        }
        acc = acc.wrapping_add(rate);
        ctx.flags = ctx.flags.wrapping_add(1);
        // In a real freestanding binary this would never return.
        if acc == 0 { break; }
    }
    loop {}
}

#[cfg(not(test))]
#[panic_handler]
fn panic(_info: &PanicInfo) -> ! {
    loop {
        #[cfg(target_arch = "x86_64")]
        unsafe {
            core::arch::asm!("cli", "hlt");
        }
    }
}
