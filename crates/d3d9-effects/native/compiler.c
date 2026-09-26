#include <stddef.h>
#include <stdint.h>
#include <stdlib.h>

#include <vkd3d_shader.h>

struct openzt2_slice
{
    const uint8_t *data;
    size_t size;
};

typedef int (*openzt2_include_open)(void *context, const char *filename,
        const char *parent_data, struct openzt2_slice *source);
typedef void (*openzt2_include_close)(void *context, const uint8_t *data, size_t size);

struct openzt2_effect_compiler
{
    void *context;
    openzt2_include_open open;
    openzt2_include_close close;
};

struct openzt2_effect_result
{
    int status;
    const uint8_t *code;
    size_t code_size;
    char *messages;
};

static int open_include(const char *filename, bool local, const char *parent_data,
        void *context, struct vkd3d_shader_code *out)
{
    struct openzt2_effect_compiler *compiler = context;
    struct openzt2_slice source;

    (void)local;
    if (!compiler->open(compiler->context, filename, parent_data, &source))
        return VKD3D_ERROR_NOT_FOUND;
    out->code = source.data;
    out->size = source.size;
    return VKD3D_OK;
}

static void close_include(const struct vkd3d_shader_code *code, void *context)
{
    struct openzt2_effect_compiler *compiler = context;
    compiler->close(compiler->context, code->code, code->size);
}

struct openzt2_effect_result openzt2_effect_compile(const char *path,
        const uint8_t *source, size_t source_size, struct openzt2_effect_compiler *compiler)
{
    const struct vkd3d_shader_hlsl_source_info hlsl =
    {
        .type = VKD3D_SHADER_STRUCTURE_TYPE_HLSL_SOURCE_INFO,
        .entry_point = NULL,
        .profile = "fx_2_0",
    };
    const struct vkd3d_shader_preprocess_info preprocess =
    {
        .type = VKD3D_SHADER_STRUCTURE_TYPE_PREPROCESS_INFO,
        .next = &hlsl,
        .pfn_open_include = open_include,
        .pfn_close_include = close_include,
        .include_context = compiler,
    };
    const struct vkd3d_shader_compile_info info =
    {
        .type = VKD3D_SHADER_STRUCTURE_TYPE_COMPILE_INFO,
        .next = &preprocess,
        .source = {source, source_size},
        .source_type = VKD3D_SHADER_SOURCE_HLSL,
        .target_type = VKD3D_SHADER_TARGET_FX,
        .log_level = VKD3D_SHADER_LOG_INFO,
        .source_name = path,
    };
    struct vkd3d_shader_code code = {0};
    char *messages = NULL;
    int status = vkd3d_shader_compile(&info, &code, &messages);
    struct openzt2_effect_result result =
    {
        status, code.code, code.size, messages,
    };
    return result;
}

void openzt2_effect_free_code(const uint8_t *code, size_t size)
{
    struct vkd3d_shader_code shader_code = {code, size};
    vkd3d_shader_free_shader_code(&shader_code);
}

void openzt2_effect_free_messages(char *messages)
{
    vkd3d_shader_free_messages(messages);
}
