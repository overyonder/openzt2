use super::AssetId;

impl AssetId {
    #[must_use]
    pub fn from_key(authored_asset_key: &str) -> Self {
        let asset_key_hash = blake3::hash(authored_asset_key.as_bytes());
        let mut stable_asset_identifier_bytes = [0; 16];
        stable_asset_identifier_bytes.copy_from_slice(&asset_key_hash.as_bytes()[..16]);
        Self(stable_asset_identifier_bytes)
    }

    #[must_use]
    pub fn from_virtual_path(virtual_asset_path: &str) -> Self {
        Self::from_key(virtual_asset_path)
    }

    #[must_use]
    pub fn to_lowercase_hexadecimal_string(self) -> String {
        const LOWERCASE_HEXADECIMAL_DIGITS: &[u8; 16] = b"0123456789abcdef";

        let mut hexadecimal_asset_identifier = String::with_capacity(32);
        for asset_identifier_byte in self.0 {
            hexadecimal_asset_identifier.push(char::from(
                LOWERCASE_HEXADECIMAL_DIGITS[(asset_identifier_byte >> 4) as usize],
            ));
            hexadecimal_asset_identifier.push(char::from(
                LOWERCASE_HEXADECIMAL_DIGITS[(asset_identifier_byte & 0x0f) as usize],
            ));
        }

        hexadecimal_asset_identifier
    }
}
