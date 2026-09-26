use std::{io, sync::Arc};

use bevy::{
    asset::{io::Reader, AssetApp, AssetLoader, LoadContext},
    prelude::*,
};

#[derive(Asset, TypePath, Debug)]
pub(crate) struct LuaScriptAsset {
    normalized_source_or_bytecode_bytes: Arc<[u8]>,
}

impl LuaScriptAsset {
    pub(crate) fn normalized_source_or_bytecode_bytes(&self) -> &[u8] {
        &self.normalized_source_or_bytecode_bytes
    }
}

#[derive(Default, TypePath)]
struct LuaScriptAssetLoader;

impl AssetLoader for LuaScriptAssetLoader {
    type Asset = LuaScriptAsset;
    type Settings = ();
    type Error = io::Error;

    async fn load(
        &self,
        reader: &mut dyn Reader,
        _: &Self::Settings,
        load_context: &mut LoadContext<'_>,
    ) -> Result<Self::Asset, Self::Error> {
        let mut bytes = Vec::new();
        reader.read_to_end(&mut bytes).await?;
        let source_is_compiled_bytecode =
            load_context
                .path()
                .path()
                .extension()
                .is_some_and(|extension| {
                    extension.eq_ignore_ascii_case("bin") || extension.eq_ignore_ascii_case("luac")
                });
        let bytes = if source_is_compiled_bytecode {
            lunify::unify(
                &bytes,
                &lunify::Format::default(),
                &lunify::Settings::default(),
            )
            .map_err(|error| io::Error::new(io::ErrorKind::InvalidData, format!("{error:?}")))?
        } else {
            bytes
        };
        Ok(LuaScriptAsset {
            normalized_source_or_bytecode_bytes: bytes.into(),
        })
    }

    fn extensions(&self) -> &[&str] {
        &["lua", "bin", "luac"]
    }
}

pub(super) fn register_lua_script_asset_and_loader(application: &mut App) {
    application
        .init_asset::<LuaScriptAsset>()
        .init_asset_loader::<LuaScriptAssetLoader>();
}

#[cfg(test)]
mod stripped_bytecode_tests {
    #[test]
    fn stripped_lua50_instructions_execute_without_debug_lines(
    ) -> Result<(), Box<dyn std::error::Error>> {
        // Lua 5.0, little endian, 32-bit int/size_t/instructions, double numbers.
        let mut bytes = b"\x1bLua\x50\x01\x04\x04\x04\x06\x08\x09\x09\x08".to_vec();
        bytes.extend_from_slice(&0x417d_f5e7_6893_09b6_u64.to_le_bytes());
        bytes.extend_from_slice(&0_u32.to_le_bytes()); // no source name
        bytes.extend_from_slice(&0_u32.to_le_bytes()); // definition line
        bytes.extend_from_slice(&[0, 0, 0, 2]); // upvalues, parameters, vararg, stack
        for _ in 0..3 {
            bytes.extend_from_slice(&0_u32.to_le_bytes());
        } // stripped debug tables
        bytes.extend_from_slice(&1_u32.to_le_bytes()); // one constant
        bytes.push(3); // number
        bytes.extend_from_slice(&42_f64.to_le_bytes());
        bytes.extend_from_slice(&0_u32.to_le_bytes()); // no nested functions
        bytes.extend_from_slice(&2_u32.to_le_bytes()); // LOADK R0 K0; RETURN R0 2
        bytes.extend_from_slice(&1_u32.to_le_bytes());
        bytes.extend_from_slice(&(27_u32 | (2 << 15)).to_le_bytes());
        let translated = lunify::unify(&bytes, &Default::default(), &Default::default())
            .map_err(|error| std::io::Error::other(format!("{error:?}")))?;
        assert_eq!(mlua::Lua::new().load(&translated).eval::<i32>()?, 42);
        Ok(())
    }

    #[test]
    fn stripped_closure_upvalues_survive_format_conversion(
    ) -> Result<(), Box<dyn std::error::Error>> {
        let lua = mlua::Lua::new();
        let source = lua
            .load("local value = 42; return function() return value end")
            .into_function()?;
        let stripped = source.dump(true);
        let narrow_format = lunify::Format {
            size_t_width: lunify::BitWidth::Bit32,
            ..Default::default()
        };
        let narrow = lunify::unify(&stripped, &narrow_format, &Default::default())
            .map_err(|error| std::io::Error::other(format!("{error:?}")))?;
        let native = lunify::unify(&narrow, &Default::default(), &Default::default())
            .map_err(|error| std::io::Error::other(format!("{error:?}")))?;
        let closure: mlua::Function = lua.load(&native).eval()?;
        assert_eq!(closure.call::<i32>(())?, 42);
        Ok(())
    }
}
