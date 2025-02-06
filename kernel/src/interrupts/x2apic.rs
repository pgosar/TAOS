use raw_cpuid::CpuId;
use x86_64::instructions::port::Port;
use x86_64::registers::model_specific::Msr;

// MSR register constants
const IA32_APIC_BASE_MSR: u32 = 0x1B;
const X2APIC_EOI: u32 = 0x80B;
const X2APIC_SIVR: u32 = 0x80F;
const X2APIC_TPR: u32 = 0x808;
const X2APIC_ID: u32 = 0x802;
const X2APIC_ICR: u32 = 0x830;
const X2APIC_LVT_TIMER: u32 = 0x832;
const X2APIC_TIMER_ICR: u32 = 0x838;
const X2APIC_TIMER_CCR: u32 = 0x839;
const X2APIC_TIMER_DCR: u32 = 0x83E;

// PIT constants
const PIT_FREQUENCY: u64 = 1_193_182;
const CHANNEL_2_PORT: u16 = 0x42;
const COMMAND_PORT: u16 = 0x43;
const CONTROL_PORT: u16 = 0x61;

pub const TIMER_VECTOR: u8 = 32;

#[derive(Debug)]
pub enum X2ApicError {
    NotSupported,
    NotEnabled,
    EnableFailed,
    InvalidVector,
    ConfigurationFailed,
    TimerError,
}

#[derive(Debug)]
pub enum PitError {
    InvalidDuration,
    CalibrationFailed,
}

pub struct X2Apic {
    enabled: bool,
}

impl X2Apic {
    fn read_msr(reg: u32) -> u64 {
        unsafe { Msr::new(reg).read() }
    }

    fn write_msr(reg: u32, value: u64) {
        unsafe { Msr::new(reg).write(value) }
    }

    pub fn new() -> Result<Self, X2ApicError> {
        let cpuid = CpuId::new();
        if !cpuid.get_feature_info().map_or(false, |f| f.has_x2apic()) {
            return Err(X2ApicError::NotSupported);
        }
        Ok(Self { enabled: false })
    }

    pub fn enable(&mut self) -> Result<(), X2ApicError> {
        if self.enabled {
            return Ok(());
        }

        let value = Self::read_msr(IA32_APIC_BASE_MSR);
        Self::write_msr(IA32_APIC_BASE_MSR, value | (1 << 11) | (1 << 10));

        let new_value = Self::read_msr(IA32_APIC_BASE_MSR);
        if (new_value & ((1 << 11) | (1 << 10))) != ((1 << 11) | (1 << 10)) {
            return Err(X2ApicError::EnableFailed);
        }

        self.enabled = true;
        self.init_default_config()
    }

    fn init_default_config(&self) -> Result<(), X2ApicError> {
        Self::write_msr(X2APIC_SIVR, 0xFF | (1 << 8));
        Self::write_msr(X2APIC_TPR, 0);
        Ok(())
    }

    pub fn send_eoi(&self) -> Result<(), X2ApicError> {
        if !self.enabled {
            return Err(X2ApicError::NotEnabled);
        }
        Self::write_msr(X2APIC_EOI, 0);
        Ok(())
    }

    pub fn get_id(&self) -> Result<u32, X2ApicError> {
        if !self.enabled {
            return Err(X2ApicError::NotEnabled);
        }
        Ok(Self::read_msr(X2APIC_ID) as u32)
    }

    pub fn send_ipi(&self, target_id: u32, vector: u8) -> Result<(), X2ApicError> {
        if !self.enabled {
            return Err(X2ApicError::NotEnabled);
        }

        if vector < 16 {
            return Err(X2ApicError::InvalidVector);
        }

        let value = ((target_id as u64) << 32) | vector as u64;
        Self::write_msr(X2APIC_ICR, value);
        Ok(())
    }

    pub fn configure_timer(&mut self, hz: u32) -> Result<(), X2ApicError> {
        if !self.enabled {
            return Err(X2ApicError::NotEnabled);
        }

        let mut pit = Pit::new();
        let counter = pit
            .calibrate_apic_timer(hz, self)
            .map_err(|_| X2ApicError::TimerError)?;

        // Configure timer: Periodic mode (1 << 17), unmasked (0 << 16), vector 32
        let timer_config = (1u64 << 17) | (TIMER_VECTOR as u64);
        Self::write_msr(X2APIC_LVT_TIMER, timer_config);

        // Set divider to 1 (value 0xB)
        Self::write_msr(X2APIC_TIMER_DCR, 0xB);

        self.set_timer_initial_count(counter)?;

        Ok(())
    }

    pub fn set_timer_initial_count(&self, count: u32) -> Result<(), X2ApicError> {
        if !self.enabled {
            return Err(X2ApicError::NotEnabled);
        }

        Self::write_msr(X2APIC_TIMER_ICR, count as u64);
        Ok(())
    }

    pub fn read_timer_count(&self) -> Result<u32, X2ApicError> {
        if !self.enabled {
            return Err(X2ApicError::NotEnabled);
        }

        Ok(Self::read_msr(X2APIC_TIMER_CCR) as u32)
    }

    pub fn mask_timer_interrupts(&self) -> Result<(), X2ApicError> {
        if !self.enabled {
            return Err(X2ApicError::NotEnabled);
        }
        Self::write_msr(
            X2APIC_LVT_TIMER,
            Self::read_msr(X2APIC_LVT_TIMER) | (1 << 16),
        );
        Ok(())
    }

    pub fn unmask_timer_interrupts(&self) -> Result<(), X2ApicError> {
        if !self.enabled {
            return Err(X2ApicError::NotEnabled);
        }
        Self::write_msr(
            X2APIC_LVT_TIMER,
            Self::read_msr(X2APIC_LVT_TIMER) & !(1 << 16),
        );
        Ok(())
    }

    pub fn read_svr(&self) -> Result<u64, X2ApicError> {
        if !self.enabled {
            return Err(X2ApicError::NotEnabled);
        }
        Ok(Self::read_msr(X2APIC_SIVR))
    }
}

pub struct Pit {
    channel_2: Port<u8>,
    command: Port<u8>,
    control: Port<u8>,
}

impl Pit {
    pub fn new() -> Self {
        Self {
            channel_2: Port::new(CHANNEL_2_PORT),
            command: Port::new(COMMAND_PORT),
            control: Port::new(CONTROL_PORT),
        }
    }

    pub fn calibrate_apic_timer(&mut self, hz: u32, apic: &mut X2Apic) -> Result<u32, PitError> {
        // First configure x2APIC timer as oneshot and masked
        apic.mask_timer_interrupts()
            .map_err(|_| PitError::CalibrationFailed)?;

        // Set divider to 1 (value 0xB)
        unsafe { Msr::new(X2APIC_TIMER_DCR).write(0xB) };

        // Set initial APIC count
        let initial = u32::MAX;
        apic.set_timer_initial_count(initial)
            .map_err(|_| PitError::CalibrationFailed)?;

        unsafe {
            // Speaker off, gate on
            self.control.write(1);

            // Configure channel 2:
            // 10 = channel 2
            // 11 = lobyte/hibyte
            // 011 = square wave
            // 0 = binary counting
            self.command.write(0b10110110);

            // Write PIT divider
            let pit_divider = PIT_FREQUENCY as u32 / 20;
            if pit_divider > 0xFFFF {
                return Err(PitError::InvalidDuration);
            }

            self.channel_2.write((pit_divider & 0xFF) as u8);
            self.channel_2.write((pit_divider >> 8) as u8);

            // Count 40 changes (full second since square wave makes it twice as fast)
            let mut last = self.control.read() & 0x20;
            let mut changes = 0;

            while changes < 40 {
                let t = self.control.read() & 0x20;
                if t != last {
                    changes += 1;
                    last = t;
                }
            }

            // Stop the PIT
            self.control.write(0);
        }

        // Calculate how many APIC ticks occurred
        let final_count = apic
            .read_timer_count()
            .map_err(|_| PitError::CalibrationFailed)?;
        let diff = initial - final_count;

        // Calculate counter needed for desired frequency
        let apic_counter = diff / hz;

        Ok(apic_counter)
    }
}
