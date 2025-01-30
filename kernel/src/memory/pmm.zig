const std = @import("std");
const limine = @import("limine");
const serial = @import("../drivers/serial.zig");
const lib = @import("../lib.zig");
const bitmap = @import("../lib/bitmap.zig");
const debugPrint = @import("../util.zig").debugPrint;

// const PAGE_SIZE = 4096;
// const BITMAP_SIZE = MAX_FRAMES / 8; // One bit per frame
extern var _kernel_end: u8;
extern var _kernel_start: u8;

pub export var memmap_request: limine.MemoryMapRequest = .{};
pub export var hhdm_request: limine.HhdmRequest = .{};
pub export var executable_address_request: limine.KernelAddressRequest = .{};
// pub export var paging_mode_request: limine.PagingModeRequest = .{};

// Current design is: Find the maximum size of physical memory,
// create a bitmap that represents that size, and worry about
// the kernel and other reserved sections of memory later
// (essentially, pinning)

pub const FrameAllocator = struct {
        const Self = @This();

        physical_usable_memory_start: u64,
        physical_memory_size: u64,
        // top vmem address - physical memory size (for mapping) - kernel size
        virtual_kernel_space_start: u64,
        bitmap: bitmap.Bitmap(null, u64),
        next_available_frame: u64,

        pub fn init() !Self {
            // get memory map from limine
            const memmap = memmap_request.response orelse {
                @panic("No memory map provided by bootloader");
            };

            const virtual_kernel_end: u64 = @intFromPtr(&_kernel_end);
            const virtual_kernel_start: u64 = @intFromPtr(&_kernel_start);
            const kernel_size: u64 = virtual_kernel_end - virtual_kernel_start;
            debugPrint("Kernel start is {X}, kernel end is {X}, kernel size is {X}", .{ virtual_kernel_start, virtual_kernel_end, kernel_size });

            const hhdm = hhdm_request.response orelse {
                @panic("HHDM Failed");
            };

            debugPrint("The HHDM offset by Limine is 0x{X}", .{hhdm.offset});

            const kernel_address = executable_address_request.response orelse {
                @panic("Could not get kernel address");
            };

            debugPrint("The kernel is at VA: {X} and PA: {X}", .{ kernel_address.virtual_base, kernel_address.physical_base });

            // we have to figure out where to place the bitmap in memory. for now, we will say this is above where the kernel ends.
            // use limine to figure out kernel's end in physical memory, and HHDM offset to get that in virtual memory
            // const kernel_end_virtual = kernel_address.physical_base + kernel_size + hhdm.offset + 1;
            // const bitmap_start_aligned = (kernel_end_virtual + (lib.PAGE_SIZE - 1)) & ~(lib.PAGE_SIZE - 1);
            // const bitmap_start_aligned_ptr: *u64 = @ptrFromInt(bitmap_start_aligned);

            // go through memmap entries and find the last physical address
            var max_physical_address: u64 = 0;
            for (memmap.entries()) |entry| {
                debugPrint("Max phys address is {X} and entry end is {X} and entry kind is {}", .{ max_physical_address, entry.base + entry.length, entry.kind });
                if (entry.kind == .usable)
                    max_physical_address = @max(max_physical_address, entry.base + entry.length - 1);
            }

            debugPrint("The size of physical memory is: {X}", .{max_physical_address});

            // // testing: set first bit in bitmap to 1 such that findFirstFree returns 1
            // frame_bitmap.setEntry(0, 1) catch |err| switch (err) {
            //     bitmap.BitmapError.OutOfBounds => @panic("Index out of bounds"),
            //     else => @panic("Unknown error occurred"),
            // };
            // const first_free_index = frame_bitmap.findFirstFree() catch |err| switch (err) {
            //     bitmap.BitmapError.BitmapFull => @panic("Bitmap is full!"),
            //     else => @panic("Unknowkn error occurred"),
            // }

            const bitmap_size: u64 = max_physical_address / lib.PAGE_SIZE;
            var bitmap_start: u64 = 0;
            const total_pages: u64 = (max_physical_address + lib.PAGE_SIZE - 1) / lib.PAGE_SIZE;
            debugPrint("Pages: {}", .{total_pages});

            const bitmap_bytes: u64 = std.mem.alignForward(u64, total_pages, 64); // Round up to nearest byte

            // figure out where to put bitmap
            for (memmap.entries()) |entry| {
                debugPrint("Entry: base: {X}, length: {X}, type: {}", .{ entry.base, entry.length, entry.kind });

                if (entry.kind == .usable and entry.length >= bitmap_bytes) {
                    bitmap_start = entry.base + hhdm.offset;
                    break;
                }
            }

            debugPrint("Kernel end in physical {X}", .{kernel_address.physical_base + kernel_size + 1});
            debugPrint("Bitmap Bytes is {X}", .{bitmap_bytes});
            const bitmap_buffer = @as([*]u8, @ptrFromInt(bitmap_start))[0..bitmap_bytes];

            var fixed_alloc = std.heap.FixedBufferAllocator.init(bitmap_buffer);
            const physical_bitmap = bitmap.Bitmap(null, u64);
            var frame_bitmap = physical_bitmap.init(
                bitmap_size,
                fixed_alloc.allocator(),
            ) catch |err| {
                debugPrint("bitmap error: {}", .{err});
                @panic("Could not nit TestBitmap");
            };

            debugPrint("Past allocating bitmap", .{});

            var found_first_frame: bool = false;
            var first_frame_start: u64 = 0;

            for (memmap.entries()) |entry| {
                if (entry.kind != .usable) {
                    debugPrint("Entry: {X} ", .{entry.base});
                    debugPrint("Bitmap total entries: {X}", .{frame_bitmap.total_entries});
                    if (entry.base > frame_bitmap.total_entries * lib.PAGE_SIZE) {
                        break;
                    }
                    frame_bitmap.setContiguous(entry.base / lib.PAGE_SIZE, entry.length / lib.PAGE_SIZE, lib.BitMapAllocationStatus.ALLOCATED) catch |err| switch (err) {
                        bitmap.BitmapError.OutOfBounds => @panic("Index out of bounds"),
                        else => {},
                    };
                } else if (!found_first_frame) {
                    first_frame_start = entry.base;
                    found_first_frame = true;
                }
            }

            debugPrint("Past setting contiguous", .{});

            const is_set_0 = frame_bitmap.isSet(0) catch |err| switch (err) {
                bitmap.BitmapError.OutOfBounds => @panic("Index out of bounds"),
                else => @panic("Unknown error occurred"),
            };

            debugPrint("This address is: {X} and the bit is set to {}", .{ 0, is_set_0 });

            return Self{
                .physical_usable_memory_start = first_frame_start,
                .physical_memory_size = max_physical_address,
                .virtual_kernel_space_start = virtual_kernel_start,
                .bitmap = frame_bitmap,
                .next_available_frame = first_frame_start / lib.PAGE_SIZE // page index of first frame
            };
        }

        pub fn getPage(self: *Self) !u64 {
            // update bitmap and next available frame to return whenever we request a page
            const available_frame = self.next_available_frame;
            self.bitmap.setEntry(available_frame, lib.BitMapAllocationStatus.ALLOCATED) catch unreachable;
            self.next_available_frame = self.bitmap.findFirstFree() catch unreachable;
            return available_frame * lib.PAGE_SIZE;
        }

        pub fn freePage(self: *Self, phys_addr: u64) void {
            // free page at phys_addr
            self.bitmap.setEntry(phys_addr / 4096, lib.BitMapAllocationStatus.FREE) catch unreachable;
        }
    };