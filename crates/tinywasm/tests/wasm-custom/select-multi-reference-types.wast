(module
  (table 1 funcref)
  (func $f)
  (elem (i32.const 0) func $f)

  (func (export "select-funcref") (param i32) (result i32)
    (ref.null func)
    (i32.const 0)
    (table.get 0)
    (local.get 0)
    (select (result funcref))
    (ref.is_null)
  )
)

(assert_return (invoke "select-funcref" (i32.const 1)) (i32.const 1))
(assert_return (invoke "select-funcref" (i32.const 0)) (i32.const 0))
