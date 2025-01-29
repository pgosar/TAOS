// A general purpose bitmap object that can be statically or dynamically allocated
const std = @import("std");
const lib = @import("../lib.zig");
const Allocator = std.mem.Allocator;

pub const BitmapError = error{
    OutOfBounds,
    BitmapFull,
};

pub fn Bitmap(comptime total_entries: ?u64, comptime BitmapType: type) type {
    return struct {
        const Self = @This();
        const static = total_entries != null;

        pub const ENTRIES_IN_ONE_VALUE: u64 = @bitSizeOf(BitmapType);

        total_entries: u64,
        free_entries: u64,
        bitmap: if (static) [
            std.mem.alignForward(u64, total_entries, ENTRIES_IN_ONE_VALUE) / ENTRIES_IN_ONE_VALUE
        ]BitmapType else []BitmapType,
        allocator: if (static) ?Allocator else Allocator,
        // this is an optimization for some use cases
        index_last_accessed: u64,

        pub fn init(num_bits: if (static) ?u64 else u64, allocator: if (static) ?Allocator else Allocator) !Self {
            if (static) {
                return Self{
                    .total_entries = total_entries.?,
                    .free_entries = total_entries.?,
                    .bitmap = [_]BitmapType{0} ** (std.mem.alignForward(u64, total_entries.?, ENTRIES_IN_ONE_VALUE) / ENTRIES_IN_ONE_VALUE),
                    .index_last_accessed = 0,
                    .allocator = null,
                };
            } else {
                const self = Self{
                    .total_entries = num_bits,
                    .free_entries = num_bits,
                    .bitmap = try allocator.alloc(BitmapType, std.mem.alignForward(u64, num_bits, ENTRIES_IN_ONE_VALUE) / ENTRIES_IN_ONE_VALUE),
                    .index_last_accessed = 0,
                    .allocator = allocator,
                };

                for (self.bitmap) |*bmp| {
                    bmp.* = 0;
                }

                return self;
            }
        }

        // Internal functions

        fn offsetInIndex(i: usize) BitmapType {
            return @as(BitmapType, 1) << @intCast(i % ENTRIES_IN_ONE_VALUE);
        }

        // this function won't error
        pub fn isFreeSafe(self: *Self, i: usize) bool {
            return self.bitmap[i / ENTRIES_IN_ONE_VALUE] & offsetInIndex(i) == 0;
        }

        // Exposed functions

        // to deallocate if dynamically allocated
        pub fn freeBitmap(self: *Self) void {
            if (!static) self.allocator.free(self.bitmap);
        }

        // sets a specific bit
        pub fn setEntry(self: *Self, i: usize, value: lib.BitMapAllocationStatus) BitmapError!void {
            if (i > self.total_entries) {
                return BitmapError.OutOfBounds;
            }

            // set the specific bit in the BitmapType we load
            const full = offsetInIndex(i);
            if (value == lib.BitMapAllocationStatus.ALLOCATED) {
                self.bitmap[i / ENTRIES_IN_ONE_VALUE] |= full;
                self.free_entries -= 1;
            } else if (value == lib.BitMapAllocationStatus.FREE) {
                self.bitmap[i / ENTRIES_IN_ONE_VALUE] &= ~full;
                self.free_entries += 1;
            }
        }

        pub fn isSet(self: *Self, i: usize) BitmapError!bool {
            if (i > self.total_entries) {
                return BitmapError.OutOfBounds;
            }

            return self.bitmap[i / ENTRIES_IN_ONE_VALUE] & offsetInIndex(i) == 1;
        }

        // debug funtion to get the size of the bitmap by walking through the entire thing
        pub fn getBitmapSizeDirty(self: *Self) u64 {
            return self.bitmap.len * @bitSizeOf(BitmapType);
        }

        pub fn findFirstFree(self: *Self) BitmapError!u64 {
            if (self.free_entries == 0) return BitmapError.BitmapFull;

            while (!isFreeSafe(self, self.index_last_accessed)) {
                self.index_last_accessed = (self.index_last_accessed + 1) % self.total_entries;
            }

            return self.index_last_accessed;
        }

        pub fn setContiguous(self: *Self, i: usize, size: usize, value: lib.BitMapAllocationStatus) BitmapError!void {
            // find the first free contiguous entries and set them to one
            for (0..size) |j| {
                try setEntry(self, (i + j), value);
            }
        }
    };
}