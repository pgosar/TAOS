const std = @import("std");
const serial = @import("../drivers/serial.zig");

// Gate types
const INTERRUPT_GATE: u8 = 0x8E; // P=1, DPL=0, Type=0xE
const TRAP_GATE: u8 = 0x8F; // P=1, DPL=0, Type=0xF

// CPU state pushed by interrupt
const InterruptFrame = packed struct {
    // Additional registers saved by our stub
    r15: u64,
    r14: u64,
    r13: u64,
    r12: u64,
    r11: u64,
    r10: u64,
    r9: u64,
    r8: u64,
    rbp: u64,
    rdi: u64,
    rsi: u64,
    rdx: u64,
    rcx: u64,
    rbx: u64,
    rax: u64,

    // Interrupt number and error code
    interrupt_number: u64,
    error_code: u64,

    // Pushed by CPU automatically
    rip: u64,
    cs: u64,
    rflags: u64,
    rsp: u64,
    ss: u64,
};

// Define handler type
const HandlerFn = *const fn (*InterruptFrame) void;

const IDT_ENTRIES: usize = 256;

// IDT Entry Structure
const IdtEntry = packed struct {
    offset_low: u16,
    segment_selector: u16,
    ist: u3,
    reserved0: u5 = 0,
    gate_type: u4,
    reserved1: u1 = 0,
    dpl: u2,
    present: u1,
    offset_mid: u16,
    offset_high: u32,
    reserved2: u32 = 0,

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

var idt_entries: [IDT_ENTRIES]IdtEntry = undefined;
var idtr: Idtr = undefined;
var handlers: [IDT_ENTRIES]?HandlerFn = undefined;

// Array of assembly stubs
extern const interrupt_stubs: [256]u64;

// Common handler for all interrupts
export fn common_interrupt_handler(frame: *InterruptFrame) callconv(.C) void {
    const vector = frame.interrupt_number;

    if (handlers[vector]) |handler| {
        handler(frame);
    } else {
        serial.println("Unhandled interrupt {d} at RIP: 0x{X}", .{ vector, frame.rip });
        serial.println("Error code: 0x{X}", .{frame.error_code});
        hang();
    }
}

pub fn init() void {
    @memset(&handlers, null);
    @memset(std.mem.asBytes(&idt_entries), 0);

    // Set up all gates to point to their respective stubs
    for (0..IDT_ENTRIES) |i| {
        set_gate(@intCast(i));
    }

    // Register default handlers
    register_handler(0, exception_divide_by_zero);
    register_handler(1, exception_debug);
    register_handler(2, exception_non_maskable);
    register_handler(3, exception_breakpoint);
    register_handler(14, page_fault_handler);

    // Set up the IDTR
    idtr.limit = @sizeOf(@TypeOf(idt_entries)) - 1;
    idtr.base = @intFromPtr(&idt_entries);

    load_idt();

    serial.println("IDT initialized", .{});
}

fn set_gate(n: u8) void {
    var entry = &idt_entries[n];
    entry.segment_selector = 0x28; // Kernel code segment
    entry.ist = 0; // Don't use IST
    entry.set_flags(INTERRUPT_GATE);
    entry.set_offset(interrupt_stubs[n]);
}

pub fn register_handler(vector: u8, handler: HandlerFn) void {
    handlers[vector] = handler;
}

fn load_idt() void {
    asm volatile ("lidt (%[idtr])"
        :
        : [idtr] "r" (&idtr),
    );
}

// Example handlers
fn exception_divide_by_zero(frame: *InterruptFrame) void {
    serial.println("Divide by zero exception at RIP: 0x{X}", .{frame.rip});
    hang();
}

fn exception_debug(frame: *InterruptFrame) void {
    serial.println("Debug exception at RIP: 0x{X}", .{frame.rip});
    hang();
}

fn exception_non_maskable(frame: *InterruptFrame) void {
    serial.println("Non-maskable interrupt at RIP: 0x{X}", .{frame.rip});
    hang();
}

fn exception_breakpoint(frame: *InterruptFrame) void {
    serial.println("Breakpoint exception at RIP: 0x{X}", .{frame.rip});
}

fn page_fault_handler(frame: *InterruptFrame) void {
    serial.println("Page fault at RIP: 0x{X}", .{frame.rip});
    serial.println("Error code: 0x{X}", .{frame.error_code});
    // Error code bits for page fault:
    // bit 0: Present (0=non-present page, 1=page protection)
    // bit 1: Write (0=read, 1=write)
    // bit 2: User (0=supervisor, 1=user)
    // bit 3: Reserved write (0=not a reserved bit, 1=reserved bit violation)
    // bit 4: Instruction Fetch (0=data access, 1=instruction fetch)
    const present = frame.error_code & 1;
    const write = (frame.error_code >> 1) & 1;
    const user = (frame.error_code >> 2) & 1;
    serial.println("  Present: {}, Write: {}, User: {}", .{ present, write, user });
    hang();
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
