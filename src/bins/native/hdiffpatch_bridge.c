// Small ABI boundary around the pinned upstream HPatch library.
// Rust owns all files and buffers; callbacks are synchronous and never retained.
#include <stdint.h>
#include "hdiffpatch/patch.h"

typedef int (*read_fn)(void*, uint64_t, unsigned char*, size_t);
typedef int (*write_fn)(void*, uint64_t, const unsigned char*, size_t);
typedef struct { void* handle; read_fn read; } input_context;
typedef struct { void* handle; write_fn write; } output_context;

static hpatch_BOOL read_input(const hpatch_TStreamInput* stream, hpatch_StreamPos_t pos,
                             unsigned char* start, unsigned char* end) {
    input_context* ctx = (input_context*)stream->streamImport;
    return ctx->read(ctx->handle, pos, start, (size_t)(end - start));
}

static hpatch_BOOL write_output(const hpatch_TStreamOutput* stream, hpatch_StreamPos_t pos,
                               const unsigned char* start, const unsigned char* end) {
    output_context* ctx = (output_context*)stream->streamImport;
    return ctx->write(ctx->handle, pos, start, (size_t)(end - start));
}

int velopack_hpatch(void* old_handle, uint64_t old_size, void* diff_handle, uint64_t diff_size,
                    void* out_handle, uint64_t expected_size, read_fn read, write_fn write,
                    unsigned char* cache, size_t cache_size) {
    input_context old_ctx = {old_handle, read}, diff_ctx = {diff_handle, read};
    output_context out_ctx = {out_handle, write};
    hpatch_TStreamInput old_stream = {&old_ctx, old_size, read_input, 0};
    hpatch_TStreamInput diff_stream = {&diff_ctx, diff_size, read_input, 0};
    hpatch_TStreamOutput out_stream = {&out_ctx, expected_size, 0, write_output};
    hpatch_compressedDiffInfo info;
    if (!getCompressedDiffInfo(&info, &diff_stream) || info.oldDataSize != old_size ||
        info.newDataSize != expected_size || info.compressedCount != 0 || info.compressType[0] != 0)
        return 0;
    return patch_decompress_with_cache(&out_stream, &old_stream, &diff_stream, 0, cache, cache + cache_size);
}
