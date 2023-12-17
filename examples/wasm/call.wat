(module
  (func $check_input (param i32) (result i32)
    local.get 0
    i32.const 10
    i32.lt_s          ;; Check if input is less than 10
    if (result i32)   ;; If so,
      i32.const 1     ;; Set 1 to the stack
      return          ;; And return immediately
    else              ;; Otherwise,
      i32.const 0     ;; Set 0 to the stack
      return          ;; And return immediately
    end)              ;; End of the if/else block

  (export "check" (func $check_input))
)