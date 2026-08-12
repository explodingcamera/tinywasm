;; Memory64 bulk operations use the target memory's address width.
(module
  (memory (export "memory") i64 1)
  (data "abc")
  (func (export "run")
    (memory.init 0 (i64.const 4) (i32.const 0) (i32.const 3))
    (memory.copy (i64.const 8) (i64.const 4) (i64.const 3))
    (memory.fill (i64.const 12) (i32.const 120) (i64.const 2)))
  (func (export "load8") (param i64) (result i32)
    (i32.load8_u (local.get 0))))

(invoke "run")
(assert_return (invoke "load8" (i64.const 4)) (i32.const 97))
(assert_return (invoke "load8" (i64.const 6)) (i32.const 99))
(assert_return (invoke "load8" (i64.const 8)) (i32.const 97))
(assert_return (invoke "load8" (i64.const 10)) (i32.const 99))
(assert_return (invoke "load8" (i64.const 12)) (i32.const 120))
(assert_return (invoke "load8" (i64.const 13)) (i32.const 120))

;; Mixed-width memory.copy pops each operand from the correct value lane.
(module
  (memory $src 1)
  (memory $dst i64 1)
  (data (memory $src) (i32.const 0) "abc")
  (func (export "run")
    (memory.copy $dst $src (i64.const 5) (i32.const 0) (i32.const 3)))
  (func (export "load8") (param i64) (result i32)
    (i32.load8_u $dst (local.get 0))))

(invoke "run")
(assert_return (invoke "load8" (i64.const 5)) (i32.const 97))
(assert_return (invoke "load8" (i64.const 7)) (i32.const 99))

;; Unsigned memory.init source bounds produce a Wasm trap rather than a host panic.
(module
  (memory 1)
  (data "x")
  (func (export "run")
    (memory.init 0 (i32.const 0) (i32.const -1) (i32.const 1))))

(assert_trap (invoke "run") "out of bounds memory access")

;; memory.init must mark its target local memory as used by allocation analysis.
(module $host
  (memory (export "memory") 1))
(register "host" $host)

(module
  (import "host" "memory" (memory 1))
  (memory $local 1)
  (data $data "x")
  (func (export "run")
    (memory.init $local $data (i32.const 0) (i32.const 0) (i32.const 1))))

(assert_return (invoke "run"))

;; Store superinstructions select the address lane from the target memory.
(module
  (memory i64 1)
  (func (export "store") (param i64 i32)
    (i32.store (local.get 0) (local.get 1)))
  (func (export "fma") (param i64 f32 f32 f32)
    (f32.store
      (local.get 0)
      (f32.add (local.get 1) (f32.mul (local.get 2) (local.get 3)))))
  (func (export "load_i32") (param i64) (result i32)
    (i32.load (local.get 0)))
  (func (export "load_f32") (param i64) (result f32)
    (f32.load (local.get 0))))

(invoke "store" (i64.const 4) (i32.const 42))
(invoke "fma" (i64.const 8) (f32.const 1) (f32.const 2) (f32.const 3))
(assert_return (invoke "load_i32" (i64.const 4)) (i32.const 42))
(assert_return (invoke "load_f32" (i64.const 8)) (f32.const 7))
