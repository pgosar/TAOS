const builtin = @import("builtin");
const limine = @import("limine");
const std = @import("std");
const serial = @import("drivers/serial.zig");
const idt = @import("interrupts/idt.zig");
const gdt = @import("interrupts/gdt.zig");
const kmalloc = @import("memory/allocator.zig");
const lib = @import("lib.zig");

extern fn load_tss(u32) void;
extern fn reload_segments() void;

pub export var framebuffer_request: limine.FramebufferRequest = .{};
pub export var smp_request: limine.SmpRequest = .{};

pub export var base_revision: limine.BaseRevision = .{ .revision = 3 };

var booted_cpus: u32 = 0;

const MAX_NUM_CORES: u32 = 16;

inline fn done() noreturn {
    while (true) {
        asm volatile ("hlt");
    }
}

// Called per core
fn smp_entry(info: *limine.SmpInfo) callconv(.C) noreturn {
    _ = @atomicRmw(u32, &booted_cpus, .Add, 1, .monotonic);

    // If this is not the BSP (Bootstrap Processor), just halt
    if (info.lapic_id != smp_request.response.?.bsp_lapic_id) {
        gdt.init(info.processor_id);
        idt.init();
        idt.enable_interrupts();
        done();
    }

    // Ensure we eventually call done() for BSP as well
    done();
}

export fn _start() callconv(.C) noreturn {

    serial.println("Kernel starting...", .{});
    if (smp_request.response) |smp_response| {
        const cpu_count = smp_response.cpu_count;
        if (cpu_count > lib.MAX_NUM_CORES) {
            serial.println("Machine has more cores than supported. OS supports up to {} cores.", .{lib.MAX_NUM_CORES});
        }
        serial.println("Initializing GDT and TSS...", .{});
        gdt.init(0);
    }
    else {
        serial.println("Cannot request how many cores machine has.", .{});
        unreachable;
    }

    kmalloc.init();


    serial.println("Initializing interrupts...", .{});
    idt.init();

    idt.enable_interrupts();

    serial.println("Testing breakpoint interrupt...", .{});
    asm volatile ("int3");

    if (!base_revision.is_supported()) {
        serial.println("Unsupported Limine protocol version", .{});
        done();
    }

    if (smp_request.response) |smp_response| {
        const cpu_count = smp_response.cpu_count;
        serial.println("Found {d} cores", .{cpu_count});

        for (0..cpu_count) |i| {
            const cpu_info = smp_response.cpus()[i];

            if (cpu_info.lapic_id == smp_response.bsp_lapic_id) {
                continue;
            }

            smp_response.cpus()[i].goto_address = smp_entry;
        }
    }

    if (framebuffer_request.response) |framebuffer_response| {
        if (framebuffer_response.framebuffer_count < 1) {
            serial.println("No framebuffer available", .{});
            done();
        }

        const framebuffer = framebuffer_response.framebuffers()[0];

        for (0..100) |i| {
            const pixel_offset = i * framebuffer.pitch + i * 4;
            @as(*u32, @ptrCast(@alignCast(framebuffer.address + pixel_offset))).* = 0xFFFFFFFF;
        }
    }

    smp_entry(smp_request.response.?.cpus()[0]);
}
