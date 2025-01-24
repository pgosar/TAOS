const std = @import("std");

pub fn build(b: *std.Build) void {
    var target_query: std.Target.Query = .{
        .cpu_arch = .x86_64,
        .os_tag = .freestanding,
        .abi = .none,
    };

    const Feature = std.Target.x86.Feature;
    target_query.cpu_features_add.addFeature(@intFromEnum(Feature.soft_float));
    target_query.cpu_features_sub.addFeature(@intFromEnum(Feature.mmx));
    target_query.cpu_features_sub.addFeature(@intFromEnum(Feature.sse));
    target_query.cpu_features_sub.addFeature(@intFromEnum(Feature.sse2));
    target_query.cpu_features_sub.addFeature(@intFromEnum(Feature.avx));
    target_query.cpu_features_sub.addFeature(@intFromEnum(Feature.avx2));

    const target = b.resolveTargetQuery(target_query);
    const optimize = b.standardOptimizeOption(.{});
    const limine = b.dependency("limine", .{});

    // Build the kernel
    const kernel = b.addExecutable(.{
        .name = "kernel",
        .root_source_file = b.path("src/main.zig"),
        .target = target,
        .optimize = optimize,
        .code_model = .kernel,
    });
    kernel.addAssemblyFile(b.path("src/interrupts/interrupts.s"));
    kernel.addAssemblyFile(b.path("./src/interrupts/gdt.s"));


    // Disable LTO to preserve Limine requests
    kernel.want_lto = false;

    // Add Limine dependency
    kernel.root_module.addImport("limine", limine.module("limine"));

    // Set x86_64 linker script
    kernel.setLinkerScriptPath(b.path("linker-x86_64.ld"));

    b.installArtifact(kernel);
}
