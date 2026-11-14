(module
  (func (export "run") (param $iterations i32) (result i32) (local $sum i32)
    (loop $loop
      local.get $iterations
      ref.i31
      ref.as_non_null
      i31.get_s
      ref.i31
      i31.get_u
      ref.i31
      ref.is_null
      local.get $sum
      i32.add
      local.set $sum
      local.get $iterations
      i32.const 1
      i32.sub
      local.tee $iterations
      br_if $loop)
    local.get $sum))
