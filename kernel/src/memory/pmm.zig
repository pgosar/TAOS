const std = @import("std");
const limine = @import("limine");
const serial = @import("../drivers/serial.zig");
const allocator = @import("./allocator.zig");
const lib = @import("../lib.zig");
const bitmap = @import("../lib/bitmap.zig");

// const PAGE_SIZE = 4096;
// const BITMAP_SIZE = MAX_FRAMES / 8; // One bit per frame
extern var _kernel_end: u8;
extern var _kernel_start: u8;

pub export var hhdm_request: limine.HhdmRequest = .{};
pub export var executable_address_request: limine.KernelAddressRequest = .{};
// pub export var paging_mode_request: limine.PagingModeRequest = .{};

const max_kernel_size: u64 = 0x8000000;

// Current design is: Find the maximum size of physical memory,
// create a bitmap that represents that size, and worry about
// the kernel and other reserved sections of memory later
// (essentially, pinning)
const FrameAllocator = packed struct {
    physical_usable_memory_start: u64,
    physical_memory_size: u64,
    // top vmem address - physical memory size (for mapping) - kernel size
    virtual_kernel_space_start: u64,
    bitmap: []u64,
};

pub fn init() void {
    // get memory map from limine
    const memmap = allocator.memmap_request.response orelse {
        @panic("No memory map provided by bootloader");
    };

    const kernel_end: u64 = @intFromPtr(&_kernel_end);
    const kernel_start: u64 = @intFromPtr(&_kernel_start);
    const kernel_size: u64 = kernel_end - kernel_start;
    serial.println("Kernel start is {X}, kernel end is {X}, kernel size is {X}", .{ kernel_start, kernel_end, kernel_size });

    const hhdm = hhdm_request.response orelse {
        @panic("HHDM Failed");
    };

    serial.println("The HHDM offset by Limine is 0x{X}", .{hhdm.offset});

    const kernel_address = executable_address_request.response orelse {
        @panic("Could not get kernel address");
    };

    serial.println("The kernel is at VA: {X} and PA: {X}", .{ kernel_address.virtual_base, kernel_address.physical_base });

    // we have to figure out where to place the bitmap in memory. for now, we will say this is above where the kernel ends.
    // use limine to figure out kernel's end in physical memory, and HHDM offset to get that in virtual memory
    // const kernel_end_virtual = kernel_address.physical_base + kernel_size + hhdm.offset + 1;
    // const bitmap_start_aligned = (kernel_end_virtual + (lib.PAGE_SIZE - 1)) & ~(lib.PAGE_SIZE - 1);
    // const bitmap_start_aligned_ptr: *u64 = @ptrFromInt(bitmap_start_aligned);

    // go through memmap entries and find the last physical address
    var max_physical_address: u64 = 0;
    for (memmap.entries()) |entry| {
        max_physical_address = @max(max_physical_address, entry.base + entry.length - 1);
    }

    serial.println("The size of physical memory is: {}", .{max_physical_address});

    // const total_frames: usize = (max_physical_address / lib.PAGE_SIZE) + 1;

    const FrameBitmap = bitmap.Bitmap(null, u64);

    const buffer: *[4096]u8 = @ptrFromInt(kernel_end);
    var fixed_alloc = std.heap.FixedBufferAllocator.init(buffer);
    const my_alloc = fixed_alloc.allocator();

    var frame_bitmap = FrameBitmap.init(5023, my_alloc) catch |err| {
        serial.println("Test bitmap error: {}", .{err});
        @panic("Could not init TestBitmap");
    };

    // testing: set first bit in bitmap to 1 such that findFirstFree returns 1
    frame_bitmap.setEntry(0, 1) catch |err| switch (err) {
        bitmap.BitmapError.OutOfBounds => @panic("Index out of bounds"),
        else => @panic("Unknown error occurred"),
    };
    const first_free_index = frame_bitmap.findFirstFree() catch |err| switch (err) {
        bitmap.BitmapError.BitmapFull => @panic("Bitmap is full!"),
        else => @panic("Unknowkn error occurred"),
    };

    serial.println("First free index {}", .{first_free_index});

    // TODO: I think the frame allocator should be placed after
    // the kernel in physical memory (using HHDM offset)
    // instead of where we are allocating right now
}
