;; A function exposed before data initialization traps can still drop that data.
(module $host
  (table (export "table") 1 funcref)
  (func (export "run")
    i32.const 0
    call_indirect))
(register "host" $host)
(assert_trap
  (module
    (import "host" "table" (table 1 funcref))
    (memory 0)
    (func $f data.drop 0)
    (elem (i32.const 0) $f)
    (data (i32.const 0) "x"))
  "out of bounds memory access")
(assert_return (invoke $host "run"))
(assert_return (invoke $host "run"))

(module $shared
  (table (export "table") 8 funcref)
  (memory (export "memory") 1)
  (func (export "run") (param i32)
    (call_indirect (local.get 0)))
  (func (export "load") (param i32) (result i32)
    (i32.load8_u (local.get 0))))
(register "shared" $shared)

;; Earlier data writes persist, but the failing and later segments are not dropped.
(assert_trap
  (module
    (import "shared" "table" (table 8 funcref))
    (import "shared" "memory" (memory 1))
    (func $earlier
      (memory.init $earlier (i32.const 10) (i32.const 0) (i32.const 1)))
    (func $remaining
      (memory.init $failed (i32.const 10) (i32.const 0) (i32.const 1))
      (memory.init $later (i32.const 11) (i32.const 0) (i32.const 1))
      (memory.init $passive (i32.const 12) (i32.const 0) (i32.const 1)))
    (func $drop
      data.drop $earlier
      data.drop $failed
      data.drop $later
      data.drop $passive)
    (elem (i32.const 0) $earlier $remaining $drop)
    (data $earlier (i32.const 0) "a")
    (data $failed (i32.const 65535) "bc")
    (data $later (i32.const 1) "d")
    (data $passive "e"))
  "out of bounds memory access")
(assert_return (invoke $shared "load" (i32.const 0)) (i32.const 97))
(assert_return (invoke $shared "load" (i32.const 1)) (i32.const 0))
(assert_return (invoke $shared "load" (i32.const 65535)) (i32.const 0))
(assert_trap (invoke $shared "run" (i32.const 0)) "out of bounds memory access")
(assert_return (invoke $shared "run" (i32.const 1)))
(assert_return (invoke $shared "load" (i32.const 10)) (i32.const 98))
(assert_return (invoke $shared "load" (i32.const 11)) (i32.const 100))
(assert_return (invoke $shared "load" (i32.const 12)) (i32.const 101))
(assert_return (invoke $shared "run" (i32.const 2)))
(assert_return (invoke $shared "run" (i32.const 2)))
(assert_trap (invoke $shared "run" (i32.const 1)) "out of bounds memory access")

;; An element trap stops all later effects, including declarative drops and data writes.
(assert_trap
  (module
    (import "shared" "table" (table 8 funcref))
    (import "shared" "memory" (memory 1))
    (func $earlier
      (table.init $earlier (i32.const 6) (i32.const 0) (i32.const 1)))
    (func $remaining
      (table.init $failed (i32.const 6) (i32.const 0) (i32.const 1))
      (table.init $later (i32.const 6) (i32.const 0) (i32.const 1))
      (table.init $passive (i32.const 6) (i32.const 0) (i32.const 1))
      (table.init $declared (i32.const 6) (i32.const 0) (i32.const 1))
      (memory.init $active (i32.const 20) (i32.const 0) (i32.const 1))
      (memory.init $data (i32.const 21) (i32.const 0) (i32.const 1)))
    (func $drop
      elem.drop $earlier
      elem.drop $failed
      elem.drop $later
      elem.drop $passive
      elem.drop $declared
      data.drop $active
      data.drop $data)
    (func $marker
      (i32.store8 (i32.const 22) (i32.const 42)))
    (elem $earlier (i32.const 0) $earlier $remaining $drop)
    (elem $failed (i32.const 7) $marker $marker)
    (elem $later (i32.const 7) $marker)
    (elem $passive func $marker)
    (elem $declared declare func $marker)
    (data $active (i32.const 0) "x")
    (data $data "y"))
  "out of bounds table access")
(assert_return (invoke $shared "load" (i32.const 0)) (i32.const 97))
(assert_trap (invoke $shared "run" (i32.const 7)) "uninitialized element")
(assert_trap (invoke $shared "run" (i32.const 0)) "out of bounds table access")
(assert_return (invoke $shared "run" (i32.const 1)))
(assert_return (invoke $shared "load" (i32.const 20)) (i32.const 120))
(assert_return (invoke $shared "load" (i32.const 21)) (i32.const 121))
(assert_return (invoke $shared "run" (i32.const 6)))
(assert_return (invoke $shared "load" (i32.const 22)) (i32.const 42))
(assert_return (invoke $shared "run" (i32.const 2)))
(assert_return (invoke $shared "run" (i32.const 2)))
(assert_trap (invoke $shared "run" (i32.const 1)) "out of bounds table access")
