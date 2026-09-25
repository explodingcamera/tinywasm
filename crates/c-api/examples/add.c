#include <assert.h>
#include <stdio.h>
#include "tinywasm.h"

int main(void) {
  /* (module (func (export "add") (param i32 i32) (result i32)
   *   local.get 0 local.get 1 i32.add)) */
  const unsigned char binary[] = {
    0,97,115,109,1,0,0,0,1,7,1,96,2,127,127,1,127,
    3,2,1,0,7,7,1,3,97,100,100,0,0,10,9,1,7,0,32,0,32,1,106,11
  };
  wasm_engine_t* engine = wasm_engine_new();
  wasm_store_t* store = wasm_store_new(engine);
  wasm_byte_vec_t bytes;
  wasm_byte_vec_new(&bytes, sizeof(binary), (const char*)binary);
  wasm_module_t* module = wasm_module_new(store, &bytes);
  wasm_byte_vec_delete(&bytes);
  if (!module) {
    wasm_message_t error;
    tinywasm_last_error_message(&error);
    fprintf(stderr, "%s\n", error.data);
    wasm_byte_vec_delete(&error);
    wasm_store_delete(store);
    wasm_engine_delete(engine);
    return 1;
  }

  wasm_extern_vec_t imports = WASM_EMPTY_VEC;
  wasm_trap_t* trap = NULL;
  wasm_instance_t* instance = wasm_instance_new(store, module, &imports, &trap);
  assert(instance && !trap);
  wasm_extern_vec_t exports;
  wasm_instance_exports(instance, &exports);
  /* The export vector owns its handles. wasm_extern_as_func borrows one. */
  wasm_val_t arguments[] = { WASM_I32_VAL(20), WASM_I32_VAL(22) };
  wasm_val_t result[1];
  /* Stack-backed vectors need no vector delete. The call writes the result slot. */
  wasm_val_vec_t args = WASM_ARRAY_VEC(arguments);
  wasm_val_vec_t results = WASM_ARRAY_VEC(result);
  trap = wasm_func_call(wasm_extern_as_func(exports.data[0]), &args, &results);
  assert(!trap);
  printf("20 + 22 = %d\n", result[0].of.i32);
  assert(result[0].of.i32 == 42);

  wasm_extern_vec_delete(&exports);
  wasm_instance_delete(instance);
  wasm_module_delete(module);
  wasm_store_delete(store);
  wasm_engine_delete(engine);
  return 0;
}
