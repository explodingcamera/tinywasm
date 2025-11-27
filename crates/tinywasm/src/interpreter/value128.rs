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
