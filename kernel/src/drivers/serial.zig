const std = @import("std");

const x86 = struct {
    pub inline fn outb(port: u16, value: u8) void {
        asm volatile ("outb %[value], %[port]"
            :
            : [port] "{dx}" (port),
              [value] "{al}" (value),
        );
    }

    pub inline fn inb(port: u16) u8 {
        return asm volatile ("inb %[port], %[ret]"
            : [ret] "={al}" (-> u8),
            : [port] "{dx}" (port),
        );
    }
};

const SerialPort = enum(u16) {
    Com1 = 0x3F8,
    Com2 = 0x2F8,
    Com3 = 0x3E8,
    Com4 = 0x2E8,
};

const LineStatus = packed struct {
    data_ready: bool,
    overrun_error: bool,
    parity_error: bool,
    framing_error: bool,
    break_indicator: bool,
    transmitter_holding_empty: bool,
    transmitter_empty: bool,
    impending_error: bool,
};

const LineControl = packed struct {
    word_length: u2,
    stop_bits: u1,
    parity: u3,
    break_control: bool,
    dlab: bool,
};

pub const Serial = struct {
    port: SerialPort,
    initialized: bool = false,

    const Self = @This();

    pub fn init(port: SerialPort) Self {
        var self = Self{ .port = port };
        self.initialize();
        return self;
    }

    fn initialize(self: *Self) void {
        if (self.initialized) return;

        // Disable interrupts
        x86.outb(self.get_port(1), 0x00);

        // Enable DLAB to set baud rate
        x86.outb(self.get_port(3), 0x80);

        // Set divisor to 3 (38400 baud)
        x86.outb(self.get_port(0), 0x03);
        x86.outb(self.get_port(1), 0x00);

        // 8 bits, no parity, one stop bit
        x86.outb(self.get_port(3), 0x03);

        // Enable FIFO, clear with 14-byte threshold
        x86.outb(self.get_port(2), 0xC7);

        // Mark as ready to use
        x86.outb(self.get_port(4), 0x0B);

        self.initialized = true;
    }

    fn get_port(self: Self, offset: u16) u16 {
        return @intFromEnum(self.port) + offset;
    }

    pub fn write(self: Self, char: u8) void {
        while (!self.can_write()) {}
        x86.outb(self.get_port(0), char);
    }

    pub fn write_string(self: Self, string: []const u8) void {
        for (string) |char| {
            self.write(char);
        }
    }

    pub fn write_string_ln(self: Self, string: []const u8) void {
        self.write_string(string);
        self.write('\n');
    }

    pub fn write_fmt(self: Self, comptime format: []const u8, args: anytype) void {
        var buf: [1024]u8 = undefined;
        const string = std.fmt.bufPrint(&buf, format, args) catch return;
        self.write_string(string);
    }

    pub fn write_fmt_ln(self: Self, comptime format: []const u8, args: anytype) void {
        self.write_fmt(format, args);
        self.write('\n');
    }

    fn can_write(self: Self) bool {
        const status = @as(LineStatus, @bitCast(x86.inb(self.get_port(5))));
        return status.transmitter_holding_empty;
    }

    pub fn can_read(self: Self) bool {
        const status = @as(LineStatus, @bitCast(x86.inb(self.get_port(5))));
        return status.data_ready;
    }

    pub fn read(self: Self) ?u8 {
        if (!self.can_read()) return null;
        return x86.inb(self.get_port(0));
    }
};

// Global instance for COM1
var com1: ?Serial = null;

pub fn get_serial() *Serial {
    if (com1 == null) {
        com1 = Serial.init(.Com1);
    }
    return &com1.?;
}

pub fn print(comptime format: []const u8, args: anytype) void {
    get_serial().write_fmt(format, args);
}

pub fn println(comptime format: []const u8, args: anytype) void {
    get_serial().write_fmt_ln(format, args);
}

pub fn write(string: []const u8) void {
    get_serial().write_string(string);
}

pub fn writeln(string: []const u8) void {
    get_serial().write_string_ln(string);
}
