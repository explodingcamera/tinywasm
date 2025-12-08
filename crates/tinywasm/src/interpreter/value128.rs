#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
pub struct Value128(i128);

impl Value128 {
    pub const fn from_le_bytes(bytes: [u8; 16]) -> Self {
        Self(i128::from_le_bytes(bytes))
    }

    pub const fn to_le_bytes(self) -> [u8; 16] {
        self.0.to_le_bytes()
    }

    pub const fn reduce_or(self) -> u8 {
        let mut result = 0u8;
        let bytes = self.to_le_bytes();
        let mut i = 0;
        while i < 16 {
            result |= bytes[i];
            i += 1;
        }
        result
    }

    pub const fn swizzle(self, s: Self) -> Self {
        let a_bytes = self.to_le_bytes();
        let s_bytes = s.to_le_bytes();
        let mut result_bytes = [0u8; 16];
        let mut i = 0;
        while i < 16 {
            let index = s_bytes[i] as usize;
            result_bytes[i] = if index < 16 { a_bytes[index] } else { 0 };
            i += 1;
        }
        Self::from_le_bytes(result_bytes)
    }

    pub const fn extend_8_i8(src: i8) -> Self {
        let mut result_bytes = [0u8; 16];
        let mut i = 0;
        while i < 8 {
            result_bytes[i * 2] = src as u8;
            result_bytes[i * 2 + 1] = if src < 0 { 0xFF } else { 0x00 };
            i += 1;
        }
        Self::from_le_bytes(result_bytes)
    }

    pub const fn extend_8_u8(src: u8) -> Self {
        let mut result_bytes = [0u8; 16];
        let mut i = 0;
        while i < 8 {
            result_bytes[i * 2] = src;
            result_bytes[i * 2 + 1] = 0x00;
            i += 1;
        }
        Self::from_le_bytes(result_bytes)
    }

    pub const fn extend_4_i16(src: i16) -> Self {
        let mut result_bytes = [0u8; 16];
        let mut i = 0;
        while i < 4 {
            let bytes = src.to_le_bytes();
            result_bytes[i * 4] = bytes[0];
            result_bytes[i * 4 + 1] = bytes[1];
            result_bytes[i * 4 + 2] = if src < 0 { 0xFF } else { 0x00 };
            result_bytes[i * 4 + 3] = if src < 0 { 0xFF } else { 0x00 };
            i += 1;
        }
        Self::from_le_bytes(result_bytes)
    }

    pub const fn extend_4_u16(src: u16) -> Self {
        let mut result_bytes = [0u8; 16];
        let mut i = 0;
        while i < 4 {
            let bytes = src.to_le_bytes();
            result_bytes[i * 4] = bytes[0];
            result_bytes[i * 4 + 1] = bytes[1];
            result_bytes[i * 4 + 2] = 0x00;
            result_bytes[i * 4 + 3] = 0x00;
            i += 1;
        }
        Self::from_le_bytes(result_bytes)
    }

    pub const fn extend_2_i32(src: i32) -> Self {
        let mut result_bytes = [0u8; 16];
        let mut i = 0;
        while i < 2 {
            let bytes = src.to_le_bytes();
            result_bytes[i * 8] = bytes[0];
            result_bytes[i * 8 + 1] = bytes[1];
            result_bytes[i * 8 + 2] = bytes[2];
            result_bytes[i * 8 + 3] = bytes[3];
            result_bytes[i * 8 + 4] = if src < 0 { 0xFF } else { 0x00 };
            result_bytes[i * 8 + 5] = if src < 0 { 0xFF } else { 0x00 };
            result_bytes[i * 8 + 6] = if src < 0 { 0xFF } else { 0x00 };
            result_bytes[i * 8 + 7] = if src < 0 { 0xFF } else { 0x00 };
            i += 1;
        }
        Self::from_le_bytes(result_bytes)
    }

    pub const fn extend_2_u32(src: u32) -> Self {
        let mut result_bytes = [0u8; 16];
        let mut i = 0;
        while i < 2 {
            let bytes = src.to_le_bytes();
            result_bytes[i * 8] = bytes[0];
            result_bytes[i * 8 + 1] = bytes[1];
            result_bytes[i * 8 + 2] = bytes[2];
            result_bytes[i * 8 + 3] = bytes[3];
            result_bytes[i * 8 + 4] = 0x00;
            result_bytes[i * 8 + 5] = 0x00;
            result_bytes[i * 8 + 6] = 0x00;
            result_bytes[i * 8 + 7] = 0x00;
            i += 1;
        }
        Self::from_le_bytes(result_bytes)
    }

    pub const fn splat_i8(src: i8) -> Self {
        let mut result_bytes = [0u8; 16];
        let byte = src as u8;
        let mut i = 0;
        while i < 16 {
            result_bytes[i] = byte;
            i += 1;
        }
        Self::from_le_bytes(result_bytes)
    }

    pub const fn splat_i16(src: i16) -> Self {
        let mut result_bytes = [0u8; 16];
        let bytes = src.to_le_bytes();
        let mut i = 0;
        while i < 8 {
            result_bytes[i * 2] = bytes[0];
            result_bytes[i * 2 + 1] = bytes[1];
            i += 1;
        }
        Self::from_le_bytes(result_bytes)
    }

    pub const fn splat_i32(src: i32) -> Self {
        let mut result_bytes = [0u8; 16];
        let bytes = src.to_le_bytes();
        let mut i = 0;
        while i < 4 {
            result_bytes[i * 4] = bytes[0];
            result_bytes[i * 4 + 1] = bytes[1];
            result_bytes[i * 4 + 2] = bytes[2];
            result_bytes[i * 4 + 3] = bytes[3];
            i += 1;
        }
        Self::from_le_bytes(result_bytes)
    }

    pub const fn splat_i64(src: i64) -> Self {
        let mut result_bytes = [0u8; 16];
        let bytes = src.to_le_bytes();
        let mut i = 0;
        while i < 2 {
            result_bytes[i * 8] = bytes[0];
            result_bytes[i * 8 + 1] = bytes[1];
            result_bytes[i * 8 + 2] = bytes[2];
            result_bytes[i * 8 + 3] = bytes[3];
            result_bytes[i * 8 + 4] = bytes[4];
            result_bytes[i * 8 + 5] = bytes[5];
            result_bytes[i * 8 + 6] = bytes[6];
            result_bytes[i * 8 + 7] = bytes[7];
            i += 1;
        }
        Self::from_le_bytes(result_bytes)
    }
}

impl From<Value128> for i128 {
    fn from(val: Value128) -> Self {
        val.0
    }
}

impl From<i128> for Value128 {
    fn from(value: i128) -> Self {
        Self(value)
    }
}

impl core::ops::Not for Value128 {
    type Output = Self;
    fn not(self) -> Self::Output {
        Self(!self.0)
    }
}

impl core::ops::BitAnd for Value128 {
    type Output = Self;
    fn bitand(self, rhs: Self) -> Self::Output {
        Self(self.0 & rhs.0)
    }
}

impl core::ops::BitOr for Value128 {
    type Output = Self;
    fn bitor(self, rhs: Self) -> Self::Output {
        Self(self.0 | rhs.0)
    }
}

impl core::ops::BitXor for Value128 {
    type Output = Self;
    fn bitxor(self, rhs: Self) -> Self::Output {
        Self(self.0 ^ rhs.0)
    }
}
