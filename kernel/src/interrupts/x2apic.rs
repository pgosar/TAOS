#![allow(dead_code)]
use crate::serial_println;
use raw_cpuid::CpuId;
use x86_64::instructions::port::Port;
use x86_64::registers::model_specific::Msr;

#[derive(Debug)]
pub enum X2ApicError {
    NotSupported,
    AlreadyEnabled,
    InvalidDestination,
    InvalidVector,
    InvalidPriority,
    CalibrationFailed,
}

// Mode the timer is operating in
#[derive(Debug, Clone, Copy)]
pub enum TimerMode {
    TscDeadline,
    Periodic,
}

// MSR constants
const IA32_APIC_BASE_MSR: u32 = 0x1B;
const IA32_X2APIC_APICID: u32 = 0x802;
const IA32_TSC_DEADLINE: u32 = 0x6E0;
const X2APIC_MSR_BASE: u32 = 0x800;

// Register offsets
const OFFSET_ID: u32 = 0x02;
const OFFSET_EOI: u32 = 0x0B;
const OFFSET_SVR: u32 = 0x0F;
const OFFSET_LVT_TIMER: u32 = 0x32;
const OFFSET_TIMER_INITIAL_COUNT: u32 = 0x38;
const OFFSET_TIMER_CURRENT_COUNT: u32 = 0x39;
const OFFSET_TIMER_DIVIDE_CONFIG: u32 = 0x3E;

// Timer mode bits
const TIMER_MODE_PERIODIC: u64 = 1 << 17;
const TSC_DEADLINE_MODE: u64 = 1 << 18;

// PIT (Programmable Interval Timer) ports
const PIT_CHANNEL_0: u16 = 0x40;
const PIT_COMMAND: u16 = 0x43;

unsafe fn init_pit_oneshot(count: u16) {
    let mut command_port: Port<u8> = Port::new(PIT_COMMAND);
    let mut data_port: Port<u8> = Port::new(PIT_CHANNEL_0);

    command_port.write(0x30);
    data_port.write(count as u8);
    data_port.write((count >> 8) as u8);
}

unsafe fn wait_pit_complete() {
    let mut command_port: Port<u8> = Port::new(PIT_COMMAND);
    let mut status_port: Port<u8> = Port::new(PIT_CHANNEL_0);

    command_port.write(0xE2);

    loop {
        let status = status_port.read();
        if (status & 0x80) != 0 {
            break;
        }
        core::hint::spin_loop();
    }
}

// Calibrate either TSC or APIC timer using PIT
unsafe fn calibrate() -> Result<u64, X2ApicError> {
    // Increased to ~50ms for better accuracy under virtualization
    const PIT_CALIBRATION_CYCLES: u16 = 59660; // ~50ms at 1.193182 MHz
    const CALIBRATION_MS: u64 = 50; // Make sure this matches PIT_CALIBRATION_CYCLES

    serial_println!("Starting calibration over {}ms...", CALIBRATION_MS);

    // Setup APIC timer for calibration
    Msr::new(X2APIC_MSR_BASE + OFFSET_TIMER_DIVIDE_CONFIG).write(0b1011);
    Msr::new(X2APIC_MSR_BASE + OFFSET_TIMER_INITIAL_COUNT).write(u32::MAX as u64);

    let start_tsc = core::arch::x86_64::_rdtsc();
    init_pit_oneshot(PIT_CALIBRATION_CYCLES);
    wait_pit_complete();
    let end_tsc = core::arch::x86_64::_rdtsc();

    let tsc_diff = end_tsc - start_tsc;
    let apic_counted =
        u32::MAX - Msr::new(X2APIC_MSR_BASE + OFFSET_TIMER_CURRENT_COUNT).read() as u32;

    serial_println!("Raw TSC difference: {}", tsc_diff);
    serial_println!("Raw APIC ticks counted: {}", apic_counted);

    // Now divide by calibration period to get per-ms rate
    let tsc_per_ms = tsc_diff / CALIBRATION_MS;
    let apic_per_ms = apic_counted as u64 / CALIBRATION_MS;

    serial_println!("TSC ticks per ms: {}", tsc_per_ms);
    serial_println!("APIC ticks per ms: {}", apic_per_ms);

    if tsc_per_ms == 0 || apic_per_ms == 0 {
        return Err(X2ApicError::CalibrationFailed);
    }

    // Add sanity check for virtualized environment
    if tsc_per_ms < 50_000 {
        serial_println!(
            "Warning: Low TSC rate detected ({}), running under heavy virtualization",
            tsc_per_ms
        );
    }

    let cpuid = CpuId::new();
    if cpuid
        .get_feature_info()
        .map_or(false, |f| f.has_tsc_deadline())
    {
        Ok(tsc_per_ms)
    } else {
        Ok(apic_per_ms)
    }
}

pub fn init() -> Result<(u32, TimerMode, u64), X2ApicError> {
    unsafe {
        let apic_base = Msr::new(IA32_APIC_BASE_MSR);
        let value = apic_base.read();

        if (value & (1 << 10)) == 0 {
            return Err(X2ApicError::NotSupported);
        }

        let id = Msr::new(IA32_X2APIC_APICID).read() as u32;

        // Check TSC Deadline support
        let cpuid = CpuId::new();
        let mode = if cpuid
            .get_feature_info()
            .map_or(false, |f| f.has_tsc_deadline())
        {
            TimerMode::TscDeadline
        } else {
            TimerMode::Periodic
        };

        // Calibrate based on selected mode
        let ticks = calibrate()?;

        Ok((id, mode, ticks))
    }
}

pub fn enable_timer(
    vector: u8,
    mode: TimerMode,
    frequency_hz: u32,
    ticks_per_ms: u64,
) -> Result<(), X2ApicError> {
    if vector < 32 {
        return Err(X2ApicError::InvalidVector);
    }

    unsafe {
        match mode {
            TimerMode::TscDeadline => {
                // Configure for TSC Deadline mode
                Msr::new(X2APIC_MSR_BASE + OFFSET_LVT_TIMER)
                    .write(TSC_DEADLINE_MODE | vector as u64);

                // Schedule first tick
                let current_tsc = core::arch::x86_64::_rdtsc();
                let deadline = current_tsc + ((1000 * ticks_per_ms) / frequency_hz as u64);
                Msr::new(IA32_TSC_DEADLINE).write(deadline);
            }
            TimerMode::Periodic => {
                // Configure for Periodic mode
                let ticks = (ticks_per_ms * 1000) / frequency_hz as u64;

                Msr::new(X2APIC_MSR_BASE + OFFSET_LVT_TIMER)
                    .write(TIMER_MODE_PERIODIC | vector as u64);
                Msr::new(X2APIC_MSR_BASE + OFFSET_TIMER_DIVIDE_CONFIG).write(0);
                Msr::new(X2APIC_MSR_BASE + OFFSET_TIMER_INITIAL_COUNT).write(ticks);
            }
        }

        // Enable APIC and set spurious vector
        Msr::new(X2APIC_MSR_BASE + OFFSET_SVR).write(0x100 | 0xFF);
    }
    Ok(())
}

pub fn schedule_next_deadline(ticks_per_ms: u64, ms: u64) {
    unsafe {
        let current_tsc = core::arch::x86_64::_rdtsc();
        let deadline = current_tsc + (ms * ticks_per_ms);
        Msr::new(IA32_TSC_DEADLINE).write(deadline);
    }
}

pub fn send_eoi() {
    unsafe {
        Msr::new(X2APIC_MSR_BASE + OFFSET_EOI).write(0);
    }
}

pub fn stop_timer(mode: TimerMode) {
    unsafe {
        match mode {
            TimerMode::TscDeadline => {
                Msr::new(IA32_TSC_DEADLINE).write(0);
            }
            TimerMode::Periodic => {
                Msr::new(X2APIC_MSR_BASE + OFFSET_TIMER_INITIAL_COUNT).write(0);
            }
        }
        // Mask the timer
        Msr::new(X2APIC_MSR_BASE + OFFSET_LVT_TIMER).write(1 << 16);
    }
}
