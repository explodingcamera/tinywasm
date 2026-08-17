(module
  (func (export "and") (param v128 v128) (result v128)
    local.get 0
    local.get 1
    v128.and
  )
  (func (export "or") (param v128 v128) (result v128)
    local.get 0
    local.get 1
    v128.or
  )
  (func (export "add") (param v128 v128) (result v128)
    local.get 0
    local.get 1
    i64x2.add
  )
  (func (export "mul") (param v128 v128) (result v128)
    local.get 0
    local.get 1
    i64x2.mul
  )
  (func (export "deep-xor") (param v128 v128 v128) (result v128 v128)
    local.get 0
    local.get 1
    local.get 2
    v128.xor
  )
)

(assert_return
  (invoke "and" (v128.const i32x4 -1 0 -1 0) (v128.const i32x4 1 2 3 4))
  (v128.const i32x4 1 0 3 0)
)
(assert_return
  (invoke "or" (v128.const i32x4 1 0 3 0) (v128.const i32x4 0 2 0 4))
  (v128.const i32x4 1 2 3 4)
)
(assert_return
  (invoke "add" (v128.const i64x2 1 2) (v128.const i64x2 3 4))
  (v128.const i64x2 4 6)
)
(assert_return
  (invoke "mul" (v128.const i64x2 2 3) (v128.const i64x2 4 5))
  (v128.const i64x2 8 15)
)
(assert_return
  (invoke "deep-xor"
    (v128.const i32x4 1 2 3 4)
    (v128.const i32x4 8 4 2 1)
    (v128.const i32x4 1 1 1 1))
  (v128.const i32x4 1 2 3 4)
  (v128.const i32x4 9 5 3 0)
)
