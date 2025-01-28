// A general purpose bitmap object that can be statically or dynamically allocated
const std = @import("std");
const Allocator = std.mem.Allocator;

pub const BitmapError = error{
    OutOfBounds,
};

pub fn Bitmap(comptime total_entries: ?u64, comptime BitmapType: type) type {
    return struct {
        const Self = @This();
        const static = total_entries != null;

        pub const ENTRIES_IN_ONE_VALUE: u64 = @bitSizeOf(BitmapType);

        total_entries: u64,
        free_entries: u64,
        bitmap: if (static) [
            std.mem.alignForward(u64, total_entries.?, ENTRIES_IN_ONE_VALUE) / ENTRIES_IN_ONE_VALUE
        ]BitmapType else []BitmapType,
        allocator: if (static) ?Allocator else Allocator,

        pub fn init(num_bits: if (static) ?u64 else u64, allocator: if (static) ?Allocator else Allocator) !Self {
            if (static) {
                return Self{
                    .total_entries = total_entries.?,
                    .free_entries = total_entries.?,
                    .bitmap = [_]BitmapType{0} ** (std.mem.alignForward(u64, total_entries.?, ENTRIES_IN_ONE_VALUE) / ENTRIES_IN_ONE_VALUE),
                    .allocator = null,
                };
            } else {
                const self = Self{
                    .total_entries = num_bits,
                    .free_entries = num_bits,
                    .bitmap = try allocator.alloc(BitmapType, std.mem.alignForward(u64, num_bits, ENTRIES_IN_ONE_VALUE) / ENTRIES_IN_ONE_VALUE),
                    .allocator = allocator,
                };

                for (self.bitmap) |*bmp| {
                    bmp.* = 0;
                }

                return self;
            }
        }

        // Internal functions

        fn iToInt(i: usize) BitmapType {
            return @intCast(i % ENTRIES_IN_ONE_VALUE);
        }

        // Exposed functions

        // sets a specific bit
        pub fn setEntry(self: *Self, i: usize, value: u1) BitmapError!void {
            if (i > self.total_entries) {
                return BitmapError.OutOfBounds;
            }

            const full = iToInt(i);
            self.bitmap[i % ENTRIES_IN_ONE_VALUE] = (full | value);
            self.free_entries -= 1;
        }

        pub fn free_bitmap(self: *Self) void {
            if (!static) self.allocator.free(self.bitmap);
        }
    };
}
