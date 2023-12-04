(module
  (func $add (export "add") (param $a i32) (param $b i32) (result i32)
    local.get $a
    local.get $b
    i32.add)

  (func $add_64 (export "add_64") (param $a i64) (param $b i64) (result i64)
    local.get $a
    local.get $b
    i64.add)
)
