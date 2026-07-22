(module
  ;; Reduced from miniz_oxide's Huffman-table builder, which reached a bounds
  ;; panic during compression. Branches to empty targets skipped DropKeep, so
  ;; a stale operand remained on the stack and corrupted a later loop.
  (func (export "branch-drops-retained") (result i32)
    (local $iteration i32)
    (local $count i32)
    (local $result i32)
    block $done
      loop $outer
        block $scan-done
          i32.const 7
          i32.const 20
          local.get $iteration
          select
          local.get $iteration
          i32.eqz
          if
            br $scan-done
          end
          i32.const 2
          local.set $count
          loop $inner
            local.get $count
            i32.const 1
            i32.sub
            local.tee $count
            br_if $inner
          end
          local.set $result
          br $done
        end
        local.get $iteration
        i32.const 1
        i32.add
        local.set $iteration
        br $outer
      end
    end
    local.get $result)
)

(assert_return (invoke "branch-drops-retained") (i32.const 7))
