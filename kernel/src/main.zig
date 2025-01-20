const builtin = @import("builtin");
const limine = @import("limine");
const std = @import("std");
const serial = @import("drivers/serial.zig");
const interrupts = @import("interrupts/idt.zig");
const kmalloc = @import("memory/allocator.zig");

pub export var framebuffer_request: limine.FramebufferRequest = .{};
pub export var smp_request: limine.SmpRequest = .{};

pub export var base_revision: limine.BaseRevision = .{ .revision = 3 };

var booted_cpus: u32 = 0;

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
        done();
    }

    // Ensure we eventually call done() for BSP as well
    done();
}

export fn _start() callconv(.C) noreturn {
    serial.println("Kernel starting...", .{});

    kmalloc.init();
    serial.println("Initializing interrupts...", .{});
    interrupts.init();
    _ = interrupts.enable_interrupts();

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
