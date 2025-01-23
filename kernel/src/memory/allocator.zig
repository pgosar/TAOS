// Todo: Replace this
// This kind of doesn't dealloc until you clean all of it.
const std = @import("std");
const limine = @import("limine");
const serial = @import("../drivers/serial.zig");

pub export var memmap_request: limine.MemoryMapRequest = .{};

// We'll store our heap info
var heap_buffer: []u8 = undefined;
var arena: std.heap.ArenaAllocator = undefined;
pub var allocator: std.mem.Allocator = undefined;

pub fn init() void {
    serial.println("Initializing kernel allocator...", .{});

    // Get memory map from Limine
    const memmap = memmap_request.response orelse {
        @panic("No memory map provided by bootloader");
    };

    // Find largest usable region for our heap
    var largest_region: ?*const limine.MemoryMapEntry = null;
    var largest_size: u64 = 0;

    for (memmap.entries()) |entry| {
        if (entry.kind == .usable and entry.length > largest_size) {
            largest_size = entry.length;
            largest_region = entry;
        }
    }

    const region = largest_region orelse {
        @panic("No usable memory regions found");
    };

    // Use 25% of the largest region or 32MB, whichever is smaller
    const heap_size = @min(largest_size / 4, 32 * 1024 * 1024);

    // Create our heap buffer in the usable region
    heap_buffer = @as([*]u8, @ptrFromInt(region.base))[0..heap_size];

    // Initialize the arena with a FixedBufferAllocator
    var fba = std.heap.FixedBufferAllocator.init(heap_buffer);
    arena = std.heap.ArenaAllocator.init(fba.allocator());
    allocator = arena.allocator();

    serial.println("Kernel allocator initialized with {}KB heap at 0x{X}", .{
        heap_size / 1024,
        region.base,
    });
}

pub fn deinit() void {
    arena.deinit();
}
