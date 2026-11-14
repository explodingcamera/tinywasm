(module
  (memory 1)
  (memory $wide i64 1)
  (func (export "memory32") (param i32 v128) (result i32 i64 v128)
    i32.const 19 i64.const 23
    local.get 0 local.get 1 v128.not v128.store
    local.get 0 v128.load
  )
  (func (export "memory64") (param i64 v128) (result i32 i64 v128)
    i32.const 19 i64.const 23
    local.get 0 local.get 1 v128.not v128.store $wide
    local.get 0 v128.load $wide
  )
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
  (func (export "scalar-survivors") (param v128 v128) (result i32 i64 v128) (local v128)
    i32.const 19
    i64.const 23
    local.get 0
    local.get 1
    i32x4.add
    local.tee 2
    v128.const i32x4 -1 -1 -1 -1
    v128.xor
    drop
    local.get 2
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
(assert_return
  (invoke "scalar-survivors" (v128.const i32x4 1 2 3 4) (v128.const i32x4 5 6 7 8))
  (i32.const 19)
  (i64.const 23)
  (v128.const i32x4 6 8 10 12)
)
(assert_return
  (invoke "memory32" (i32.const 65520) (v128.const i32x4 0 -1 1 -2))
  (i32.const 19) (i64.const 23) (v128.const i32x4 -1 0 -2 1)
)
(assert_return
  (invoke "memory64" (i64.const 65520) (v128.const i32x4 0 -1 1 -2))
  (i32.const 19) (i64.const 23) (v128.const i32x4 -1 0 -2 1)
)
(assert_trap (invoke "memory32" (i32.const 65521) (v128.const i32x4 0 0 0 0)) "out of bounds memory access")
(assert_trap (invoke "memory64" (i64.const -1) (v128.const i32x4 0 0 0 0)) "out of bounds memory access")
