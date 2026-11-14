(module
  (memory 1)
  (data (i32.const 0) "\07\00\00\00\0b\00\00\00")
  (func (export "run") (param $iterations i32) (result i32) (local $last i32)
    (loop $loop
      i32.const 0 i32.load
      i32.const 4 i32.load
      i32.mul local.set $last
      local.get $iterations i32.const 1 i32.sub
      local.tee $iterations br_if $loop)
    local.get $last))
