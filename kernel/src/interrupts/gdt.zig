const std = @import("std");
const serial = @import("../drivers/serial.zig");
const lib = @import("../lib.zig");

extern fn reload_segments() void;
extern fn load_tss(u32) void;

pub const TSS_START: u16 = 0x28;


pub const GdtEntry = packed struct {
    limit_low: u16 = 0xFF,
    base_low: u16 = 0,
    base_middle: u8 = 0,
    access: u8 = 0,
    limit_high: u4 = 0xF,
    flags: u4 = 0,
    base_high: u8 = 0
};


const GdtPtr = packed struct {
    limit: u16,
    base: u64
};

const Tss = packed struct {
    reserved0: u32,
    rsp0: u64 = 0,
    rsp1: u64 = 0,
    rsp2: u64 = 0,
    reserved1: u32,
    reserved2: u32,
    ist1: u64 = 0,
    ist2: u64 = 0,
    ist3: u64 = 0,
    ist4: u64 = 0,
    ist5: u64 = 0,
    ist6: u64 = 0,
    ist7: u64 = 0,
    reserved3: u32,
    reserved4: u32,
    reserved5: u16,
    iopb: u16
};

const TssDescriptorUpper = packed struct {
    base_upper: u32,
    reserved: u32 = 0
};

// 5 entries for null entry, kernel and user code and data,
// and 32 entries for two GDT entries per TSS
const GDT_ENTRIES: usize = 7;

var gdt_entries: [lib.MAX_NUM_CORES][GDT_ENTRIES]GdtEntry = undefined;
var tss: [lib.MAX_NUM_CORES]Tss = undefined;

// Mini stack for setting RSP - placeholder
var rsp0: [lib.MAX_NUM_CORES][lib.PAGE_SIZE * 2]u8 = undefined;


// Initializes the GDT with metadata of segments,
// initializes TSS and stores its metadata in GDT,
// and updates registers with metadata on GDT, segments, and TSS
pub fn init(cpu_num: u32) void {
    // Null Descriptor
    set_gate(cpu_num, 0, 0, 0);
    // Kernel Mode Code Segment
    set_gate(cpu_num, 1, 0x9A, 0xA);
    // Kernel Mode Data Segment
    set_gate(cpu_num, 2, 0x92, 0xC);
    // User Mode Code Segment
    set_gate(cpu_num, 3, 0xFA, 0xA);
    // User Mode Data Segment
    set_gate(cpu_num, 4, 0xF2, 0xC);
    // Task State Segment

    // Update metadata in TSSes
    tss[cpu_num].rsp0 = @intFromPtr(&rsp0[cpu_num][4096]);
    tss[cpu_num].iopb = @sizeOf(Tss);
    const tss_base: u64 = @intFromPtr(&tss[cpu_num]);

    // Set TSS Descriptor
    var tss_descriptor_lower: GdtEntry = undefined;
    var tss_descriptor_upper: TssDescriptorUpper = undefined;
    tss_descriptor_lower.limit_low = @as(u16, @truncate(@sizeOf(Tss) - 1));
    tss_descriptor_lower.base_low = @as(u16, @truncate(tss_base));
    tss_descriptor_lower.base_middle = @as(u8, @truncate(tss_base >> 16));
    tss_descriptor_lower.access = 0x89;
    tss_descriptor_lower.limit_high = @as(u4, @truncate((@sizeOf(Tss) - 1) >> 16));
    tss_descriptor_lower.flags = 0;
    tss_descriptor_lower.base_high = @as(u8, @truncate(tss_base >> 24));
    tss_descriptor_upper.base_upper = @as(u32, @truncate(tss_base >> 32));
    tss_descriptor_upper.reserved = 0;

    const tss_entry_lower: *GdtEntry = &tss_descriptor_lower;
    const tss_entry_upper: *GdtEntry = @ptrCast(&tss_descriptor_upper);

    // TSS Descriptor is two entries - put them both in GDT
    gdt_entries[cpu_num][5] = tss_entry_lower.*;
    gdt_entries[cpu_num][6] = tss_entry_upper.*;

    var gdt_ptr: GdtPtr = undefined;
    gdt_ptr.base = @intFromPtr(&gdt_entries[cpu_num]);

    gdt_ptr.limit = (@sizeOf(GdtEntry) * GDT_ENTRIES) - 1;
    load_gdt(gdt_ptr);
    load_tss(TSS_START + (cpu_num * (2 * @sizeOf(GdtEntry))));
    reload_segments();
}

// Helper function for filling in ent)ry into GDT
fn set_gate(cpu_num: u32, num: usize, access: u8, flags: u4) void {
    var entry = &gdt_entries[cpu_num][num];
    entry.limit_low = 0;
    entry.base_low = 0;
    entry.base_middle = 0;
    entry.access = access;
    entry.flags = flags;
    entry.base_high = 0;
}

// Updates GDTR register with pointer to GDT
pub fn load_gdt(gdt_ptr: GdtPtr) void {
    asm volatile ("lgdt (%[gdt_ptr])"
        :
        : [gdt_ptr] "r" (&gdt_ptr),
    );
}
