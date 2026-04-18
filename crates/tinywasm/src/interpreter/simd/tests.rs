use super::Value128;

fn ref_swizzle(a: [u8; 16], idx: [u8; 16]) -> [u8; 16] {
    let mut out = [0u8; 16];
    for i in 0..16 {
        let j = idx[i];
        out[i] = if j < 16 { a[(j & 0x0f) as usize] } else { 0 };
    }
    out
}

fn ref_shuffle(a: [u8; 16], b: [u8; 16], idx: [u8; 16]) -> [u8; 16] {
    let mut out = [0u8; 16];
    for i in 0..16 {
        let j = idx[i] & 31;
        out[i] = if j < 16 { a[j as usize] } else { b[(j & 0x0f) as usize] };
    }
    out
}

#[test]
fn swizzle_matches_reference() {
    let a = [0x00, 0x11, 0x22, 0x33, 0x44, 0x55, 0x66, 0x77, 0x88, 0x99, 0xaa, 0xbb, 0xcc, 0xdd, 0xee, 0xff];

    for seed in 0u32..512 {
        let mut s = [0u8; 16];
        let mut x = seed.wrapping_mul(0x9e37_79b9).wrapping_add(0x7f4a_7c15);
        for byte in &mut s {
            x ^= x << 13;
            x ^= x >> 17;
            x ^= x << 5;
            *byte = (x & 0xff) as u8;
        }

        let got = Value128(a).i8x16_swizzle(Value128(s));
        let expected = ref_swizzle(a, s).into();
        assert_eq!(got, expected, "seed={seed}");
    }
}

#[test]
fn shuffle_matches_reference() {
    let a = [0x10, 0x11, 0x12, 0x13, 0x14, 0x15, 0x16, 0x17, 0x18, 0x19, 0x1a, 0x1b, 0x1c, 0x1d, 0x1e, 0x1f];
    let b = [0xa0, 0xa1, 0xa2, 0xa3, 0xa4, 0xa5, 0xa6, 0xa7, 0xa8, 0xa9, 0xaa, 0xab, 0xac, 0xad, 0xae, 0xaf];

    for seed in 0u32..512 {
        let mut idx = [0u8; 16];
        let mut x = seed.wrapping_mul(0x85eb_ca6b).wrapping_add(0xc2b2_ae35);
        for byte in &mut idx {
            x ^= x << 13;
            x ^= x >> 17;
            x ^= x << 5;
            *byte = (x & 0xff) as u8;
        }

        let got = Value128::i8x16_shuffle(Value128(a), Value128(b), Value128(idx));
        let expected = ref_shuffle(a, b, idx).into();
        assert_eq!(got, expected, "seed={seed}");
    }
}
