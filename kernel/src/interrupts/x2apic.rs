use x2apic::lapic::{xapic_base, IpiDestMode, LocalApicBuilder, TimerDivide, TimerMode};
use x86_64::instructions::port::Port;

use crate::serial_print;

#[derive(Debug)]
pub enum ApicError {
    TimerOverflow,
    CalibrationFailed,
    ApicInitFailed,
    PitTimeout,
}

// PIT constants
const PIT_FREQ: u32 = 1193182;
const CHANNEL_0: u16 = 0x40;
const MODE_CMD: u16 = 0x43;

const TIMER_VECTOR: usize = 32;
const ERROR_VECTOR: usize = 33;
const SPURIOUS_VECTOR: usize = 0xFF;

const PIT_TICKS: u16 = 50;

/// Initialize x2APIC for the current CPU
pub fn init_x2apic() -> Result<(), ApicError> {
    let mut lapic = unsafe {
        LocalApicBuilder::new()
            .set_xapic_base(xapic_base())
            .timer_vector(TIMER_VECTOR)
            .error_vector(ERROR_VECTOR)
            .spurious_vector(SPURIOUS_VECTOR)
            .timer_mode(TimerMode::Periodic)
            .timer_divide(TimerDivide::Div2)
            .build()
            .map_err(|_| ApicError::ApicInitFailed)?
    };

    unsafe {
        lapic.enable();
    }

    Ok(())
}

/// Signal end-of-interrupt
#[inline(always)]
pub fn eoi() -> Result<(), ApicError> {
    let mut lapic = unsafe {
        LocalApicBuilder::new()
            .set_xapic_base(xapic_base())
            .timer_vector(TIMER_VECTOR)
            .error_vector(ERROR_VECTOR)
            .spurious_vector(SPURIOUS_VECTOR)
            .build()
            .map_err(|_| ApicError::ApicInitFailed)?
    };

    unsafe {
        lapic.end_of_interrupt();
    }
    Ok(())
}

/// Send IPI to target CPU
#[inline]
pub fn send_ipi(vector: u8, target_cpu: u32) -> Result<(), ApicError> {
    let mut lapic = unsafe {
        LocalApicBuilder::new()
            .set_xapic_base(xapic_base())
            .timer_vector(TIMER_VECTOR)
            .error_vector(ERROR_VECTOR)
            .spurious_vector(SPURIOUS_VECTOR)
            .ipi_destination_mode(IpiDestMode::Physical)
            .build()
            .map_err(|_| ApicError::ApicInitFailed)?
    };

    unsafe {
        lapic.send_ipi(vector, target_cpu);
        // Wait for delivery
        while lapic.get_ipi_delivery_status() {
            core::hint::spin_loop();
        }
    }

    Ok(())
}

struct Pit {
    channel0: Port<u8>,
    mode_cmd: Port<u8>,
}

impl Pit {
    const fn new() -> Self {
        Self {
            channel0: Port::new(CHANNEL_0),
            mode_cmd: Port::new(MODE_CMD),
        }
    }

    unsafe fn configure_for_calibration(&mut self, count: u16) -> Result<(), ApicError> {
        // Channel 0, access mode LSB/MSB, mode 2 (rate generator)
        self.mode_cmd.write(0x34);

        // Write count value - LSB first, then MSB
        self.channel0.write((count & 0xFF) as u8);
        self.channel0.write((count >> 8) as u8);

        Ok(())
    }

    unsafe fn read_count(&mut self) -> u16 {
        // Latch count value command for channel 0
        self.mode_cmd.write(0x00);

        // Read count - LSB then MSB
        let low = self.channel0.read() as u16;
        let high = self.channel0.read() as u16;
        (high << 8) | low
    }

    unsafe fn wait_for_completion(&mut self, original_count: u16) -> Result<(), ApicError> {
        let mut prev_count = self.read_count();
        let mut iterations = 0;
        let mut total_ticks = 0;
        
        while iterations < 1000000 {
            let current = self.read_count();
            
            // Calculate ticks elapsed in this step, handling wraparound
            let ticks = if current > prev_count {
                // Counter wrapped around
                (prev_count as u32) + (0xFFFF - current as u32)
            } else {
                (prev_count - current) as u32
            };
            
            total_ticks += ticks;
            
            // Have we waited long enough?
            if total_ticks >= original_count as u32 {
                return Ok(());
            }
    
            prev_count = current;
            iterations += 1;
            core::hint::spin_loop();
        }
    
        serial_print!("PIT timed out after {} iterations\n", iterations);
        Err(ApicError::PitTimeout)
    }
}

/// Calibrate the APIC timer using PIT
/// Returns ticks per millisecond
pub fn calibrate_apic_timer() -> Result<u32, ApicError> {
    let mut pit = Pit::new();

    let mut lapic = unsafe {
        LocalApicBuilder::new()
            .set_xapic_base(xapic_base())
            .timer_vector(TIMER_VECTOR)
            .error_vector(ERROR_VECTOR)
            .spurious_vector(SPURIOUS_VECTOR)
            .timer_mode(TimerMode::OneShot)
            .timer_divide(TimerDivide::Div2)
            .build()
            .map_err(|_| ApicError::ApicInitFailed)?
    };

    unsafe {
        lapic.enable();
        lapic.enable_timer();
        lapic.set_timer_initial(0);

        // Calculate PIT ticks for 100ms calibration period
        let pit_ticks = ((PIT_FREQ as u64 * PIT_TICKS as u64) / 1000) as u16;

        // Configure and start PIT
        pit.configure_for_calibration(pit_ticks)?;

        // Start APIC timer with maximum value
        lapic.set_timer_initial(u32::MAX);

        pit.wait_for_completion(pit_ticks)?;

        let elapsed = u32::MAX - lapic.timer_current();

        let ticks_per_ms = elapsed / (PIT_TICKS as u32);

        // Sanity check - expect between 1K and 1M ticks per millisecond
        if ticks_per_ms < 1_000 || ticks_per_ms > 1_000_000 {
            return Err(ApicError::CalibrationFailed);
        }

        // Reset PIT to a known good state
        pit.mode_cmd.write(0x36); // Channel 0, LSB/MSB, mode 3
        pit.channel0.write(0);
        pit.channel0.write(0);

        Ok(ticks_per_ms)
    }
}

/// Configure timer for current CPU in periodic mode
pub fn configure_cpu_timer(vector: u8, ms: u32, ticks_per_ms: u32) -> Result<(), ApicError> {
    let ticks = ticks_per_ms
        .checked_mul(ms)
        .ok_or(ApicError::TimerOverflow)?;
    serial_print!("Programming timer with {} ticks\n", ticks);

    let mut lapic = unsafe {
        LocalApicBuilder::new()
            .set_xapic_base(xapic_base())
            .timer_vector(vector as usize)
            .error_vector(ERROR_VECTOR)
            .spurious_vector(SPURIOUS_VECTOR)
            .timer_mode(TimerMode::Periodic)
            .timer_divide(TimerDivide::Div2)
            .timer_initial(ticks)
            .build()
            .map_err(|_| ApicError::ApicInitFailed)?
    };

    unsafe {
        lapic.enable();
        lapic.enable_timer();
    }

    Ok(())
}

/// Configure timer for current CPU in one-shot mode
pub fn configure_oneshot_timer(vector: u8, ms: u32, ticks_per_ms: u32) -> Result<(), ApicError> {
    let ticks = ticks_per_ms
        .checked_mul(ms)
        .ok_or(ApicError::TimerOverflow)?;

    let mut lapic = unsafe {
        LocalApicBuilder::new()
            .set_xapic_base(xapic_base())
            .timer_vector(vector as usize)
            .error_vector(ERROR_VECTOR)
            .spurious_vector(SPURIOUS_VECTOR)
            .timer_mode(TimerMode::OneShot)
            .timer_divide(TimerDivide::Div2)
            .timer_initial(ticks)
            .build()
            .map_err(|_| ApicError::ApicInitFailed)?
    };

    unsafe {
        lapic.enable();
        lapic.enable_timer();
        lapic.set_timer_mode(TimerMode::OneShot);
        lapic.set_timer_divide(TimerDivide::Div2);
        lapic.set_timer_initial(ticks);
    }

    Ok(())
}

pub fn stop_cpu_timer() -> Result<(), ApicError> {
    let mut lapic = unsafe {
        LocalApicBuilder::new()
            .set_xapic_base(xapic_base())
            .timer_vector(TIMER_VECTOR)
            .error_vector(ERROR_VECTOR)
            .spurious_vector(SPURIOUS_VECTOR)
            .build()
            .map_err(|_| ApicError::ApicInitFailed)?
    };

    unsafe {
        lapic.set_timer_initial(0);
        lapic.disable_timer();
    }

    Ok(())
}
