use tinywasm::{ModuleInstance, Store};

/// A `local.get` right before a label and a `local.tee` right after it must
/// not be fused: branches land on the label with the value on the stack and
/// still have to execute the tee.
#[test]
fn branches_landing_on_local_tee_still_tee() -> Result<(), Box<dyn core::error::Error>> {
    let wasm = wat::parse_str(
        r#"
        (module
          (func $forty_two (result i32) i32.const 42)

          ;; block end label: `br` out of the `if` carries the callee result
          (func (export "block_end") (param i32 i32) (result i32)
            block (result i32)
              local.get 0
              i32.eqz
              if
                call $forty_two
                br 1
              end
              local.get 0
            end
            local.tee 1
            drop
            local.get 1)

          ;; if/else join label: the then-arm's implicit jump lands on the tee
          (func (export "if_else_join") (param i32 i32) (result i32)
            local.get 0
            if (result i32)
              call $forty_two
            else
              local.get 0
            end
            local.tee 1
            drop
            local.get 1)

          ;; loop start label: `br 0` re-enters at the tee with the loop param
          (func (export "loop_start") (param i32 i32) (result i32)
            local.get 0
            loop (param i32) (result i32)
              local.tee 1
              i32.const 40
              i32.lt_u
              if (result i32)
                local.get 1
                i32.const 21
                i32.add
                br 1
              else
                local.get 1
              end
            end)
        )
        "#,
    )?;

    let module = tinywasm::parse_bytes(&wasm)?;
    let mut store = Store::default();
    let instance = ModuleInstance::instantiate(&mut store, &module, None)?;

    let block_end = instance.func::<(i32, i32), i32>(&store, "block_end")?;
    assert_eq!(block_end.call(&mut store, (5, 7))?, 5, "fallthrough keeps local 0");
    assert_eq!(block_end.call(&mut store, (0, 7))?, 42, "branch path must tee the callee result");

    let if_else_join = instance.func::<(i32, i32), i32>(&store, "if_else_join")?;
    assert_eq!(if_else_join.call(&mut store, (0, 7))?, 0, "else arm keeps local 0");
    assert_eq!(if_else_join.call(&mut store, (1, 7))?, 42, "then arm must tee the callee result");

    let loop_start = instance.func::<(i32, i32), i32>(&store, "loop_start")?;
    assert_eq!(loop_start.call(&mut store, (40, 7))?, 40, "no iteration keeps the param");
    assert_eq!(loop_start.call(&mut store, (0, 7))?, 42, "re-entering the loop must tee the carried value");

    Ok(())
}
