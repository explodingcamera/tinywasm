(module
  (func $loop_test (export "loop_test") (result i32)
    (local i32) ;; Local 0: Counter

    ;; Initialize the counter
    (local.set 0 (i32.const 0))

    ;; Loop starts here
    (loop $my_loop
      ;; Increment the counter
      (local.set 0 (i32.add (local.get 0) (i32.const 1)))

      ;; Exit condition: break out of the loop if counter >= 10
      (br_if $my_loop (i32.lt_s (local.get 0) (i32.const 10)))
    )
    
    ;; Return the counter value
    (local.get 0)
  )

  (func $loop_test3 (export "loop_test3") (result i32)
    (local i32) ;; Local 0: Counter

    ;; Initialize the counter
    (local.set 0 (i32.const 0))

    ;; Loop starts here
    (block $exit_loop  ;; Label for exiting the loop
      (loop $my_loop
        ;; Increment the counter
        (local.set 0 (i32.add (local.get 0) (i32.const 1)))

        ;; Prepare an index for br_table
        ;; Here, we use the counter, but you could modify this
        ;; For simplicity, 0 will continue the loop, any other value will exit
        (local.get 0)
        (i32.const 10)
        (i32.lt_s)
        (br_table $my_loop $exit_loop)
      )
    )
    
    ;; Return the counter value
    (local.get 0)
  )

  (func $calculate (export "loop_test2") (result i32)
    (local i32) ;; Local 0: Counter for the outer loop
    (local i32) ;; Local 1: Counter for the inner loop
    (local i32) ;; Local 2: Result variable

    ;; Initialize variables
    (local.set 0 (i32.const 0)) ;; Initialize outer loop counter
    (local.set 1 (i32.const 0)) ;; Initialize inner loop counter
    (local.set 2 (i32.const 0)) ;; Initialize result variable

    (block $outer  ;; Outer loop label
      (loop $outer_loop
        (local.set 1 (i32.const 5)) ;; Reset inner loop counter for each iteration of the outer loop

        (block $inner  ;; Inner loop label
          (loop $inner_loop
            (br_if $inner (i32.eqz (local.get 1))) ;; Break to $inner if inner loop counter is zero

            ;; Computation: Adding product of counters to the result
            (local.set 2 (i32.add (local.get 2) (i32.mul (local.get 0) (local.get 1))))

            ;; Decrement inner loop counter
            (local.set 1 (i32.sub (local.get 1) (i32.const 1)))
          )
        )

        ;; Increment outer loop counter
        (local.set 0 (i32.add (local.get 0) (i32.const 1)))

        ;; Break condition for outer loop: break if outer loop counter >= 5
        (br_if $outer (i32.ge_s (local.get 0) (i32.const 5))) 
      )
    )

    ;; Return the result
    (local.get 2)
  )
)