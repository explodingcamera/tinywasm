#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
pub struct Value128(i128);

impl Value128 {
    pub const fn from_le_bytes(bytes: [u8; 16]) -> Self {
        Self(i128::from_le_bytes(bytes))
    }

    pub const fn to_le_bytes(self) -> [u8; 16] {
        self.0.to_le_bytes()
    }

    #[rustfmt::skip]
    pub const fn as_i8x16(self) -> [i8; 16] {
        let b = self.to_le_bytes();
        [b[0] as i8, b[1] as i8, b[2] as i8, b[3] as i8, b[4] as i8, b[5] as i8, b[6] as i8, b[7] as i8, b[8] as i8, b[9] as i8, b[10] as i8, b[11] as i8, b[12] as i8, b[13] as i8, b[14] as i8, b[15] as i8]
    }

    #[rustfmt::skip]
    pub const fn as_u8x16(self) -> [u8; 16] {
        self.to_le_bytes()
    }

    #[rustfmt::skip]
    pub const fn from_i8x16(x: [i8; 16]) -> Self {
        Self::from_le_bytes([x[0] as u8, x[1] as u8, x[2] as u8, x[3] as u8, x[4] as u8, x[5] as u8, x[6] as u8, x[7] as u8, x[8] as u8, x[9] as u8, x[10] as u8, x[11] as u8, x[12] as u8, x[13] as u8, x[14] as u8, x[15] as u8])
    }

    #[rustfmt::skip]
    pub const fn from_u8x16(x: [u8; 16]) -> Self {
        Self::from_le_bytes(x)
    }

    #[rustfmt::skip]
    pub const fn as_i16x8(self) -> [i16; 8] {
        let b = self.to_le_bytes();
        [i16::from_le_bytes([b[0], b[1]]), i16::from_le_bytes([b[2], b[3]]), i16::from_le_bytes([b[4], b[5]]), i16::from_le_bytes([b[6], b[7]]), i16::from_le_bytes([b[8], b[9]]), i16::from_le_bytes([b[10], b[11]]), i16::from_le_bytes([b[12], b[13]]), i16::from_le_bytes([b[14], b[15]])]
    }

    #[rustfmt::skip]
    pub const fn as_u16x8(self) -> [u16; 8] {
        let b = self.to_le_bytes();
        [u16::from_le_bytes([b[0], b[1]]), u16::from_le_bytes([b[2], b[3]]), u16::from_le_bytes([b[4], b[5]]), u16::from_le_bytes([b[6], b[7]]), u16::from_le_bytes([b[8], b[9]]), u16::from_le_bytes([b[10], b[11]]), u16::from_le_bytes([b[12], b[13]]), u16::from_le_bytes([b[14], b[15]])]
    }

    #[rustfmt::skip]
    pub const fn from_i16x8(x: [i16; 8]) -> Self {
        Self::from_le_bytes([x[0].to_le_bytes()[0], x[0].to_le_bytes()[1], x[1].to_le_bytes()[0], x[1].to_le_bytes()[1], x[2].to_le_bytes()[0], x[2].to_le_bytes()[1], x[3].to_le_bytes()[0], x[3].to_le_bytes()[1], x[4].to_le_bytes()[0], x[4].to_le_bytes()[1], x[5].to_le_bytes()[0], x[5].to_le_bytes()[1], x[6].to_le_bytes()[0], x[6].to_le_bytes()[1], x[7].to_le_bytes()[0], x[7].to_le_bytes()[1]])
    }

    #[rustfmt::skip]
    pub const fn from_u16x8(x: [u16; 8]) -> Self {
        Self::from_le_bytes([x[0].to_le_bytes()[0], x[0].to_le_bytes()[1], x[1].to_le_bytes()[0], x[1].to_le_bytes()[1], x[2].to_le_bytes()[0], x[2].to_le_bytes()[1], x[3].to_le_bytes()[0], x[3].to_le_bytes()[1], x[4].to_le_bytes()[0], x[4].to_le_bytes()[1], x[5].to_le_bytes()[0], x[5].to_le_bytes()[1], x[6].to_le_bytes()[0], x[6].to_le_bytes()[1], x[7].to_le_bytes()[0], x[7].to_le_bytes()[1]])
    }

    #[rustfmt::skip]
    pub const fn as_i32x4(self) -> [i32; 4] {
        let b = self.to_le_bytes();
        [i32::from_le_bytes([b[0], b[1], b[2], b[3]]), i32::from_le_bytes([b[4], b[5], b[6], b[7]]), i32::from_le_bytes([b[8], b[9], b[10], b[11]]), i32::from_le_bytes([b[12], b[13], b[14], b[15]])]
    }

    #[rustfmt::skip]
    pub const fn as_u32x4(self) -> [u32; 4] {
        let b = self.to_le_bytes();
        [u32::from_le_bytes([b[0], b[1], b[2], b[3]]), u32::from_le_bytes([b[4], b[5], b[6], b[7]]), u32::from_le_bytes([b[8], b[9], b[10], b[11]]), u32::from_le_bytes([b[12], b[13], b[14], b[15]])]
    }

    #[rustfmt::skip]
    pub const fn as_f32x4(self) -> [f32; 4] {
        let b = self.to_le_bytes();
        [f32::from_bits(u32::from_le_bytes([b[0], b[1], b[2], b[3]])), f32::from_bits(u32::from_le_bytes([b[4], b[5], b[6], b[7]])), f32::from_bits(u32::from_le_bytes([b[8], b[9], b[10], b[11]])), f32::from_bits(u32::from_le_bytes([b[12], b[13], b[14], b[15]]))]
    }

    #[rustfmt::skip]
    pub const fn from_i32x4(x: [i32; 4]) -> Self {
        Self::from_le_bytes([x[0].to_le_bytes()[0], x[0].to_le_bytes()[1], x[0].to_le_bytes()[2], x[0].to_le_bytes()[3], x[1].to_le_bytes()[0], x[1].to_le_bytes()[1], x[1].to_le_bytes()[2], x[1].to_le_bytes()[3], x[2].to_le_bytes()[0], x[2].to_le_bytes()[1], x[2].to_le_bytes()[2], x[2].to_le_bytes()[3], x[3].to_le_bytes()[0], x[3].to_le_bytes()[1], x[3].to_le_bytes()[2], x[3].to_le_bytes()[3]])
    }

    #[rustfmt::skip]
    pub const fn from_u32x4(x: [u32; 4]) -> Self {
        Self::from_le_bytes([x[0].to_le_bytes()[0], x[0].to_le_bytes()[1], x[0].to_le_bytes()[2], x[0].to_le_bytes()[3], x[1].to_le_bytes()[0], x[1].to_le_bytes()[1], x[1].to_le_bytes()[2], x[1].to_le_bytes()[3], x[2].to_le_bytes()[0], x[2].to_le_bytes()[1], x[2].to_le_bytes()[2], x[2].to_le_bytes()[3], x[3].to_le_bytes()[0], x[3].to_le_bytes()[1], x[3].to_le_bytes()[2], x[3].to_le_bytes()[3]])
    }

    #[rustfmt::skip]
    pub const fn from_f32x4(x: [f32; 4]) -> Self {
        Self::from_le_bytes([x[0].to_bits().to_le_bytes()[0], x[0].to_bits().to_le_bytes()[1], x[0].to_bits().to_le_bytes()[2], x[0].to_bits().to_le_bytes()[3], x[1].to_bits().to_le_bytes()[0], x[1].to_bits().to_le_bytes()[1], x[1].to_bits().to_le_bytes()[2], x[1].to_bits().to_le_bytes()[3], x[2].to_bits().to_le_bytes()[0], x[2].to_bits().to_le_bytes()[1], x[2].to_bits().to_le_bytes()[2], x[2].to_bits().to_le_bytes()[3], x[3].to_bits().to_le_bytes()[0], x[3].to_bits().to_le_bytes()[1], x[3].to_bits().to_le_bytes()[2], x[3].to_bits().to_le_bytes()[3]])
    }

    #[rustfmt::skip]
    pub const fn as_i64x2(self) -> [i64; 2] {
        let b = self.to_le_bytes();
        [i64::from_le_bytes([b[0], b[1], b[2], b[3], b[4], b[5], b[6], b[7]]), i64::from_le_bytes([b[8], b[9], b[10], b[11], b[12], b[13], b[14], b[15]])]
    }

    #[rustfmt::skip]
    pub const fn as_u64x2(self) -> [u64; 2] {
        let b = self.to_le_bytes();
        [u64::from_le_bytes([b[0], b[1], b[2], b[3], b[4], b[5], b[6], b[7]]), u64::from_le_bytes([b[8], b[9], b[10], b[11], b[12], b[13], b[14], b[15]])]
    }

    #[rustfmt::skip]
    pub const fn as_f64x2(self) -> [f64; 2] {
        let b = self.to_le_bytes();
        [f64::from_bits(u64::from_le_bytes([b[0], b[1], b[2], b[3], b[4], b[5], b[6], b[7]])), f64::from_bits(u64::from_le_bytes([b[8], b[9], b[10], b[11], b[12], b[13], b[14], b[15]]))]
    }

    #[rustfmt::skip]
    pub const fn from_i64x2(x: [i64; 2]) -> Self {
        Self::from_le_bytes([x[0].to_le_bytes()[0], x[0].to_le_bytes()[1], x[0].to_le_bytes()[2], x[0].to_le_bytes()[3], x[0].to_le_bytes()[4], x[0].to_le_bytes()[5], x[0].to_le_bytes()[6], x[0].to_le_bytes()[7], x[1].to_le_bytes()[0], x[1].to_le_bytes()[1], x[1].to_le_bytes()[2], x[1].to_le_bytes()[3], x[1].to_le_bytes()[4], x[1].to_le_bytes()[5], x[1].to_le_bytes()[6], x[1].to_le_bytes()[7]])
    }

    #[rustfmt::skip]
    pub const fn from_u64x2(x: [u64; 2]) -> Self {
        Self::from_le_bytes([x[0].to_le_bytes()[0], x[0].to_le_bytes()[1], x[0].to_le_bytes()[2], x[0].to_le_bytes()[3], x[0].to_le_bytes()[4], x[0].to_le_bytes()[5], x[0].to_le_bytes()[6], x[0].to_le_bytes()[7], x[1].to_le_bytes()[0], x[1].to_le_bytes()[1], x[1].to_le_bytes()[2], x[1].to_le_bytes()[3], x[1].to_le_bytes()[4], x[1].to_le_bytes()[5], x[1].to_le_bytes()[6], x[1].to_le_bytes()[7]])
    }

    #[rustfmt::skip]
    pub const fn from_f64x2(x: [f64; 2]) -> Self {
        Self::from_le_bytes([x[0].to_bits().to_le_bytes()[0], x[0].to_bits().to_le_bytes()[1], x[0].to_bits().to_le_bytes()[2], x[0].to_bits().to_le_bytes()[3], x[0].to_bits().to_le_bytes()[4], x[0].to_bits().to_le_bytes()[5], x[0].to_bits().to_le_bytes()[6], x[0].to_bits().to_le_bytes()[7], x[1].to_bits().to_le_bytes()[0], x[1].to_bits().to_le_bytes()[1], x[1].to_bits().to_le_bytes()[2], x[1].to_bits().to_le_bytes()[3], x[1].to_bits().to_le_bytes()[4], x[1].to_bits().to_le_bytes()[5], x[1].to_bits().to_le_bytes()[6], x[1].to_bits().to_le_bytes()[7]])
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

    pub const fn splat_f32(src: f32) -> Self {
        Self::splat_i32(src.to_bits() as i32)
    }

    pub const fn splat_f64(src: f64) -> Self {
        Self::splat_i64(src.to_bits() as i64)
    }

    pub const fn extract_lane_i8(self, lane: u8) -> i8 {
        debug_assert!(lane < 16);
        let lane = lane as usize;
        let bytes = self.to_le_bytes();
        bytes[lane] as i8
    }

    pub const fn extract_lane_u8(self, lane: u8) -> u8 {
        debug_assert!(lane < 16);
        let lane = lane as usize;
        let bytes = self.to_le_bytes();
        bytes[lane]
    }

    pub const fn extract_lane_i16(self, lane: u8) -> i16 {
        debug_assert!(lane < 8);
        let lane = lane as usize;
        let bytes = self.to_le_bytes();
        let start = lane * 2;
        i16::from_le_bytes([bytes[start], bytes[start + 1]])
    }

    pub const fn extract_lane_u16(self, lane: u8) -> u16 {
        debug_assert!(lane < 8);
        let lane = lane as usize;
        let bytes = self.to_le_bytes();
        let start = lane * 2;
        u16::from_le_bytes([bytes[start], bytes[start + 1]])
    }

    pub const fn extract_lane_i32(self, lane: u8) -> i32 {
        debug_assert!(lane < 4);
        let lane = lane as usize;
        let bytes = self.to_le_bytes();
        let start = lane * 4;
        i32::from_le_bytes([bytes[start], bytes[start + 1], bytes[start + 2], bytes[start + 3]])
    }

    pub const fn extract_lane_i64(self, lane: u8) -> i64 {
        debug_assert!(lane < 2);
        let lane = lane as usize;
        let bytes = self.to_le_bytes();
        let start = lane * 8;
        i64::from_le_bytes([
            bytes[start],
            bytes[start + 1],
            bytes[start + 2],
            bytes[start + 3],
            bytes[start + 4],
            bytes[start + 5],
            bytes[start + 6],
            bytes[start + 7],
        ])
    }

    pub const fn extract_lane_f32(self, lane: u8) -> f32 {
        f32::from_bits(self.extract_lane_i32(lane) as u32)
    }

    pub const fn extract_lane_f64(self, lane: u8) -> f64 {
        f64::from_bits(self.extract_lane_i64(lane) as u64)
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
