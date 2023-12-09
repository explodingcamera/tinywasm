(module
  (func $add_fn (param $a i32) (param $b i32) (result i32)
    local.get $a
    local.get $b
    i32.add
  )

  (func $add (param $x i32) (param $y i32) (result i32)
    local.get $x
    local.get $y
    call $add_fn
  )

  (export "add" (func $add))
)

