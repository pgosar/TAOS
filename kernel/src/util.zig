const std = @import("std");
const builtin = @import("builtin");
const serial = @import("./drivers/serial.zig");

pub inline fn debugPrint(comptime format: []const u8, args: anytype) void {
    if (builtin.mode == .Debug) {
        serial.println(format, args);
    }
}
