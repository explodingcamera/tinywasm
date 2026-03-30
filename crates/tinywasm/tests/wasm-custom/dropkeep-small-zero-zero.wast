(module
  (func $leak (result i32)
    (block
      (i64.const 7)
      (i32.const 1)
      (br_if 0)
      (unreachable)
    )
    (i32.const 0)
  )

  (func (export "run") (param i32) (result i32)
    (local i32)
    (local.get 0)
    (local.set 1)

    (block
      (loop
        (local.get 1)
        (i32.eqz)
        (br_if 1)

        (call $leak)
        (drop)

        (local.get 1)
        (i32.const 1)
        (i32.sub)
        (local.set 1)
        (br 0)
      )
    )

    (i32.const 0)
  )
)

(assert_return (invoke "run" (i32.const 40000)) (i32.const 0))
