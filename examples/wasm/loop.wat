(module
  (func $loop (export "loop") (result i32)
    (local i32)  ;; Declare a local i32 variable, let's call it 'i'
    (i32.const 0)  ;; Initialize 'i' to 0
    (local.set 0)

    (loop $loopStart  ;; Start of the loop
      (local.get 0)  ;; Get the current value of 'i'
      (i32.const 1)  ;; Push 1 onto the stack
      (i32.add)      ;; Add 'i' and 1
      (local.set 0)  ;; Update 'i' with the new value

      (local.get 0)  ;; Push the current value of 'i' to check the condition
      (i32.const 10) ;; Push 10 onto the stack
      (i32.lt_s)     ;; Check if 'i' is less than 10
      (br_if $loopStart)  ;; If 'i' < 10, continue the loop
    )

    (local.get 0)  ;; After the loop, get the value of 'i' to be returned
    ;; The function will return the value of 'i' here
  )
)
