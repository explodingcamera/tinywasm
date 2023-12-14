/// Add two values from the stack
macro_rules! add_instr {
    ($ty:ty, $stack:ident) => {{
        let [a, b] = $stack.values.pop_n_const::<2>()?;
        let a: $ty = a.into();
        let b: $ty = b.into();
        $stack.values.push((a + b).into());
    }};
}

/// Subtract the top two values on the stack
macro_rules! sub_instr {
    ($ty:ty, $stack:ident) => {{
        let [a, b] = $stack.values.pop_n_const::<2>()?;
        let a: $ty = a.into();
        let b: $ty = b.into();
        $stack.values.push((a - b).into());
    }};
}

/// Divide the top two values on the stack
macro_rules! checked_divs_instr {
    ($ty:ty, $stack:ident) => {{
        let [a, b] = $stack.values.pop_n_const::<2>()?;
        let a: $ty = a.into();
        let b: $ty = b.into();
        let Some(res) = a.checked_div(b) else {
            return Err(Error::Trap(crate::Trap::DivisionByZero));
        };

        $stack.values.push(res.into());
    }};
}

/// Divide the top two values on the stack
macro_rules! checked_divu_instr {
    ($ty:ty, $uty:ty, $stack:ident) => {{
        let [a, b] = $stack.values.pop_n_const::<2>()?;
        let a: $ty = a.into();
        let b: $ty = b.into();
        let Some(res) = (a as $uty).checked_div(b as $uty) else {
            return Err(Error::Trap(crate::Trap::DivisionByZero));
        };

        $stack.values.push((res as $ty).into());
    }};
}

/// Divide the top two values on the stack
macro_rules! div_instr {
    ($ty:ty, $stack:ident) => {{
        let [a, b] = $stack.values.pop_n_const::<2>()?;
        let a: $ty = a.into();
        let b: $ty = b.into();
        $stack.values.push((a / b).into());
    }};
}

/// Less than signed instruction
macro_rules! lts_instr {
    ($ty:ty, $stack:ident) => {{
        let [a, b] = $stack.values.pop_n_const::<2>()?;
        let a: $ty = a.into();
        let b: $ty = b.into();
        $stack.values.push(((a < b) as i32).into());
    }};
}

/// Less than unsigned instruction
macro_rules! ltu_instr {
    ($ty:ty, $uty:ty, $stack:ident) => {{
        let [a, b] = $stack.values.pop_n_const::<2>()?;
        let a: $ty = a.into();
        let b: $ty = b.into();
        // Cast to unsigned type before comparison
        let a_unsigned: $uty = a as $uty;
        let b_unsigned: $uty = b as $uty;
        $stack.values.push(((a_unsigned < b_unsigned) as i32).into());
    }};
}

/// Multiply the top two values on the stack
macro_rules! mul_instr {
    ($ty:ty, $stack:ident) => {{
        let [a, b] = $stack.values.pop_n_const::<2>()?;
        let a: $ty = a.into();
        let b: $ty = b.into();
        $stack.values.push((a * b).into());
    }};
}

/// Compare the top two values on the stack for equality
macro_rules! eq_instr {
    ($ty:ty, $stack:ident) => {{
        let [a, b] = $stack.values.pop_n_const::<2>()?;
        let a: $ty = a.into();
        let b: $ty = b.into();
        $stack.values.push(((a == b) as i32).into());
    }};
}

/// Compare the top value on the stack for equality with zero
macro_rules! eqz_instr {
    ($ty:ty, $stack:ident) => {{
        let a: $ty = $stack.values.pop()?.into();
        $stack.values.push(((a == 0) as i32).into());
    }};
}

/// Compare the top two values on the stack for inequality
macro_rules! ne_instr {
    ($ty:ty, $stack:ident) => {{
        let [a, b] = $stack.values.pop_n_const::<2>()?;
        let a: $ty = a.into();
        let b: $ty = b.into();
        $stack.values.push(((a != b) as i32).into());
    }};
}

/// Greater or equal than signed instruction
macro_rules! ges_instr {
    ($ty:ty, $stack:ident) => {{
        let [a, b] = $stack.values.pop_n_const::<2>()?;
        let a: $ty = a.into();
        let b: $ty = b.into();
        $stack.values.push(((a >= b) as i32).into());
    }};
}

/// Greater or equal than unsigned instruction
macro_rules! geu_instr {
    ($ty:ty, $uty:ty, $stack:ident) => {{
        let [a, b] = $stack.values.pop_n_const::<2>()?;
        let a: $ty = a.into();
        let b: $ty = b.into();
        // Cast to unsigned type before comparison
        let a_unsigned: $uty = a as $uty;
        let b_unsigned: $uty = b as $uty;
        $stack.values.push(((a_unsigned >= b_unsigned) as i32).into());
    }};
}

pub(super) use add_instr;
pub(super) use checked_divs_instr;
pub(super) use checked_divu_instr;
pub(super) use div_instr;
pub(super) use eq_instr;
pub(super) use eqz_instr;
pub(super) use ges_instr;
pub(super) use geu_instr;
pub(super) use lts_instr;
pub(super) use ltu_instr;
pub(super) use mul_instr;
pub(super) use ne_instr;
pub(super) use sub_instr;
