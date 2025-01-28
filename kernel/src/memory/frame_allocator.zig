const std = @import("std");
const limine = @import("limine");
const serial = @import("../drivers/serial.zig");
const allocator = @import("./allocator.zig");

// const PAGE_SIZE = 4096;
// const BITMAP_SIZE = MAX_FRAMES / 8; // One bit per frame
extern var _kernel_end: u8;
extern var _kernel_start: u8;

pub export var hhdm_request: limine.HhdmRequest = .{};
pub export var executable_address_request: limine.KernelAddressRequest = .{};
// pub export var paging_mode_request: limine.PagingModeRequest = .{};

const max_kernel_size: u64 = 0x8000000;

// from top vmem address, we subtract space for mapping (physical memory size), we also subtract space for the kernel (max_kernel_size)

const FrameAllocator = packed struct {
    physical_usable_memory_start: u64,
    physical_memory_size: u64,
    // top vmem address - physical memory size (for mapping) - kernel size
    virtual_kernel_space_start: u64,
    bitmap: [4096]u64,
};

pub fn init() void {
    // get memory map from limine
    const memmap = allocator.memmap_request.response orelse {
        @panic("No memory map provided by bootloader");
    };

    // Find the size of physmem that we care about
    // var usable_physmem: ?*const limine.MemoryMapEntry = null;
    var usable_physmem_size: u64 = 0;

    serial.println("Memmap entries:", .{});
    for (memmap.entries()) |entry| {
        serial.println("Entry kind: {}, Entry start: {X}, Entry length: {X}", .{ entry.kind, entry.base, entry.length });
        if (entry.kind == .usable) {
            usable_physmem_size += entry.length;
        }
    }

    const HHDM_BASE: u64 = @intFromPtr(&_kernel_end);
    const kernel_start: u64 = @intFromPtr(&_kernel_start);
    const kernel_size: u64 = HHDM_BASE - kernel_start;
    serial.println("Kernel start is {X}, kernel end is {X}, kernel size is {X}", .{ kernel_start, HHDM_BASE, kernel_size });

    const hhdm = hhdm_request.response orelse {
        @panic("HHDM Failed");
    };

    serial.println("The HHDM offset by Limine is 0x{X}", .{hhdm.offset});

    const kernel_address = executable_address_request.response orelse {
        @panic("Could not get kernel address");
    };

    serial.println("The kernel is at VA: {X} and PA: {X}", .{ kernel_address.virtual_base, kernel_address.physical_base });
}
