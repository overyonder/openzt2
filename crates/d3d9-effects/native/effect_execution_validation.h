/* Parsed FX execution contracts. These are checked once before the owner is
 * returned to Rust; immutable programs and parameter-scratch metadata retain it. */
static int effect_storage_fits_native_allocator(size_t count, size_t element_size)
{
    return element_size && count <= (size_t)INT_MAX / element_size;
}

static int effect_span_fits(size_t start, size_t width, size_t count)
{
    return start <= count && width <= count - start;
}

static int effect_numeric_type_is_supported(const MOJOSHADER_symbolTypeInfo *type)
{
    if ((unsigned int)type->parameter_class > MOJOSHADER_SYMCLASS_MATRIX_COLUMNS
            || type->parameter_type < MOJOSHADER_SYMTYPE_BOOL
            || type->parameter_type > MOJOSHADER_SYMTYPE_FLOAT
            || !type->rows || type->rows > 4 || !type->columns || type->columns > 4
            || type->member_count)
        return 0;
    if ((type->parameter_class == MOJOSHADER_SYMCLASS_SCALAR && (type->rows != 1 || type->columns != 1))
            || (type->parameter_class == MOJOSHADER_SYMCLASS_VECTOR && type->rows != 1))
        return 0;
    return 1;
}

static int effect_numeric_storage_is_padded(const MOJOSHADER_effectValue *value)
{
    const MOJOSHADER_symbolTypeInfo *type = &value->type;
    if (!effect_numeric_type_is_supported(type) || !value->values
            || !effect_storage_fits_native_allocator(value->value_count, sizeof(float)))
        return 0;
    const size_t elements = type->elements ? type->elements : 1;
    return elements <= (size_t)INT_MAX / (type->rows * 4 * sizeof(float))
            && value->value_count == elements * type->rows * 4;
}

static int effect_aggregate_storage_is_padded(const MOJOSHADER_effectValue *value)
{
    const MOJOSHADER_symbolTypeInfo *type = &value->type;
    if (type->parameter_type != MOJOSHADER_SYMTYPE_VOID
            || !type->member_count || !type->members || !value->values
            || !effect_storage_fits_native_allocator(type->member_count, sizeof(MOJOSHADER_symbolStructMember))
            || !effect_storage_fits_native_allocator(value->value_count, sizeof(float)))
        return 0;
    size_t components = 0;
    for (unsigned int i = 0; i < type->member_count; ++i)
    {
        const MOJOSHADER_symbolStructMember *member = &type->members[i];
        if (!member->name || !effect_numeric_type_is_supported(&member->info))
            return 0;
        const size_t elements = member->info.elements ? member->info.elements : 1;
        if (elements > value->value_count / (member->info.rows * 4))
            return 0;
        const size_t width = elements * member->info.rows * 4;
        if (!effect_span_fits(components, width, value->value_count))
            return 0;
        components += width;
    }
    const size_t elements = type->elements ? type->elements : 1;
    return components && elements <= value->value_count / components
            && components * elements == value->value_count
            && type->rows == 1 && type->columns == components;
}

static const char *validate_effect_symbol_binding(const MOJOSHADER_effect *effect,
        const MOJOSHADER_symbol *symbol, unsigned int parameter_index,
        size_t float_registers, int preshader_input, int numeric_state_input)
{
    if (parameter_index >= (unsigned int)effect->param_count)
        return "effect binding parameter index is out of range";
    const MOJOSHADER_effectValue *value = &effect->params[parameter_index].value;
    if (!symbol->name || !value->name || strcmp(symbol->name, value->name) || !symbol->register_count)
        return "effect binding name or register count is invalid";
    if (symbol->register_set == MOJOSHADER_SYMREGSET_SAMPLER && !preshader_input)
    {
        if (value->type.parameter_class != MOJOSHADER_SYMCLASS_OBJECT
                || value->type.parameter_type < MOJOSHADER_SYMTYPE_SAMPLER
                || value->type.parameter_type > MOJOSHADER_SYMTYPE_SAMPLERCUBE
                || symbol->info.parameter_class != MOJOSHADER_SYMCLASS_OBJECT
                || symbol->info.parameter_type < MOJOSHADER_SYMTYPE_SAMPLER
                || symbol->info.parameter_type > MOJOSHADER_SYMTYPE_SAMPLERCUBE
                || !effect_span_fits(symbol->register_index, symbol->register_count, 16)
                || (value->value_count && !value->valuesSS)
                || !effect_storage_fits_native_allocator(value->value_count, sizeof(MOJOSHADER_effectSamplerState)))
            return "effect sampler binding storage is unsupported";
        return NULL; /* sampler records are not copied by copy_parameter_data */
    }
    /* Flat aggregates retain padded logical rows; CTAB separately owns register
     * majority and field boundaries. State expressions require numeric inputs. */
    if (value->type.parameter_class == MOJOSHADER_SYMCLASS_STRUCT)
    {
        if (numeric_state_input || symbol->info.parameter_class != MOJOSHADER_SYMCLASS_STRUCT
                || symbol->info.parameter_type != MOJOSHADER_SYMTYPE_VOID
                || symbol->register_set != MOJOSHADER_SYMREGSET_FLOAT4
                || !effect_aggregate_storage_is_padded(value)
                || !symbol->info.member_count || !symbol->info.members
                || !effect_storage_fits_native_allocator(symbol->info.member_count, sizeof(MOJOSHADER_symbolStructMember))
                || !effect_span_fits(symbol->register_index, symbol->register_count, float_registers)
                || symbol->info.member_count != value->type.member_count
                || (symbol->info.elements ? symbol->info.elements : 1)
                    != (value->type.elements ? value->type.elements : 1))
            return "effect aggregate binding storage is unsupported";
        size_t registers = 0;
        for (unsigned int i = 0; i < value->type.member_count; ++i)
        {
            const MOJOSHADER_symbolStructMember *member = &value->type.members[i];
            const MOJOSHADER_symbolStructMember *binding = &symbol->info.members[i];
            const MOJOSHADER_symbolTypeInfo *source = &member->info;
            const MOJOSHADER_symbolTypeInfo *target = &binding->info;
            const int source_matrix = source->parameter_class == MOJOSHADER_SYMCLASS_MATRIX_ROWS
                    || source->parameter_class == MOJOSHADER_SYMCLASS_MATRIX_COLUMNS;
            const int target_matrix = target->parameter_class == MOJOSHADER_SYMCLASS_MATRIX_ROWS
                    || target->parameter_class == MOJOSHADER_SYMCLASS_MATRIX_COLUMNS;
            if (!binding->name || strcmp(member->name, binding->name)
                    || !effect_numeric_type_is_supported(target)
                    || source->parameter_type != target->parameter_type
                    || source->rows != target->rows || source->columns != target->columns
                    || (source->elements ? source->elements : 1) != (target->elements ? target->elements : 1)
                    || (source_matrix ? !target_matrix : source->parameter_class != target->parameter_class))
                return "effect aggregate member binding metadata is incompatible";
            const size_t rows = target->parameter_class == MOJOSHADER_SYMCLASS_MATRIX_COLUMNS
                    ? target->columns : target->rows;
            const size_t elements = target->elements ? target->elements : 1;
            /* A matrix can expand to at most four register rows per native row.
             * Bound the complete metadata independently of the used prefix. */
            if (elements > value->value_count / rows
                    || !effect_span_fits(registers, rows * elements, value->value_count))
                return "effect aggregate member register span is invalid";
            registers += rows * elements;
        }
        const size_t elements = value->type.elements ? value->type.elements : 1;
        /* CTAB describes every field but binds only through the highest used
         * register. The copier limits every write to this validated prefix. */
        if (!registers || elements > value->value_count / registers
                || symbol->register_count > registers * elements)
            return "effect aggregate binding exceeds declared register layout";
        return NULL;
    }
    if (!effect_numeric_storage_is_padded(value)
            || (unsigned int)symbol->info.parameter_class > MOJOSHADER_SYMCLASS_MATRIX_COLUMNS
            || symbol->info.parameter_type < MOJOSHADER_SYMTYPE_BOOL
            || symbol->info.parameter_type > MOJOSHADER_SYMTYPE_FLOAT
            || !symbol->info.rows || symbol->info.rows > 4
            || !symbol->info.columns || symbol->info.columns > 4
            || symbol->info.member_count)
        return "effect binding requires supported numeric storage and type metadata";
    if (preshader_input && symbol->register_set != MOJOSHADER_SYMREGSET_FLOAT4)
        return "preshader inputs require float register bindings";
    if (symbol->register_set == MOJOSHADER_SYMREGSET_FLOAT4
            || symbol->register_set == MOJOSHADER_SYMREGSET_INT4)
    {
        const size_t limit = symbol->register_set == MOJOSHADER_SYMREGSET_FLOAT4 ? float_registers : 16;
        if (!effect_span_fits(symbol->register_index, symbol->register_count, limit)
                || !effect_span_fits(0, (size_t)symbol->register_count * 4, value->value_count)
                || (symbol->register_set != MOJOSHADER_SYMREGSET_FLOAT4
                    && value->type.parameter_type == MOJOSHADER_SYMTYPE_FLOAT))
            return "effect binding register or parameter storage span is invalid";
    }
    else if (symbol->register_set == MOJOSHADER_SYMREGSET_BOOL)
    {
        const size_t rows = ((size_t)symbol->register_count + value->type.columns - 1) / value->type.columns;
        if (!effect_span_fits(symbol->register_index, symbol->register_count, 16)
                || rows > value->value_count / 4
                || value->type.parameter_type == MOJOSHADER_SYMTYPE_FLOAT)
            return "effect boolean binding register or parameter storage span is invalid";
    }
    else
        return "effect binding register set is unsupported";
    return NULL;
}

static unsigned int supported_preshader_operand_count(MOJOSHADER_preshaderOpcode opcode)
{
    switch (opcode)
    {
        case MOJOSHADER_PRESHADEROP_MOV: case MOJOSHADER_PRESHADEROP_NEG:
        case MOJOSHADER_PRESHADEROP_RCP: case MOJOSHADER_PRESHADEROP_FRC:
        case MOJOSHADER_PRESHADEROP_EXP: case MOJOSHADER_PRESHADEROP_LOG:
        case MOJOSHADER_PRESHADEROP_RSQ: case MOJOSHADER_PRESHADEROP_SIN:
        case MOJOSHADER_PRESHADEROP_COS: case MOJOSHADER_PRESHADEROP_ASIN:
        case MOJOSHADER_PRESHADEROP_ACOS: case MOJOSHADER_PRESHADEROP_ATAN:
            return 2;
        case MOJOSHADER_PRESHADEROP_MIN: case MOJOSHADER_PRESHADEROP_MAX:
        case MOJOSHADER_PRESHADEROP_LT: case MOJOSHADER_PRESHADEROP_GE:
        case MOJOSHADER_PRESHADEROP_ADD: case MOJOSHADER_PRESHADEROP_MUL:
        case MOJOSHADER_PRESHADEROP_ATAN2: case MOJOSHADER_PRESHADEROP_DIV:
        case MOJOSHADER_PRESHADEROP_DOT:
        case MOJOSHADER_PRESHADEROP_MIN_SCALAR: case MOJOSHADER_PRESHADEROP_MAX_SCALAR:
        case MOJOSHADER_PRESHADEROP_LT_SCALAR: case MOJOSHADER_PRESHADEROP_GE_SCALAR:
        case MOJOSHADER_PRESHADEROP_ADD_SCALAR: case MOJOSHADER_PRESHADEROP_MUL_SCALAR:
        case MOJOSHADER_PRESHADEROP_ATAN2_SCALAR: case MOJOSHADER_PRESHADEROP_DIV_SCALAR:
            return 3;
        case MOJOSHADER_PRESHADEROP_CMP:
            return 4;
        default:
            return 0; /* pinned evaluator does not implement MOVC/NOISE/DOT_SCALAR */
    }
}

static const char *validate_effect_preshader_execution(const MOJOSHADER_effect *effect,
        const MOJOSHADER_preshader *program, const unsigned int *parameters,
        unsigned int parameter_count, size_t output_components, int numeric_state_input)
{
    if (!program || !program->malloc || !program->free
            || parameter_count != program->symbol_count
            || (parameter_count && (!parameters || !program->symbols))
            || (program->literal_count && !program->literals)
            || (program->instruction_count && !program->instructions)
            || (program->register_count && !program->registers)
            || !effect_storage_fits_native_allocator(parameter_count, sizeof(unsigned int))
            || !effect_storage_fits_native_allocator(program->symbol_count, sizeof(MOJOSHADER_symbol))
            || !effect_storage_fits_native_allocator(program->literal_count, sizeof(double))
            || !effect_storage_fits_native_allocator(program->instruction_count, sizeof(MOJOSHADER_preshaderInstruction))
            || !effect_storage_fits_native_allocator(program->temp_count, sizeof(double))
            || !effect_storage_fits_native_allocator(program->register_count, 4 * sizeof(float)))
        return "preshader storage or dependency metadata is invalid";
    for (unsigned int i = 0; i < parameter_count; ++i)
    {
        const char *error = validate_effect_symbol_binding(effect, &program->symbols[i],
                parameters[i], program->register_count, 1, numeric_state_input);
        if (error)
            return error;
    }
    for (unsigned int i = 0; i < program->instruction_count; ++i)
    {
        const MOJOSHADER_preshaderInstruction *instruction = &program->instructions[i];
        const unsigned int arity = supported_preshader_operand_count(instruction->opcode);
        if (!arity || instruction->operand_count != arity
                || !instruction->element_count || instruction->element_count > 4)
            return "preshader opcode, arity or element width is unsupported";
        for (unsigned int j = 0; j < arity; ++j)
        {
            const MOJOSHADER_preshaderOperand *operand = &instruction->operands[j];
            const size_t width = j == 0 && instruction->opcode >= MOJOSHADER_PRESHADEROP_SCALAR_OPS
                    ? 1 : instruction->element_count;
            if (operand->array_register_count)
                return "preshader indirect array operands are unsupported";
            if (j == arity - 1 && operand->type != MOJOSHADER_PRESHADEROPERAND_TEMP
                    && operand->type != MOJOSHADER_PRESHADEROPERAND_OUTPUT)
                return "preshader destination is not writable storage";
            switch (operand->type)
            {
                case MOJOSHADER_PRESHADEROPERAND_LITERAL:
                    if (!effect_span_fits(operand->index, width, program->literal_count))
                        return "preshader literal operand span is invalid";
                    break;
                case MOJOSHADER_PRESHADEROPERAND_INPUT:
                {
                    int covered = 0;
                    for (unsigned int k = 0; k < program->symbol_count; ++k)
                    {
                        const MOJOSHADER_symbol *symbol = &program->symbols[k];
                        const size_t base = (size_t)symbol->register_index * 4;
                        if (operand->index >= base
                                && effect_span_fits(operand->index - base, width, (size_t)symbol->register_count * 4))
                            covered = 1;
                    }
                    if (!covered)
                        return "preshader input operand span escapes its binding";
                    break;
                }
                case MOJOSHADER_PRESHADEROPERAND_OUTPUT:
                    if (!effect_span_fits(operand->index, width, output_components))
                        return "preshader output operand span is invalid";
                    break;
                case MOJOSHADER_PRESHADEROPERAND_TEMP:
                    if (!effect_span_fits(operand->index, width, program->temp_count))
                        return "preshader temporary operand span is invalid";
                    break;
                default:
                    return "preshader operand storage is unsupported";
            }
        }
    }
    return NULL;
}

static const char *validate_effect_pass_sampler_storage(const MOJOSHADER_effect *effect,
        const MOJOSHADER_effectValue *sampler)
{
    /* BeginPass reads the first word even when the sampler has no states. */
    if (sampler->type.parameter_class != MOJOSHADER_SYMCLASS_OBJECT
            || sampler->type.parameter_type < MOJOSHADER_SYMTYPE_SAMPLER
            || sampler->type.parameter_type > MOJOSHADER_SYMTYPE_SAMPLERCUBE
            || !sampler->valuesSS || !effect_span_fits(0, 1, sampler->value_count)
            || !effect_storage_fits_native_allocator(sampler->value_count, sizeof(MOJOSHADER_effectSamplerState)))
        return "pass sampler parameter storage is unsupported";
    for (unsigned int i = 0; i < sampler->value_count; ++i)
    {
        const MOJOSHADER_effectSamplerState *state = &sampler->valuesSS[i];
        const MOJOSHADER_effectValue *value = &state->value;
        if (state->type != MOJOSHADER_SAMP_TEXTURE)
        {
            if (!effect_numeric_storage_is_padded(value))
                return "pass sampler numeric state storage is unsupported";
            continue;
        }
        if (value->type.parameter_class != MOJOSHADER_SYMCLASS_OBJECT
                || value->type.parameter_type < MOJOSHADER_SYMTYPE_TEXTURE
                || value->type.parameter_type > MOJOSHADER_SYMTYPE_TEXTURECUBE
                || !value->valuesI || !effect_span_fits(0, 1, value->value_count)
                || !effect_storage_fits_native_allocator(value->value_count, sizeof(unsigned int))
                || value->valuesI[0] < 0 || value->valuesI[0] >= effect->object_count)
            return "pass sampler texture state storage or index is invalid";
        const MOJOSHADER_effectObject *object = &effect->objects[value->valuesI[0]];
        /* Texture states can share a mapping with a texture or sampler object.
         * A zero-initialized void object has no texture binding. */
        if (object->type != MOJOSHADER_SYMTYPE_VOID
                && (object->type < MOJOSHADER_SYMTYPE_TEXTURE
                    || object->type > MOJOSHADER_SYMTYPE_SAMPLERCUBE))
            return "pass sampler texture object is not mapping storage";
    }
    return NULL;
}

static const char *validate_effect_state_execution(const struct openzt2_effect *owner,
        const MOJOSHADER_effectState *state)
{
    const MOJOSHADER_effectValue *value = &state->value;
    const int parameter = fixed_state_parameter(owner, state);
    if (state->parameter && parameter < 0)
        return "effect state parameter is unresolved";
    const MOJOSHADER_effectValue *source = parameter >= 0 ? &owner->effect->params[parameter].value : value;
    if ((unsigned int)state->type == 178)
    {
        if (!value->values || !effect_span_fits(0, 1, value->value_count)
                || !effect_storage_fits_native_allocator(value->value_count, effect_value_storage_element_size(value)))
            return "pass sampler state storage is invalid";
        if (parameter >= 0)
        {
            const char *error = validate_effect_pass_sampler_storage(owner->effect, source);
            if (error)
                return error;
        }
    }
    if (value->type.parameter_class <= MOJOSHADER_SYMCLASS_MATRIX_COLUMNS)
    {
        if (!effect_numeric_storage_is_padded(value) || !effect_numeric_storage_is_padded(source)
                || value->type.parameter_type != source->type.parameter_type)
            return "numeric effect state layout or direct parameter storage is unsupported";
        const size_t logical_components = source->type.parameter_class == MOJOSHADER_SYMCLASS_MATRIX_ROWS
                || source->type.parameter_class == MOJOSHADER_SYMCLASS_MATRIX_COLUMNS
                ? (size_t)source->value_count / 4 * source->type.columns : source->value_count;
        if (logical_components > value->value_count)
            return "direct effect state cannot hold the complete numeric parameter";
    }
    else if (state->parameter && (!value->values || !value->value_count
                || !source->values || !source->value_count))
        return "effect object state storage is invalid";
    if (!state->preshader && (state->preshader_param_count || state->preshader_params))
        return "effect state has bindings without an expression";
    if (state->preshader)
    {
        if (value->type.parameter_class > MOJOSHADER_SYMCLASS_MATRIX_COLUMNS || value->value_count > 16)
            return "state preshader output layout is unsupported";
        return validate_effect_preshader_execution(owner->effect, state->preshader,
                state->preshader_params, state->preshader_param_count, value->value_count, 1);
    }
    if (state->type == MOJOSHADER_RS_VERTEXSHADER || state->type == MOJOSHADER_RS_PIXELSHADER)
    {
        const MOJOSHADER_symbolType expected = state->type == MOJOSHADER_RS_VERTEXSHADER
                ? MOJOSHADER_SYMTYPE_VERTEXSHADER : MOJOSHADER_SYMTYPE_PIXELSHADER;
        if (source->type.parameter_class != MOJOSHADER_SYMCLASS_OBJECT
                || source->type.parameter_type != expected
                || value->type.parameter_class != MOJOSHADER_SYMCLASS_OBJECT
                || value->value_count != 1 || !value->valuesI
                || !source->valuesI || source->value_count != 1
                || source->valuesI[0] < 0 || source->valuesI[0] >= owner->effect->object_count)
            return "effect shader state object storage or index is invalid";
        const MOJOSHADER_effectObject *object = &owner->effect->objects[source->valuesI[0]];
        if (object->type != expected && object->type != MOJOSHADER_SYMTYPE_VOID)
            return "effect shader state object type is invalid";
    }
    return NULL;
}

static const char *validate_parsed_effect_execution(const struct openzt2_effect *owner)
{
    const MOJOSHADER_effect *effect = owner->effect;
    if (effect->param_count < 0 || effect->technique_count < 0 || effect->object_count < 0
            || (effect->param_count && !effect->params)
            || (effect->technique_count && !effect->techniques)
            || (effect->object_count && !effect->objects)
            || !effect_storage_fits_native_allocator(effect->param_count, sizeof(MOJOSHADER_effectParam))
            || !effect_storage_fits_native_allocator(effect->technique_count, sizeof(MOJOSHADER_effectTechnique))
            || !effect_storage_fits_native_allocator(effect->object_count, sizeof(MOJOSHADER_effectObject)))
        return "effect execution collections are invalid";
    for (int i = 0; i < effect->param_count; ++i)
    {
        const MOJOSHADER_effectValue *value = &effect->params[i].value;
        if (!value->name || (value->value_count && !value->values)
                || !effect_storage_fits_native_allocator(value->value_count, effect_value_storage_element_size(value)))
            return "effect parameter storage is invalid";
        if (value->type.parameter_class <= MOJOSHADER_SYMCLASS_MATRIX_COLUMNS
                && !effect_numeric_storage_is_padded(value))
            return "effect numeric parameter layout is unsupported";
        if (value->type.parameter_class == MOJOSHADER_SYMCLASS_STRUCT
                && !effect_aggregate_storage_is_padded(value))
            return "effect aggregate parameter layout is unsupported";
    }
    for (int i = 0; i < effect->object_count; ++i)
    {
        const MOJOSHADER_effectObject *object = &effect->objects[i];
        if (object->type != MOJOSHADER_SYMTYPE_VERTEXSHADER && object->type != MOJOSHADER_SYMTYPE_PIXELSHADER)
            continue;
        const MOJOSHADER_effectShader *shader = &object->shader;
        if (shader->is_preshader)
            return "effect shader-array selectors are unsupported";
        if (!shader->shader)
            continue;
        const MOJOSHADER_parseData *data = effect_shader_parse_data(shader->shader);
        if (!data || data->error_count || shader->param_count != (unsigned int)data->symbol_count
                || (shader->param_count && (!shader->params || !data->symbols))
                || !effect_storage_fits_native_allocator(shader->param_count, sizeof(unsigned int))
                || !effect_storage_fits_native_allocator(shader->param_count, sizeof(MOJOSHADER_symbol)))
            return "effect shader binding metadata is invalid";
        if ((shader->sampler_count && !shader->samplers)
                || !effect_storage_fits_native_allocator(shader->sampler_count, sizeof(MOJOSHADER_samplerStateRegister)))
            return "effect shader sampler metadata is invalid";
        unsigned int sampler = 0;
        for (unsigned int j = 0; j < shader->param_count; ++j)
        {
            const char *error = validate_effect_symbol_binding(effect, &data->symbols[j], shader->params[j], 256, 0, 0);
            if (error)
                return error;
            if (data->symbols[j].register_set == MOJOSHADER_SYMREGSET_SAMPLER)
            {
                const MOJOSHADER_effectValue *value = &effect->params[shader->params[j]].value;
                if (sampler >= shader->sampler_count
                        || shader->samplers[sampler].sampler_register != data->symbols[j].register_index
                        || shader->samplers[sampler].sampler_state_count != value->value_count
                        || shader->samplers[sampler].sampler_states != value->valuesSS)
                    return "effect shader sampler binding differs from its parameter";
                ++sampler;
            }
        }
        if (sampler != shader->sampler_count)
            return "effect shader sampler binding count is invalid";
        if (!data->preshader && (shader->preshader_param_count || shader->preshader_params))
            return "effect shader has bindings without an expression";
        if (data->preshader)
        {
            const char *error = validate_effect_preshader_execution(effect, data->preshader,
                    shader->preshader_params, shader->preshader_param_count, 256 * 4, 0);
            if (error)
                return error;
        }
    }
    for (int i = 0; i < effect->technique_count; ++i)
    {
        const MOJOSHADER_effectTechnique *technique = &effect->techniques[i];
        if ((technique->pass_count && !technique->passes)
                || !effect_storage_fits_native_allocator(technique->pass_count, sizeof(MOJOSHADER_effectPass)))
            return "effect pass storage is invalid";
        for (unsigned int j = 0; j < technique->pass_count; ++j)
        {
            const MOJOSHADER_effectPass *pass = &technique->passes[j];
            if ((pass->state_count && !pass->states)
                    || !effect_storage_fits_native_allocator(pass->state_count, sizeof(MOJOSHADER_effectState)))
                return "effect state storage is invalid";
            for (unsigned int k = 0; k < pass->state_count; ++k)
            {
                const char *error = validate_effect_state_execution(owner, &pass->states[k]);
                if (error)
                    return error;
            }
        }
    }
    return NULL;
}
