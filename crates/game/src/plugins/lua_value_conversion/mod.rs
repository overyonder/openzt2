use mlua::Value;

pub(super) fn convert_integral_lua_number_to_i64(lua_value: Value) -> Option<i64> {
    match lua_value {
        Value::Integer(lua_integer) => Some(lua_integer),
        Value::Number(lua_number)
            if lua_number.is_finite()
                && lua_number.fract() == 0.0
                && lua_number >= i64::MIN as f64
                && lua_number <= i64::MAX as f64 =>
        {
            Some(lua_number as i64)
        }
        _ => None,
    }
}
