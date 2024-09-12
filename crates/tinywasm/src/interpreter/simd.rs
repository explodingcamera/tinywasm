// WIP

struct V128([u8; 16]);

impl V128 {
    fn f32x4(&self) -> [f32; 4] {
        let mut res = [0.0; 4];
        for i in 0..4 {
            let mut f = [0; 4];
            for j in 0..4 {
                f[j] = self.0[i * 4 + j];
            }
            res[i] = f32::from_le_bytes(f);
        }
        res
    }

    fn i32x4(&self) -> [i32; 4] {
        let mut res = [0; 4];
        for i in 0..4 {
            let mut f = [0; 4];
            for j in 0..4 {
                f[j] = self.0[i * 4 + j];
            }
            res[i] = i32::from_le_bytes(f);
        }
        res
    }

    fn i64x2(&self) -> [i64; 2] {
        let mut res = [0; 2];
        for i in 0..2 {
            let mut f = [0; 8];
            for j in 0..8 {
                f[j] = self.0[i * 8 + j];
            }
            res[i] = i64::from_le_bytes(f);
        }
        res
    }

    fn f64x2(&self) -> [f64; 2] {
        let mut res = [0.0; 2];
        for i in 0..2 {
            let mut f = [0; 8];
            for j in 0..8 {
                f[j] = self.0[i * 8 + j];
            }
            res[i] = f64::from_le_bytes(f);
        }
        res
    }

    fn i16x8(&self) -> [i16; 8] {
        let mut res = [0; 8];
        for i in 0..8 {
            let mut f = [0; 2];
            for j in 0..2 {
                f[j] = self.0[i * 2 + j];
            }
            res[i] = i16::from_le_bytes(f);
        }
        res
    }

    fn i8x16(&self) -> [i8; 16] {
        let mut res = [0; 16];
        for i in 0..16 {
            res[i] = i8::from_le_bytes([self.0[i]]);
        }
        res
    }
}

fn vvunop(c1: V128) -> V128 {
    let mut res = [0; 16];
    for i in 0..16 {
        res[i] = !c1.0[i];
    }
    V128(res)
}

fn vvbinop(c1: V128, c2: V128) -> V128 {
    let mut res = [0; 16];
    for i in 0..16 {
        res[i] = c1.0[i] & c2.0[i];
    }
    V128(res)
}

fn vvternop(c1: V128, c2: V128, c3: V128) -> V128 {
    let mut res = [0; 16];
    for i in 0..16 {
        res[i] = c1.0[i] & c2.0[i] | !c1.0[i] & c3.0[i];
    }
    V128(res)
}

fn any_true(val: V128) -> bool {
    val.0.iter().any(|&x| x != 0)
}

fn i8x16_swizzle(c1: V128, c2: V128) -> V128 {
    let mut res = [0; 16];
    for i in 0..16 {
        res[i] = c1.0[c2.0[i] as usize];
    }
    V128(res)
}

fn i18x16_shuffle(c1: V128, c2: V128) -> V128 {
    let mut res = [0; 16];
    for i in 0..16 {
        res[i] = c1.0[(c2.0[i] & 0xf) as usize];
    }
    V128(res)
}

fn f32x4_abs(val: V128) -> V128 {
    let mut res = [0; 16];
    for i in 0..4 {
        let f = val.f32x4();
        let f = f32::abs(f[i]);
        let f = f.to_le_bytes();
        for j in 0..4 {
            res[i * 4 + j] = f[j];
        }
    }
    V128(res)
}
