(module
  (func (export "i32-shift-8") (param i32) (result i32)
    local.get 0
    i32.const 8
    i32.shl
    i32.const 8
    i32.shr_s
  )
  (func (export "i32-extend-8") (param i32) (result i32)
    local.get 0
    i32.const 24
    i32.shl
    i32.const 24
    i32.shr_s
  )
  (func (export "i32-extend-16") (param i32) (result i32)
    local.get 0
    i32.const 16
    i32.shl
    i32.const 16
    i32.shr_s
  )
  (func (export "i64-shift-16") (param i64) (result i64)
    local.get 0
    i64.const 16
    i64.shl
    i64.const 16
    i64.shr_s
  )
  (func (export "i64-extend-8") (param i64) (result i64)
    local.get 0
    i64.const 56
    i64.shl
    i64.const 56
    i64.shr_s
  )
  (func (export "i64-extend-16") (param i64) (result i64)
    local.get 0
    i64.const 48
    i64.shl
    i64.const 48
    i64.shr_s
  )
  (func (export "i64-extend-32") (param i64) (result i64)
    local.get 0
    i64.const 32
    i64.shl
    i64.const 32
    i64.shr_s
  )
)

(assert_return (invoke "i32-shift-8" (i32.const 0x80)) (i32.const 0x80))
(assert_return (invoke "i32-extend-8" (i32.const 0x80)) (i32.const -128))
(assert_return (invoke "i32-extend-16" (i32.const 0x8000)) (i32.const -32768))
(assert_return (invoke "i64-shift-16" (i64.const 0x80)) (i64.const 0x80))
(assert_return (invoke "i64-extend-8" (i64.const 0x80)) (i64.const -128))
(assert_return (invoke "i64-extend-16" (i64.const 0x8000)) (i64.const -32768))
(assert_return (invoke "i64-extend-32" (i64.const 0x80000000)) (i64.const -2147483648))
