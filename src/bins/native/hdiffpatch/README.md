# HPatch source provenance

Unmodified patch-only sources from [HDiffPatch v4.12.0](https://github.com/sisong/HDiffPatch/tree/v4.12.0/libHDiffPatch/HPatch), licensed under MIT (see LICENSE).
The tag source ZIP SHA-256 is `aa131a3fc771cedf3b7cc51034ca257c0d43c4b5d1cdb9f45e3b3de59944dcd6`.

Files copied from `libHDiffPatch/HPatch`: `patch.c`, `patch.h`, `patch_private.h`, `patch_types.h`, and `hpatch_mt/hpatch_mt.h`.
The parent `hdiffpatch_bridge.c` is Velopack's adapter. Cargo compiles the library with `cc` for the target platform; no downloaded runtime helper is needed by installed applications.

The adapter accepts uncompressed HDIFF13 only, validates old and new sizes before patching, and uses a 1 MiB cache with synchronous Rust file callbacks. Rust checks the existing `.shasum` sidecar's SHA-1 and size before replacing the old file. This is integrity checking, not authentication. Patch compression comes from the enclosing nupkg ZIP. Multithreaded patching and compression plugins are disabled.

To update, copy the same files and license from a pinned upstream release, update the packaging helper and download checksums, and run the C# and Rust delta tests together. Preserve the HDIFF13 format unless both consumers are explicitly migrated.
