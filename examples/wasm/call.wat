(module
  (func (export "check") (param i32) (result i32)
    i64.const 0       ;; Set 0 to the stack
    local.get 0
    i32.const 10
    i32.lt_s          ;; Check if input is less than 10
    if (param i64) (result i32)   ;; If so,
      i32.const 1     ;; Set 1 to the stack
      return          ;; And return immediately
    else              ;; Otherwise,
      i32.const 0     ;; Set 0 to the stack
      return          ;; And return immediately
    end)              ;; End of the if/else block

  (func (export "simple_block") (result i32)
    (block (result i32)
      (i32.const 0)
      (i32.const 1)
      (i32.add)
    )
  )

  (func (export "checkloop") (result i32)
    (block (result i32)
      (i32.const 0)
      (loop (param i32)
        (block (br 2 (i32.const 18)))
        (br 0 (i32.const 20))
      )
      (i32.const 19)
    )
  )


  (func (export "param") (result i32)
    (i32.const 1)
    (loop (param i32) (result i32)
      (i32.const 2)
      (i32.add)
    )
  )
)