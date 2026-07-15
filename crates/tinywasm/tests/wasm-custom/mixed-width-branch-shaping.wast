;; A branch preserves its mixed-width results while discarding lane-specific
;; temporaries above values that predate the block.
(module
  (func (export "mixed") (result i64)
    i64.const 40
    (block (result i64 i32)
      i32.const 99
      i64.const 98
      v128.const i32x4 1 2 3 4
      i64.const 9
      i32.const 7
      br 0)
    drop
    i64.add))

(assert_return (invoke "mixed") (i64.const 49))
