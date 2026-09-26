#include <stddef.h>
#include <stdint.h>
#include <stdlib.h>
#include <string.h>

#include <mojoshader.h>
#include <mojoshader_effects.h>

static const char external_vertex_declarations_required[] =
        "shader model 1 vertex bytecode requires external vertex declarations; "
        "compile source with explicit input semantics before SPIR-V translation";

static int shader_bytecode_requires_external_vertex_declarations(const uint8_t *bytecode, size_t size)
{
    return size >= 4 && bytecode[3] == 0xff && bytecode[2] == 0xfe && bytecode[1] == 1;
}

struct openzt2_backend_shader
{
    unsigned int references;
    uint8_t *bytecode;
    size_t bytecode_size;
    const MOJOSHADER_parseData *parse;
};

struct openzt2_effect
{
    MOJOSHADER_effect *effect;
    MOJOSHADER_effectStateChanges changes;
    struct openzt2_backend_shader *vertex;
    struct openzt2_backend_shader *pixel;
    float vertex_float[256 * 4];
    int vertex_int[16 * 4];
    uint8_t vertex_bool[16];
    float pixel_float[256 * 4];
    int pixel_int[16 * 4];
    uint8_t pixel_bool[16];
    char *error;
};

static void set_effect_error(struct openzt2_effect *owner, const char *message)
{
    free(owner->error);
    owner->error = malloc(strlen(message) + 1);
    if (owner->error)
        strcpy(owner->error, message);
}

static void *compile_effect_shader(const void *context, const char *mainfn,
        const unsigned char *tokenbuf, unsigned int size, const MOJOSHADER_swizzle *swiz,
        unsigned int swizcount, const MOJOSHADER_samplerMap *smap, unsigned int smapcount)
{
    struct openzt2_effect *owner = (struct openzt2_effect *)context;
    if (shader_bytecode_requires_external_vertex_declarations(tokenbuf, size))
    {
        set_effect_error(owner, external_vertex_declarations_required);
        return NULL;
    }
    struct openzt2_backend_shader *shader = calloc(1, sizeof(*shader));

    if (!shader)
        return NULL;
    shader->bytecode = malloc(size);
    if (!shader->bytecode)
    {
        free(shader);
        return NULL;
    }
    memcpy(shader->bytecode, tokenbuf, size);
    shader->bytecode_size = size;
    shader->references = 1;
    shader->parse = MOJOSHADER_parse(MOJOSHADER_PROFILE_SPIRV, mainfn, tokenbuf, size,
            swiz, swizcount, smap, smapcount, NULL, NULL, NULL);
    if (!shader->parse || shader->parse->error_count)
    {
        set_effect_error(owner, shader->parse && shader->parse->error_count
                ? shader->parse->errors[0].error : "MojoShader returned no shader");
        MOJOSHADER_freeParseData(shader->parse);
        free(shader->bytecode);
        free(shader);
        return NULL;
    }
    return shader;
}

static void add_shader_reference(void *value)
{
    ++((struct openzt2_backend_shader *)value)->references;
}

static void delete_effect_shader(const void *context, void *value)
{
    struct openzt2_backend_shader *shader = value;
    (void)context;
    if (!shader || --shader->references)
        return;
    MOJOSHADER_freeParseData(shader->parse);
    free(shader->bytecode);
    free(shader);
}

static MOJOSHADER_parseData *effect_shader_parse_data(void *value)
{
    return (MOJOSHADER_parseData *)((struct openzt2_backend_shader *)value)->parse;
}

static void bind_effect_shaders(const void *context, void *vertex, void *pixel)
{
    struct openzt2_effect *owner = (struct openzt2_effect *)context;
    owner->vertex = vertex;
    owner->pixel = pixel;
}

static void get_bound_effect_shaders(const void *context, void **vertex, void **pixel)
{
    const struct openzt2_effect *owner = context;
    *vertex = owner->vertex;
    *pixel = owner->pixel;
}

static void map_effect_registers(const void *context, float **vsf, int **vsi, uint8_t **vsb,
        float **psf, int **psi, uint8_t **psb)
{
    struct openzt2_effect *owner = (struct openzt2_effect *)context;
    *vsf = owner->vertex_float;
    *vsi = owner->vertex_int;
    *vsb = owner->vertex_bool;
    *psf = owner->pixel_float;
    *psi = owner->pixel_int;
    *psb = owner->pixel_bool;
}

static void unmap_effect_registers(const void *context)
{
    (void)context;
}

static const char *effect_error(const void *context)
{
    const struct openzt2_effect *owner = context;
    return owner->error ? owner->error : "MojoShader Effects failed";
}

struct openzt2_effect *openzt2_effect_open(const uint8_t *bytecode, size_t size, char **message)
{
    struct openzt2_effect *owner;
    MOJOSHADER_effectShaderContext context;

    *message = NULL;
    if (size > UINT32_MAX || !(owner = calloc(1, sizeof(*owner))))
        return NULL;
    memset(&context, 0, sizeof(context));
    context.compileShader = compile_effect_shader;
    context.shaderAddRef = add_shader_reference;
    context.deleteShader = delete_effect_shader;
    context.getParseData = effect_shader_parse_data;
    context.bindShaders = bind_effect_shaders;
    context.getBoundShaders = get_bound_effect_shaders;
    context.mapUniformBufferMemory = map_effect_registers;
    context.unmapUniformBufferMemory = unmap_effect_registers;
    context.getError = effect_error;
    context.shaderContext = owner;
    owner->effect = MOJOSHADER_compileEffect(bytecode, (unsigned int)size,
            NULL, 0, NULL, 0, &context);
    if (!owner->effect || owner->effect->error_count)
    {
        const char *error = owner->error;
        if (!error && owner->effect && owner->effect->error_count)
            error = owner->effect->errors[0].error;
        if (error)
        {
            *message = malloc(strlen(error) + 1);
            if (*message)
                strcpy(*message, error);
        }
        if (owner->effect)
            MOJOSHADER_deleteEffect(owner->effect);
        free(owner->error);
        free(owner);
        return NULL;
    }
    return owner;
}

void openzt2_effect_close(struct openzt2_effect *owner)
{
    if (!owner)
        return;
    MOJOSHADER_deleteEffect(owner->effect);
    free(owner->error);
    free(owner);
}

void openzt2_effect_set_raw(struct openzt2_effect *owner, const char *name,
        const void *data, unsigned int size)
{
    int index;
    /* Blue Fang material assignments address FX semantics. These often differ
     * from the case-sensitive HLSL variable names used by the name setter. */
    for (index = 0; index < owner->effect->param_count; ++index)
    {
        const MOJOSHADER_effectParam *parameter = &owner->effect->params[index];
        if (parameter->value.semantic && !strcmp(parameter->value.semantic, name))
        {
            MOJOSHADER_effectSetRawValueHandle(parameter, data, 0, size);
            return;
        }
    }
    MOJOSHADER_effectSetRawValueName(owner->effect, name, data, 0, size);
}

unsigned int openzt2_effect_parameter_count(const struct openzt2_effect *owner)
{
    return owner->effect->param_count;
}

const MOJOSHADER_effectValue *openzt2_effect_parameter(const struct openzt2_effect *owner,
        unsigned int index)
{
    return &owner->effect->params[index].value;
}

const char *openzt2_effect_parameter_name(const struct openzt2_effect *owner, unsigned int index)
{
    return owner->effect->params[index].value.name;
}

const char *openzt2_effect_parameter_semantic(const struct openzt2_effect *owner, unsigned int index)
{
    return owner->effect->params[index].value.semantic;
}

unsigned int openzt2_effect_parameter_class(const struct openzt2_effect *owner, unsigned int index)
{
    return owner->effect->params[index].value.type.parameter_class;
}

unsigned int openzt2_effect_parameter_type(const struct openzt2_effect *owner, unsigned int index)
{
    return owner->effect->params[index].value.type.parameter_type;
}

unsigned int openzt2_effect_parameter_rows(const struct openzt2_effect *owner, unsigned int index)
{
    return owner->effect->params[index].value.type.rows;
}

unsigned int openzt2_effect_parameter_columns(const struct openzt2_effect *owner, unsigned int index)
{
    return owner->effect->params[index].value.type.columns;
}

unsigned int openzt2_effect_parameter_elements(const struct openzt2_effect *owner, unsigned int index)
{
    return owner->effect->params[index].value.type.elements;
}

unsigned int openzt2_effect_parameter_annotations(const struct openzt2_effect *owner, unsigned int index)
{
    return owner->effect->params[index].annotation_count;
}

unsigned int openzt2_effect_technique_count(const struct openzt2_effect *owner)
{
    return owner->effect->technique_count;
}

const char *openzt2_effect_technique_name(const struct openzt2_effect *owner, unsigned int technique)
{
    return owner->effect->techniques[technique].name;
}

int openzt2_effect_technique_is_valid(const struct openzt2_effect *owner, unsigned int technique)
{
    const MOJOSHADER_effectTechnique *candidate = NULL;
    const MOJOSHADER_effectTechnique *wanted = &owner->effect->techniques[technique];
    while ((candidate = MOJOSHADER_effectFindNextValidTechnique(owner->effect, candidate)) != NULL)
        if (candidate == wanted)
            return 1;
    return 0;
}

int openzt2_effect_technique_float_annotation(const struct openzt2_effect *owner,
        unsigned int technique, const char *name, float *value)
{
    const MOJOSHADER_effectTechnique *source = &owner->effect->techniques[technique];
    unsigned int annotation;
    for (annotation = 0; annotation < source->annotation_count; ++annotation)
    {
        const MOJOSHADER_effectAnnotation *candidate = &source->annotations[annotation];
        if (candidate->name && !strcmp(candidate->name, name) && candidate->value_count > 0
                && candidate->type.parameter_type == MOJOSHADER_SYMTYPE_FLOAT)
        {
            *value = candidate->valuesF[0];
            return 1;
        }
    }
    return 0;
}

unsigned int openzt2_effect_pass_count(const struct openzt2_effect *owner, unsigned int technique)
{
    return owner->effect->techniques[technique].pass_count;
}

const char *openzt2_effect_pass_name(const struct openzt2_effect *owner,
        unsigned int technique, unsigned int pass)
{
    return owner->effect->techniques[technique].passes[pass].name;
}

void openzt2_effect_begin_pass(struct openzt2_effect *owner,
        unsigned int technique, unsigned int pass)
{
    unsigned int pass_count;
    MOJOSHADER_effectSetTechnique(owner->effect, &owner->effect->techniques[technique]);
    MOJOSHADER_effectBegin(owner->effect, &pass_count, 0, &owner->changes);
    MOJOSHADER_effectBeginPass(owner->effect, pass);
    /* MojoShader reports explicit ShaderConstant states to the backend. They
     * are separate from uniforms copied through mapUniformBufferMemory. */
    for (unsigned int i = 0; i < owner->changes.render_state_change_count; ++i) {
        const MOJOSHADER_effectState *state = &owner->changes.render_state_changes[i];
        unsigned int type = (unsigned int)state->type;
        if (type < 148 || type > 163) continue;
        const unsigned int pixel = type >= 156;
        const unsigned int bank = (type - 148) % 8;
        const MOJOSHADER_effectValue *value = &state->value;
        unsigned int first = state->index;
        unsigned int count = value->value_count;
        if (bank == 1) {
            if (first >= 16 || count > 16 - first) {
                set_effect_error(owner, "explicit boolean shader constant range exceeds the register bank");
                continue;
            }
            uint8_t *target = pixel ? owner->pixel_bool : owner->vertex_bool;
            for (unsigned int component = 0; component < count; ++component)
                target[first + component] = value->valuesI[component] != 0;
        } else {
            const unsigned int capacity = bank == 2 ? 16 : 256;
            if (first >= capacity || count > (capacity - first) * 4) {
                set_effect_error(owner, "explicit shader constant range exceeds the register bank");
                continue;
            }
            if (bank == 2) {
                int *target = pixel ? owner->pixel_int : owner->vertex_int;
                memcpy(target + first * 4, value->valuesI, count * sizeof(int));
            } else {
                float *target = pixel ? owner->pixel_float : owner->vertex_float;
                memcpy(target + first * 4, value->valuesF, count * sizeof(float));
            }
        }
    }
}

void openzt2_effect_end_pass(struct openzt2_effect *owner)
{
    MOJOSHADER_effectEndPass(owner->effect);
    MOJOSHADER_effectEnd(owner->effect);
}

unsigned int openzt2_effect_state_count(const struct openzt2_effect *owner)
{
    return owner->changes.render_state_change_count;
}

unsigned int openzt2_effect_state_type(const struct openzt2_effect *owner, unsigned int index)
{
    return owner->changes.render_state_changes[index].type;
}

unsigned int openzt2_effect_state_index(const struct openzt2_effect *owner, unsigned int index)
{
    return owner->changes.render_state_changes[index].index;
}

unsigned int openzt2_effect_state_value(const struct openzt2_effect *owner, unsigned int index)
{
    return (unsigned int)owner->changes.render_state_changes[index].value.valuesI[0];
}

void openzt2_effect_state_float_values(const struct openzt2_effect *owner,
        unsigned int index, float *values, unsigned int count)
{
    const MOJOSHADER_effectValue *value = &owner->changes.render_state_changes[index].value;
    const unsigned int available = value->value_count < count ? value->value_count : count;
    unsigned int i;
    for (i = 0; i < available; ++i)
        values[i] = value->valuesF[i];
    for (; i < count; ++i)
        values[i] = 0.0f;
}

const char *openzt2_effect_state_mapping(const struct openzt2_effect *owner, unsigned int index)
{
    const MOJOSHADER_effectState *state = &owner->changes.render_state_changes[index];
    if (state->parameter)
        return state->parameter;
    const unsigned int object = (unsigned int)state->value.valuesI[0];
    return owner->effect->objects[object].mapping.name;
}

static const MOJOSHADER_effectValue *mapped_sampler(
        const struct openzt2_effect *owner, unsigned int index)
{
    const MOJOSHADER_effectState *state = &owner->changes.render_state_changes[index];
    int parameter;

    if (!state->parameter)
        return NULL;
    for (parameter = 0; parameter < owner->effect->param_count; ++parameter)
        if (!strcmp(owner->effect->params[parameter].value.name, state->parameter))
            return &owner->effect->params[parameter].value;
    return NULL;
}

unsigned int openzt2_effect_sampler_state_count(
        const struct openzt2_effect *owner, unsigned int index)
{
    const MOJOSHADER_effectValue *sampler = mapped_sampler(owner, index);
    return sampler ? sampler->value_count : 0;
}

unsigned int openzt2_effect_sampler_state_type(
        const struct openzt2_effect *owner, unsigned int index, unsigned int state)
{
    return mapped_sampler(owner, index)->valuesSS[state].type;
}

unsigned int openzt2_effect_sampler_state_value(
        const struct openzt2_effect *owner, unsigned int index, unsigned int state)
{
    return (unsigned int)mapped_sampler(owner, index)->valuesSS[state].value.valuesI[0];
}

const char *openzt2_effect_sampler_texture_mapping(
        const struct openzt2_effect *owner, unsigned int index, unsigned int state)
{
    const MOJOSHADER_effectValue *value = &mapped_sampler(owner, index)->valuesSS[state].value;
    const unsigned int object = (unsigned int)value->valuesI[0];
    return owner->effect->objects[object].mapping.name;
}

const uint8_t *openzt2_effect_vertex_shader(const struct openzt2_effect *owner, size_t *size)
{
    if (!owner->vertex)
    {
        *size = 0;
        return NULL;
    }
    *size = owner->vertex->bytecode_size;
    return owner->vertex->bytecode;
}

const uint8_t *openzt2_effect_pixel_shader(const struct openzt2_effect *owner, size_t *size)
{
    if (!owner->pixel)
    {
        *size = 0;
        return NULL;
    }
    *size = owner->pixel->bytecode_size;
    return owner->pixel->bytecode;
}

const float *openzt2_effect_vertex_float(const struct openzt2_effect *owner) { return owner->vertex_float; }
const int *openzt2_effect_vertex_int(const struct openzt2_effect *owner) { return owner->vertex_int; }
const uint8_t *openzt2_effect_vertex_bool(const struct openzt2_effect *owner) { return owner->vertex_bool; }
const float *openzt2_effect_pixel_float(const struct openzt2_effect *owner) { return owner->pixel_float; }
const int *openzt2_effect_pixel_int(const struct openzt2_effect *owner) { return owner->pixel_int; }
const uint8_t *openzt2_effect_pixel_bool(const struct openzt2_effect *owner) { return owner->pixel_bool; }

struct openzt2_shader_result
{
    int status;
    const MOJOSHADER_parseData *shader;
    char *message;
};

struct openzt2_shader_result openzt2_shader_translate(const uint8_t *bytecode, size_t size)
{
    const MOJOSHADER_parseData *shader;
    struct openzt2_shader_result result = {0};

    if (size > UINT32_MAX)
    {
        result.status = -1;
        return result;
    }

    shader = shader_bytecode_requires_external_vertex_declarations(bytecode, size) ? NULL
            : MOJOSHADER_parse(MOJOSHADER_PROFILE_SPIRV, "main", bytecode,
                    (unsigned int)size, NULL, 0, NULL, 0, NULL, NULL, NULL);
    if (!shader || shader->error_count || !shader->output || shader->output_len <= 0)
    {
        const char *message = shader_bytecode_requires_external_vertex_declarations(bytecode, size)
                ? external_vertex_declarations_required
                : shader && shader->error_count && shader->errors[0].error
                ? shader->errors[0].error : "MojoShader returned no SPIR-V";
        result.status = -1;
        result.message = malloc(strlen(message) + 1);
        if (result.message)
            strcpy(result.message, message);
        if (shader)
            MOJOSHADER_freeParseData(shader);
        return result;
    }

    result.shader = shader;
    if (shader->shader_type == MOJOSHADER_TYPE_VERTEX)
        MOJOSHADER_linkSPIRVShaders(shader, NULL, NULL, 0);
    return result;
}

int openzt2_shader_link(const MOJOSHADER_parseData *vertex,
        const MOJOSHADER_parseData *pixel)
{
    return MOJOSHADER_linkSPIRVShaders(vertex, pixel, NULL, 0) > 0 ? 0 : -1;
}

struct openzt2_shader_result openzt2_shader_assemble(const uint8_t *source, size_t size)
{
    const MOJOSHADER_parseData *shader;
    struct openzt2_shader_result result = {0};

    if (size > UINT32_MAX)
    {
        result.status = -1;
        return result;
    }
    shader = MOJOSHADER_assemble("inline.fx", (const char *)source, (unsigned int)size,
            NULL, 0, NULL, 0, NULL, 0, NULL, NULL, NULL, NULL, NULL);
    if (!shader || shader->error_count || !shader->output || shader->output_len <= 0)
    {
        const char *message = shader && shader->error_count && shader->errors[0].error
                ? shader->errors[0].error : "MojoShader returned no assembled shader";
        result.status = -1;
        result.message = malloc(strlen(message) + 1);
        if (result.message)
            strcpy(result.message, message);
        if (shader)
            MOJOSHADER_freeParseData(shader);
        return result;
    }
    result.shader = shader;
    return result;
}

const uint8_t *openzt2_shader_code(const MOJOSHADER_parseData *shader)
{
    return (const uint8_t *)shader->output;
}

size_t openzt2_shader_code_size(const MOJOSHADER_parseData *shader)
{
    const int size = MOJOSHADER_spirvBinarySize(shader);
    return size > 0 ? (size_t)size : 0;
}

size_t openzt2_assembled_shader_bytecode_size(const MOJOSHADER_parseData *shader)
{
    return shader->output_len > 0 ? (size_t)shader->output_len : 0;
}

size_t openzt2_shader_input_count(const MOJOSHADER_parseData *shader)
{
    return shader->attribute_count > 0 ? (size_t)shader->attribute_count : 0;
}

int openzt2_shader_input_usage(const MOJOSHADER_parseData *shader, size_t index)
{
    return shader->attributes[index].usage;
}

unsigned int openzt2_shader_input_index(const MOJOSHADER_parseData *shader, size_t index)
{
    return (unsigned int)shader->attributes[index].index;
}

size_t openzt2_shader_vertex_output_count(const MOJOSHADER_parseData *shader)
{
    return shader->shader_type == MOJOSHADER_TYPE_VERTEX && shader->output_count > 0
            ? (size_t)shader->output_count : 0;
}

int openzt2_shader_vertex_output_usage(const MOJOSHADER_parseData *shader, size_t index)
{
    return shader->outputs[index].usage;
}

unsigned int openzt2_shader_vertex_output_index(const MOJOSHADER_parseData *shader, size_t index)
{
    return (unsigned int)shader->outputs[index].index;
}

int openzt2_shader_vertex_output_location(const MOJOSHADER_parseData *shader, size_t index)
{
    return MOJOSHADER_spirvVertexOutputLocation(shader,
            shader->outputs[index].usage, shader->outputs[index].index);
}

size_t openzt2_shader_symbol_count(const MOJOSHADER_parseData *shader)
{
    return shader->symbol_count > 0 ? (size_t)shader->symbol_count : 0;
}

unsigned int openzt2_shader_packed_uniform_source(const MOJOSHADER_parseData *shader,
        unsigned int type, unsigned int packed_index)
{
    for (int i = 0; i < shader->uniform_count; ++i) {
        const MOJOSHADER_uniform *u = &shader->uniforms[i];
        if ((unsigned int)u->type != type || u->constant) continue;
        unsigned int count = u->array_count > 0 ? (unsigned int)u->array_count : 1;
        if (packed_index < count) return (unsigned int)u->index + packed_index;
        packed_index -= count;
    }
    return ~0u;
}

const char *openzt2_shader_symbol_name(const MOJOSHADER_parseData *shader, size_t index)
{
    return shader->symbols[index].name;
}

unsigned int openzt2_shader_symbol_register_set(const MOJOSHADER_parseData *shader, size_t index)
{
    return (unsigned int)shader->symbols[index].register_set;
}

unsigned int openzt2_shader_symbol_register_index(const MOJOSHADER_parseData *shader, size_t index)
{
    return shader->symbols[index].register_index;
}

unsigned int openzt2_shader_symbol_register_count(const MOJOSHADER_parseData *shader, size_t index)
{
    return shader->symbols[index].register_count;
}

unsigned int openzt2_shader_symbol_is_row_major_matrix(const MOJOSHADER_parseData *shader, size_t index)
{
    return shader->symbols[index].info.parameter_class == MOJOSHADER_SYMCLASS_MATRIX_ROWS;
}

void openzt2_shader_free(const MOJOSHADER_parseData *shader)
{
    MOJOSHADER_freeParseData(shader);
}

void openzt2_shader_free_message(char *message)
{
    free(message);
}
