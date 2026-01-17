const std = @import("std");
const clap = @import("clap");

const wyhash = @import("./wyhash.zig").Wyhash11.hash;

const mem = std.mem;
const path = std.path;
const fs = std.fs;

const MakeError = std.fs.Dir.MakeError;

const wyhash_seed: u64 = 0;

const cli_error = error{MissingOutDirFlag};

pub const std_options = std.Options{
    .log_level = .debug,
};

/// CLI help message
const cli_help =
    \\ Tool for producing correctly named and positioned bun cache entries.
    \\
    \\ Does the following (roughly):
    \\ - Creates $out dir
    \\ - Calculates the correct output location for the package
    \\ - Symlinks the package contents to the calculated output location
    \\ - Creates any parent directories
    \\
    \\ Args:
    \\
;

/// CLI parameters
const params = clap.parseParamsComptime(
    \\--help             Display this help and exit.
    \\--out <path>       The $out directory to create and write to
    \\--name <str>       The package name (and version) as found in `bun.lock`
    \\--package <path>   The contents of the package to copy
    \\--patch-file <path>   Optional patch file to compute hash from
    \\
);

/// Clap parser string matchers
const parsers = .{
    .path = clap.parsers.string,
    .str = clap.parsers.string,
};

/// Main entry point
pub fn main() !void {
    var arena = std.heap.ArenaAllocator.init(std.heap.page_allocator);
    defer arena.deinit();

    const allocator = arena.allocator();

    var diag = clap.Diagnostic{};
    var res = clap.parse(clap.Help, &params, parsers, .{
        .diagnostic = &diag,
        .allocator = allocator,
        .assignment_separators = "=:",
    }) catch |err| {
        try diag.reportToFile(.stderr(), err);
        return err;
    };
    defer res.deinit();

    if (res.args.help != 0) {
        std.debug.print(cli_help, .{});
        return clap.usageToFile(.stdout(), clap.Help, &params);
    }

    const linker = PkgLinker.init(res.args.out, res.args.name, res.args.package, res.args.@"patch-file") orelse {
        std.debug.print(cli_help, .{});
        return clap.usageToFile(.stdout(), clap.Help, &params);
    };

    const cache_entry_location = try cachedFolderPrintBasename(
        allocator,
        linker.name,
        linker.patch_hash,
    );
    defer allocator.free(cache_entry_location);

    try linker.create_cache_entry(allocator, cache_entry_location);

    std.log.info("Successfully created cache entry symlink for `{s}`.\n", .{linker.name});
}

/// # Package Linker
///
/// Responsible for sym-linking the packages to their resulting directory
/// in the out path
pub const PkgLinker = struct {
    out: []const u8,
    name: []const u8,
    package: []const u8,
    patch_hash: ?u64,

    /// Create a new package linker
    pub fn init(out: ?[]const u8, name: ?[]const u8, package: ?[]const u8, patch_file: ?[]const u8) ?PkgLinker {
        // Compute patch hash if patch file is provided
        const patch_hash: ?u64 = if (patch_file) |pf| blk: {
            const file = fs.cwd().openFile(pf, .{}) catch |err| {
                std.log.err("Failed to open patch file `{s}`: {}\n", .{ pf, err });
                return null;
            };
            defer file.close();

            const contents = file.readToEndAlloc(std.heap.page_allocator, std.math.maxInt(usize)) catch |err| {
                std.log.err("Failed to read patch file `{s}`: {}\n", .{ pf, err });
                return null;
            };
            defer std.heap.page_allocator.free(contents);

            break :blk wyhash(wyhash_seed, contents);
        } else null;

        return PkgLinker{
            .out = out orelse return null,
            .name = name orelse return null,
            .package = package orelse return null,
            .patch_hash = patch_hash,
        };
    }

    /// # Create cache entry
    ///
    /// Creates a new cache entry at the output location passed.
    ///
    /// Only the leaf nodes may be symlinks hence yhis creates one of two cases:
    ///
    /// typescript@4.0.0
    /// - Create a symlink at $out/typescript@4.0.0
    ///
    /// @types/bun
    /// - Create parent directory $out/@types
    /// - Create a symlink at $out/@types/bun
    pub fn create_cache_entry(
        linker: PkgLinker,
        allocator: mem.Allocator,
        cache_entry_location: []u8,
    ) !void {
        std.log.info("Creating entry for `{s}`...\n", .{linker.name});

        const link_out_absolute = try std.fmt.allocPrint(
            allocator,
            "{s}/{s}",
            .{ linker.out, cache_entry_location },
        );
        defer allocator.free(link_out_absolute);

        std.log.debug("Link out path: `{s}`.\n", .{link_out_absolute});

        const link_parent_dir = try fs.path.resolve(
            allocator,
            &[_][]const u8{ link_out_absolute, ".." },
        );
        defer allocator.free(link_parent_dir);

        std.log.debug("Link parent dir: `{s}`.\n", .{link_parent_dir});

        try fs.cwd().makePath(link_parent_dir);
        std.log.debug("Created parent directory.\n", .{});

        try fs.symLinkAbsolute(
            linker.package,
            link_out_absolute,
            .{ .is_directory = true },
        );
    }
};

pub fn cachedFolderPrintBasename(
    allocator: mem.Allocator,
    input: []const u8,
    patch_hash: ?u64,
) ![]u8 {
    return if (mem.startsWith(u8, input, "tarball:"))
        cachedTarballFolderPrintBasename(allocator, input, patch_hash)
    else if (mem.startsWith(u8, input, "github:"))
        cachedGithubFolderPrintBasename(allocator, input, patch_hash)
    else if (mem.startsWith(u8, input, "git:"))
        cachedGitFolderPrintBasename(allocator, input, patch_hash)
    else
        cachedNpmPackageFolderPrintBasename(allocator, input, patch_hash);
}

/// Format patch hash suffix for cache entry names
fn formatPatchHashSuffix(allocator: mem.Allocator, patch_hash: ?u64) ![]const u8 {
    return if (patch_hash) |hash|
        try std.fmt.allocPrint(allocator, "_patch_hash={x}", .{hash})
    else
        try allocator.dupe(u8, "");
}

/// Produce a correct bun cache folder name for a given npm identifier
///
/// Adapted from [here](https://github.com/oven-sh/bun/blob/134341d2b48168cbb86f74879bf6c1c8e24b799c/src/install/PackageManager/PackageManagerDirectories.zig#L288)
pub fn cachedNpmPackageFolderPrintBasename(
    allocator: mem.Allocator,
    pkg: []const u8,
    patch_hash: ?u64,
) ![]u8 {
    const patch_suffix = try formatPatchHashSuffix(allocator, patch_hash);
    defer allocator.free(patch_suffix);

    const version_start = mem.lastIndexOfScalar(u8, pkg, '@') orelse {
        return std.fmt.allocPrint(allocator, "{s}@@@1{s}", .{ pkg, patch_suffix });
    };
    const name = pkg[0..version_start];
    const ver = pkg[version_start..];

    if (mem.indexOfScalar(u8, ver, '-')) |preIndex| {
        const version = ver[0..preIndex];
        const pre_and_build = ver[preIndex + 1 ..];

        if (mem.indexOfScalar(u8, pre_and_build, '+')) |buildIndex| {
            const pre = pre_and_build[0..buildIndex];
            const build = pre_and_build[buildIndex + 1 ..];

            return std.fmt.allocPrint(allocator, "{s}{s}-{x:0>16}+{X:0>16}@@@1{s}", .{
                name,
                version,
                wyhash(wyhash_seed, pre),
                wyhash(wyhash_seed, build),
                patch_suffix,
            });
        }

        return std.fmt.allocPrint(allocator, "{s}{s}-{x:0>16}@@@1{s}", .{
            name,
            version,
            wyhash(wyhash_seed, pre_and_build),
            patch_suffix,
        });
    }

    if (mem.indexOfScalar(u8, ver, '+')) |buildIndex| {
        const version = ver[0..buildIndex];
        const build = ver[buildIndex + 1 ..];

        return std.fmt.allocPrint(allocator, "{s}{s}+{X:0>16}@@@1{s}", .{
            name,
            version,
            wyhash(wyhash_seed, build),
            patch_suffix,
        });
    }

    return std.fmt.allocPrint(allocator, "{s}@@@1{s}", .{ pkg, patch_suffix });
}

/// Produce a correct bun cache folder name for a given tarball dependency
///
/// Adapted from [here](https://github.com/oven-sh/bun/blob/550522e99b303d8172b7b16c5750d458cb056434/src/install/PackageManager/PackageManagerDirectories.zig#L353)
pub fn cachedTarballFolderPrintBasename(
    allocator: mem.Allocator,
    url: []const u8,
    patch_hash: ?u64,
) ![]u8 {
    const pre = "tarball:";
    const without_pre = url[pre.len..];
    const patch_suffix = try formatPatchHashSuffix(allocator, patch_hash);
    defer allocator.free(patch_suffix);

    return std.fmt.allocPrint(allocator, "@T@{x:0>16}@@@1{s}", .{
        wyhash(wyhash_seed, without_pre),
        patch_suffix,
    });
}

/// Produce a correct bun cache folder name for a given github dependency
///
/// Adapted from [here](https://github.com/oven-sh/bun/blob/550522e99b303d8172b7b16c5750d458cb056434/src/install/PackageManager/PackageManagerDirectories.zig#L353)
pub fn cachedGithubFolderPrintBasename(
    allocator: mem.Allocator,
    url: []const u8,
    patch_hash: ?u64,
) ![]u8 {
    const pre = "github:";
    const without_pre = url[pre.len..];
    const patch_suffix = try formatPatchHashSuffix(allocator, patch_hash);
    defer allocator.free(patch_suffix);

    return std.fmt.allocPrint(allocator, "@GH@{s}@@@1{s}", .{
        without_pre,
        patch_suffix,
    });
}

/// Produce a correct bun cache folder name for a given git dependency
///
/// Adapted from [here](https://github.com/oven-sh/bun/blob/550522e99b303d8172b7b16c5750d458cb056434/src/install/PackageManager/PackageManagerDirectories.zig#L353)
pub fn cachedGitFolderPrintBasename(
    allocator: mem.Allocator,
    url: []const u8,
    patch_hash: ?u64,
) ![]u8 {
    const pre = "git:";
    const without_pre = url[pre.len..];
    const patch_suffix = try formatPatchHashSuffix(allocator, patch_hash);
    defer allocator.free(patch_suffix);

    return std.fmt.allocPrint(allocator, "@G@{s}{s}", .{
        without_pre,
        patch_suffix,
    });
}

const expectEqualSlices = std.testing.expectEqualSlices;
const testing_allocator = std.testing.allocator;

fn testBaseNameFn(
    tests: []const struct { []const u8, []const u8 },
    func: anytype,
) !void {
    for (tests) |case| {
        const input, const output = case;

        const res = try func(testing_allocator, input);
        defer testing_allocator.free(res);

        try expectEqualSlices(u8, output, res);
    }
}

fn testNpmBaseName(input: []const u8, patch_hash: ?u64, expected: []const u8) !void {
    const res = try cachedNpmPackageFolderPrintBasename(testing_allocator, input, patch_hash);
    defer testing_allocator.free(res);
    try expectEqualSlices(u8, expected, res);
}

test "cachedNpmPackageFolderPrintBasename function" {
    // Without patches
    try testNpmBaseName("react@1.2.3-beta.1+build.123", null, "react@1.2.3-c0734e9369ab610d+F48F05ED5AABC3A0@@@1");
    try testNpmBaseName("tailwindcss@4.0.0-beta.9", null, "tailwindcss@4.0.0-73c5c46324e78b9b@@@1");
    try testNpmBaseName("react@1.2.3+build.123", null, "react@1.2.3+F48F05ED5AABC3A0@@@1");
    try testNpmBaseName("react@1.2.3", null, "react@1.2.3@@@1");
    try testNpmBaseName("undici-types@6.20.0", null, "undici-types@6.20.0@@@1");
    try testNpmBaseName("@types/react-dom@19.0.4", null, "@types/react-dom@19.0.4@@@1");
    try testNpmBaseName("react-compiler-runtime@19.0.0-beta-e552027-20250112", null, "react-compiler-runtime@19.0.0-0f3fc645a5103715@@@1");

    // With patch hash
    try testNpmBaseName("lodash@4.17.21", 0x1234567890abcdef, "lodash@4.17.21@@@1_patch_hash=1234567890abcdef");
    try testNpmBaseName("@apollo/client@3.10.1", 0xdeadbeef, "@apollo/client@3.10.1@@@1_patch_hash=deadbeef");
}

test "cachedTarballFolderPrintBasename function" {
    const res = try cachedTarballFolderPrintBasename(testing_allocator, "tarball:https://registry.npmjs.org/zod/-/zod-3.21.4.tgz", null);
    defer testing_allocator.free(res);
    try expectEqualSlices(u8, "@T@3be02e19198e30ee@@@1", res);
}

test "cachedGithubFolderPrintBasename function" {
    const res = try cachedGithubFolderPrintBasename(testing_allocator, "github:colinhacks-zod-f9bbb50", null);
    defer testing_allocator.free(res);
    try expectEqualSlices(u8, "@GH@colinhacks-zod-f9bbb50@@@1", res);
}

test "cachedGitFolderPrintBasename function" {
    const res = try cachedGitFolderPrintBasename(testing_allocator, "git:ee100d81f12ae315a81c2a664979a6cc1bce99a2", null);
    defer testing_allocator.free(res);
    try expectEqualSlices(u8, "@G@ee100d81f12ae315a81c2a664979a6cc1bce99a2", res);
}
