const std = @import("std");
const limine = @import("limine");
const serial = @import("../drivers/serial.zig");
const allocator = @import("./allocator.zig");

// const PAGE_SIZE = 4096;
// const BITMAP_SIZE = MAX_FRAMES / 8; // One bit per frame
//

extern var _kernel_end: u8;

fn getKernelEndAddress() usize {
    return @intFromPtr(&_kernel_end);
}

pub fn init() void {
    // get memory map from limine
    const memmap = allocator.memmap_request.response orelse {
        @panic("No memory map provided by bootloader");
    };

    // Find the size of physmem that we care about
    // var usable_physmem: ?*const limine.MemoryMapEntry = null;
    var usable_physmem_size: u64 = 0;

    for (memmap.entries()) |entry| {
        if (entry.kind == .usable) {
            usable_physmem_size += entry.length;
        }
    }

    serial.println("The physical memory size is {}", .{usable_physmem_size});

    serial.println("The kernel ends at {X}", .{getKernelEndAddress()});
}

// Initialize the physical frame allocator
// pub fn init_physical_allocator(memory_map_start: usize, memory_map_end: usize) void {
//
//     // get memory map from limine
//     const memmap = allocator.memmap_request.response orelse {
//         @panic("No memory map provided by bootloader");
//     };
//
//     // Find the size of physmem that we care about
//     // var usable_physmem: ?*const limine.MemoryMapEntry = null;
//     var usable_physmem_size: u64 = 0;
//
//     for (memmap.entries()) |entry| {
//         if (entry.kind == .usable) {
//             serial.println("The base size is {}", .{entry.base});
//             usable_physmem_size += entry.length;
//         }
//     }
//
//     serial.println("The physical memory size is {}", .{usable_physmem_size});
//

//
// std.debug.print("Initializing physical frame allocator...\n", .{});
//
// // Example: Assume usable memory starts right after the kernel
// memory_start = memory_map_start;
// const usable_memory_size = memory_map_end - memory_map_start;
//
// // Calculate total frames
// total_frames = usable_memory_size / PAGE_SIZE;
//
// // Clear the bitmap
// std.mem.set(u8, &bitmap, 0);
// }

// Allocate a free frame
// pub fn alloc_frame() ?usize {
//     for (bitmap) |byte, i| {
//         if (byte != 0xFF) { // Check if there are free frames in this byte
//             for (0..8) |bit| {
//                 if ((byte & (1 << bit)) == 0) { // Free frame found
//                     bitmap[i] |= (1 << bit); // Mark frame as allocated
//                     const frame_index = i * 8 + bit;
//                     if (frame_index >= total_frames) {
//                         return null; // Out of frames
//                     }
//                     return memory_start + frame_index * PAGE_SIZE;
//                 }
//             }
//         }
//     }
//     return null; // No free frames
// }

// Free a previously allocated frame
// pub fn free_frame(frame_addr: usize) void {
//     const frame_index = (frame_addr - memory_start) / PAGE_SIZE;
//     const byte_index = frame_index / 8;
//     const bit_index = frame_index % 8;
//
//     if (frame_index < total_frames) {
//         bitmap[byte_index] &= ~(1 << bit_index); // Mark frame as free
//     }
// }
