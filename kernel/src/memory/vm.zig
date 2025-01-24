const std = @import("std");
const limine = @import("limine");
const serial = @import("../drivers/serial.zig");
const allocator = @import("./allocator.zig");

pub fn init() void {
    // get memory map from limine
    const memmap = allocator.memmap_request.response orelse {
        @panic("No memory map provided by bootloader");
    };

    // Find the size of physmem that we care about
    var usable_physmem: ?*const limine.MemoryMapEntry = null;
    var usable_physmem_size: u64 = 0;

    for (memmap.entries()) |entry| {
        if (entry.kind == .usable) {
            usable_physmem = entry;
            usable_physmem_size += entry.length;
        }
    }

    serial.println("The size is {}", .{usable_physmem_size});
}
