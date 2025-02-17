use core::{arch::asm, sync::atomic::Ordering};

use x86_64::VirtAddr;

use crate::{
    constants::{idt::TLB_SHOOTDOWN_VECTOR, MAX_CORES},
    interrupts::x2apic::{current_core_id, send_ipi_all_cores, TLB_SHOOTDOWN_ADDR},
};

pub fn tlb_shootdown(target_vaddr: VirtAddr) {
    let current_core = current_core_id();

    let vaddr = target_vaddr.as_u64();

    for core in 0..MAX_CORES {
        if core != current_core {
            TLB_SHOOTDOWN_ADDR[core].store(vaddr, Ordering::SeqCst);
        }
    }

    send_ipi_all_cores(TLB_SHOOTDOWN_VECTOR);

    unsafe {
        asm!("invlpg [{}]", in(reg) vaddr, options(nostack, preserves_flags));
    }
}
