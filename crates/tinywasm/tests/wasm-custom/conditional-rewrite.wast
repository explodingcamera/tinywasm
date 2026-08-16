(module
  (func (export "retained-value-before-update-branch") (param i32 i32) (result i32)
    (block (result i32)
      local.get 0
      i32.eqz
      local.get 1
      i32.const 1
      i32.add
      local.tee 1
      br_if 0
    )
    drop
    local.get 1
  )

  (func (export "compare-eqz-br-if") (param i32 i32) (result i32)
    block
      i32.const 1
      local.get 0
      local.get 1
      i32.lt_u
      i32.eqz
      br_if 0
      drop
      i32.const 2
      return
    end
    i32.const 3
  )
)

(assert_return (invoke "retained-value-before-update-branch" (i32.const 0) (i32.const 0)) (i32.const 1))
(assert_return (invoke "retained-value-before-update-branch" (i32.const 1) (i32.const -1)) (i32.const 0))
(assert_return (invoke "compare-eqz-br-if" (i32.const 1) (i32.const 2)) (i32.const 2))
(assert_return (invoke "compare-eqz-br-if" (i32.const 2) (i32.const 1)) (i32.const 3))
