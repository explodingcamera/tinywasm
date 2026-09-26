;; Several typed catch clauses: only the one matching the tag runs.
(module
  (tag $a) (tag $b) (tag $c)
  (func (export "typed-catch-picks-middle") (result i32)
    (block $ha
      (block $hb
        (block $hc
          (try_table (catch $a $ha) (catch $b $hb) (catch $c $hc)
            (throw $b))
          (unreachable))
        (return (i32.const 3)))
      (return (i32.const 2)))
    (i32.const 1)))
(assert_return (invoke "typed-catch-picks-middle") (i32.const 2))

;; Mixed-width payloads, with values left on the stack below the handler block
;; and above the throw; throw_ref re-raises a caught payload after its try_table.
(module
  (tag $mixed (param i32 i64 f64 v128))
  (tag $pair (param i32 i64))
  (func (export "mixed-payload") (result i32 i64 v128 i32 i64 f64 v128)
    (i32.const 100) (i64.const 200) (v128.const i32x4 1 2 3 4)   ;; under the handler block
    (block $h (result i32 i64 f64 v128)
      (try_table (catch $mixed $h)
        (i32.const -1) (i64.const -1) (v128.const i32x4 -1 -1 -1 -1) ;; above it
        (throw $mixed (i32.const 7) (i64.const 0x1_0000_0008)
                      (f64.const -10.25) (v128.const i64x2 5 6)))
      (unreachable)))
  (func (export "throw_ref-keeps-payload") (result i32 i64)
    (local $x exnref)
    (block $outer (result i32 i64)
      (try_table (catch $pair $outer)
        (block $h (result i32 i64 exnref)
          (try_table (catch_ref $pair $h)
            (throw $pair (i32.const 88) (i64.const -88)))
          (unreachable))
        (local.set $x) (drop) (drop)          ;; consume the handler's copy
        (throw_ref (local.get $x)))            ;; re-raise outside the try_table
      (unreachable))))
(assert_return (invoke "mixed-payload")
  (i32.const 100) (i64.const 200) (v128.const i32x4 1 2 3 4)
  (i32.const 7) (i64.const 0x1_0000_0008) (f64.const -10.25) (v128.const i64x2 5 6))
(assert_return (invoke "throw_ref-keeps-payload") (i32.const 88) (i64.const -88))

;; A try_table with results: the normal exit, and a throw that ends the body.
(module
  (tag $e (param i32))
  (tag $void)
  (func (export "result-typed") (param i32) (result i32 i32)
    (i32.const 5)
    (block $h (result i32)
      (try_table (result i32) (catch $e $h)
        (i32.const 1000)
        (br_if 0 (i32.const 7) (i32.eqz (local.get 0)))  ;; normal exit, junk under
        (throw $e (i32.const 99)))))                     ;; body ends in throw
  (func (export "result-typed-catch_all") (result i32)
    (block $h
      (try_table (result i32) (catch_all $h) (throw $void))
      (return))
    (i32.const 33)))
(assert_return (invoke "result-typed" (i32.const 0)) (i32.const 5) (i32.const 7))
(assert_return (invoke "result-typed" (i32.const 1)) (i32.const 5) (i32.const 99))
(assert_return (invoke "result-typed-catch_all") (i32.const 33))

;; An exception leaving a nested try_table: once the outer handler catches it,
;; the inner handler is out of scope. Repeated in a loop.
(module
  (tag $e1) (tag $e2)
  (func (export "inner-handler-out-of-scope") (result i32)
    (block $outer
      (try_table (catch $e1 $outer)
        (block $inner
          (try_table (catch $e2 $inner) (throw $e1))
          (unreachable))
        (return (i32.const 99))))
    (throw $e2))                        ;; only the inner try_table catches $e2
  (func (export "throw-to-outer-in-loop") (result i32)
    (local $n i32)
    (loop $again
      (block $outer
        (try_table (catch $e1 $outer)
          (block $inner
            (try_table (catch $e2 $inner) (throw $e1))
            (unreachable))
          (return (i32.const -1))))
      (br_if $again (i32.lt_u (local.tee $n (i32.add (local.get $n) (i32.const 1)))
                              (i32.const 100000))))
    (local.get $n)))
(assert_exception (invoke "inner-handler-out-of-scope"))
(assert_return (invoke "throw-to-outer-in-loop") (i32.const 100000))

;; A payload crossing frames through call_indirect and return_call, with a middle
;; frame that catches it and re-raises it with throw_ref.
(module
  (tag $e (param i32 i64))
  (type $v (func))
  (table funcref (elem $thrower))
  (func $thrower (throw $e (i32.const 42) (i64.const -42)))
  (func $via-indirect (call_indirect (type $v) (i32.const 0)))
  (func $via-tail (return_call $via-indirect))
  (func $rethrower
    (block $h (result i32 i64 exnref)
      (try_table (catch_ref $e $h) (call $via-tail))
      (return))
    (throw_ref))
  (func (export "cross-frame-payload") (result i32 i32 i64)
    (i32.const 7)
    (block $h (result i32 i64)
      (try_table (result i32 i64) (catch $e $h)
        (i32.const 1) (i64.const 2)
        (call $rethrower)))))           ;; call is the body's last instruction
(assert_return (invoke "cross-frame-payload") (i32.const 7) (i32.const 42) (i64.const -42))

;; Branches out of a try_table inside loops, the handler being inactive after a
;; br, and a catch whose label is a loop.
(module
  (tag $e)
  (tag $next (param i32))
  (func (export "br-to-loop-from-try_table") (result i32)
    (local $i i32)
    (block $exit
      (loop $body
        (try_table (catch $e $exit)
          (br_if $exit (i32.ge_u (local.tee $i (i32.add (local.get $i) (i32.const 1)))
                                 (i32.const 100000)))
          (br $body))))
    (local.get $i))
  (func (export "br-out-in-enclosing-loop") (result i32)
    (local $i i32)
    (loop $again
      (block $out
        (try_table (catch_all $out) (br $out)))
      (br_if $again (i32.lt_u (local.tee $i (i32.add (local.get $i) (i32.const 1)))
                              (i32.const 100000))))
    (local.get $i))
  (func (export "handler-inactive-after-br")
    (block $h
      (block $out
        (try_table (catch $e $h) (br $out)))
      (throw $e)))
  (func (export "catch-to-loop-label") (result i32)
    (local $n i32)
    (i32.const 0)
    (loop $l (param i32) (result i32)
      (local.set $n)
      (try_table (result i32) (catch $next $l)
        (if (i32.lt_u (local.get $n) (i32.const 1000))
          (then (throw $next (i32.add (local.get $n) (i32.const 1)))))
        (local.get $n)))))
(assert_return (invoke "br-to-loop-from-try_table") (i32.const 100000))
(assert_return (invoke "br-out-in-enclosing-loop") (i32.const 100000))
(assert_exception (invoke "handler-inactive-after-br"))
(assert_return (invoke "catch-to-loop-label") (i32.const 1000))
