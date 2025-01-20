const std = @import("std");
const serial = @import("../drivers/serial.zig");

// Gate types
const INTERRUPT_GATE: u8 = 0x8E; // P=1, DPL=0, Type=0xE
const TRAP_GATE: u8 = 0x8F; // P=1, DPL=0, Type=0xF

// CPU state pushed by interrupt
const InterruptFrame = packed struct {
    // Pushed by CPU automatically
    rip: u64,
    cs: u64,
    rflags: u64,
    rsp: u64,
    ss: u64,
};

// CPU state with error code
const InterruptFrameWithError = packed struct {
    error_code: u64,
    rip: u64,
    cs: u64,
    rflags: u64,
    rsp: u64,
    ss: u64,
};

// Define handler types
const InterruptHandler = *const fn (*InterruptFrame) callconv(.Interrupt) void;
const InterruptHandlerWithError = *const fn (*InterruptFrameWithError) callconv(.Interrupt) void;

// IDT Entry Structure (16 bytes)
const IdtEntry = packed struct {
    offset_low: u16, // Lower 16 bits of handler address
    segment_selector: u16, // Code segment selector
    ist: u3, // Interrupt Stack Table offset
    reserved0: u5 = 0, // Reserved, must be 0
    gate_type: u4, // Gate type (0xE for interrupt, 0xF for trap)
    reserved1: u1 = 0, // Reserved, must be 0
    dpl: u2, // Descriptor Privilege Level
    present: u1, // Present bit
    offset_mid: u16, // Middle 16 bits of handler address
    offset_high: u32, // Upper 32 bits of handler address
    reserved2: u32 = 0, // Reserved, must be 0

    pub fn set_offset(self: *IdtEntry, offset: u64) void {
        self.offset_low = @truncate(offset & 0xFFFF);
        self.offset_mid = @truncate((offset >> 16) & 0xFFFF);
        self.offset_high = @truncate((offset >> 32) & 0xFFFFFFFF);
    }

    pub fn set_flags(self: *IdtEntry, flags: u8) void {
        self.gate_type = @truncate(flags & 0xF);
        self.dpl = @truncate((flags >> 5) & 0x3);
        self.present = @truncate((flags >> 7) & 0x1);
    }
};

const Idtr = packed struct {
    limit: u16,
    base: u64,
};

const pushes_error = [32]bool{
    false, // 0: Divide by zero
    false, // 1: Debug
    false, // 2: NMI
    false, // 3: Breakpoint
    false, // 4: Overflow
    false, // 5: Bound range exceeded
    false, // 6: Invalid opcode
    false, // 7: Device not available
    true, // 8: Double fault
    false, // 9: Coprocessor segment overrun (reserved)
    true, // 10: Invalid TSS
    true, // 11: Segment not present
    true, // 12: Stack segment fault
    true, // 13: General protection fault
    true, // 14: Page fault
    false, // 15: Reserved
    false, // 16: x87 FPU error
    true, // 17: Alignment check
    false, // 18: Machine check
    false, // 19: SIMD floating point
    false, // 20: Virtualization
    true, // 21: Control protection
    false, // 22: Reserved
    false, // 23: Reserved
    false, // 24: Reserved
    false, // 25: Reserved
    false, // 26: Reserved
    false, // 27: Reserved
    false, // 28: Reserved
    false, // 29: Reserved
    true, // 30: Security exception
    false, // 31: Reserved
};

const IDT_ENTRIES: usize = 256;

var idt_entries: [IDT_ENTRIES]IdtEntry = undefined;
var idtr: Idtr = undefined;

pub fn init() void {
    serial.println("Initializing IDT...", .{});

    @memset(std.mem.asBytes(&idt_entries), 0);

    set_gate(0, exception_divide_by_zero, INTERRUPT_GATE); // Divide by zero
    set_gate(1, exception_debug, INTERRUPT_GATE); // Debug
    set_gate(2, exception_non_maskable, INTERRUPT_GATE); // NMI
    set_gate(3, exception_hw_breakpoint, INTERRUPT_GATE); // Breakpoint
    // ... set up other exception handlers ...

    // Set up the IDTR
    idtr.limit = @sizeOf(@TypeOf(idt_entries)) - 1;
    idtr.base = @intFromPtr(&idt_entries);

    // Load the IDTR
    load_idt();

    serial.println("IDT initialized", .{});
}

fn set_gate(n: u8, handler: InterruptHandler, flags: u8) void {
    var entry = &idt_entries[n];
    entry.segment_selector = 0x28; // Kernel code segment
    entry.ist = 0; // Don't use IST
    entry.set_flags(flags);
    entry.set_offset(@intFromPtr(handler));
}

fn load_idt() void {
    asm volatile ("lidt (%[idtr])"
        :
        : [idtr] "r" (&idtr),
    );
}

fn exception_divide_by_zero(frame: *InterruptFrame) callconv(.Interrupt) void {
    serial.println("Divide by zero exception at RIP: 0x{X}", .{frame.rip});
    hang();
}

fn exception_debug(frame: *InterruptFrame) callconv(.Interrupt) void {
    serial.println("Debug exception at RIP: 0x{X}", .{frame.rip});
    hang();
}

fn exception_non_maskable(frame: *InterruptFrame) callconv(.Interrupt) void {
    serial.println("Non-maskable interrupt at RIP: 0x{X}", .{frame.rip});
    hang();
}

fn exception_hw_breakpoint(frame: *InterruptFrame) callconv(.Interrupt) void {
    serial.println("Breakpoint exception at RIP: 0x{X}", .{frame.rip});
}

fn hang() noreturn {
    while (true) {
        asm volatile ("cli; hlt");
    }
}

pub fn enable_interrupts() void {
    asm volatile ("sti");
}

pub fn disable_interrupts() void {
    asm volatile ("cli");
}
