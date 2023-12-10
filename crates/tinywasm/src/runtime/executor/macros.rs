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

pub(super) use add_instr;
pub(super) use div_instr;
pub(super) use eq_instr;
pub(super) use lts_instr;
pub(super) use mul_instr;
pub(super) use sub_instr;
