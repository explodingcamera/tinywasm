#![no_main]
#![allow(non_snake_case)]

#[no_mangle]
pub extern "C" fn fibonacci(n: i32) -> i32 {
    let mut sum = 0;
    let mut last = 0;
    let mut curr = 1;
    for _i in 1..n {
        sum = last + curr;
        last = curr;
        curr = sum;
    }
    sum
}

#[no_mangle]
pub extern "C" fn fibonacci_recursive(n: i32) -> i32 {
    if n <= 1 {
        return n;
    }
    fibonacci_recursive(n - 1) + fibonacci_recursive(n - 2)
}
