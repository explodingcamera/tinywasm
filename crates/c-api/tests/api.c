#include <stdio.h>
#include <stdlib.h>
#include "tinywasm.h"

static wasm_byte_vec_t read_binary(const char* directory, const char* name) {
  char path[4096];
  snprintf(path, sizeof(path), "%s/%s.wasm", directory, name);
  FILE* file = fopen(path, "rb");
  assert(file);
  assert(fseek(file, 0, SEEK_END) == 0);
  long size = ftell(file);
  assert(size >= 0);
  rewind(file);
  wasm_byte_vec_t bytes;
  wasm_byte_vec_new_uninitialized(&bytes, (size_t)size);
  assert(fread(bytes.data, 1, bytes.size, file) == bytes.size);
  fclose(file);
  return bytes;
}

struct callback_env {
  wasm_func_t* nested;
  wasm_memory_t* memory;
  wasm_trap_t* trap;
  unsigned depth;
  unsigned calls;
  unsigned finalized;
};

static void finalize(void* data) { ++*(unsigned*)data; }
static void finalize_env(void* data) { ++((struct callback_env*)data)->finalized; }

static wasm_trap_t* callback_a(void* data, const wasm_val_vec_t* args, wasm_val_vec_t* results) {
  struct callback_env* env = data;
  ++env->calls;
  ++env->depth;
  if (env->depth == 3) {
    results->data[0] = (wasm_val_t)WASM_I32_VAL(args->data[0].of.i32 + 1);
  } else {
    wasm_trap_t* trap = wasm_func_call(env->nested, args, results);
    assert(!trap);
  }
  --env->depth;
  wasm_memory_data(env->memory)[0] += 1;
  assert(wasm_memory_size(env->memory) == 1);
  return NULL;
}

static wasm_trap_t* callback_b(void* data, const wasm_val_vec_t* args, wasm_val_vec_t* results) {
  struct callback_env* env = data;
  if (args->data[0].of.i32 < 0) return wasm_trap_copy(env->trap);
  results->data[0] = (wasm_val_t)WASM_I32_VAL(args->data[0].of.i32 + 2);
  return NULL;
}

int main(int argc, char** argv) {
  assert(argc == 2);
  wasm_engine_t* engine = wasm_engine_new();
  wasm_store_t* store = wasm_store_new(engine);
  wasm_byte_vec_t bytes = read_binary(argv[1], "api");
  assert(wasm_module_validate(store, &bytes));
  wasm_module_t* module = wasm_module_new(store, &bytes);
  assert(module);
  wasm_byte_vec_delete(&bytes);

  wasm_importtype_vec_t import_types;
  wasm_module_imports(module, &import_types);
  assert(import_types.size == 2);
  const wasm_name_t* name = wasm_importtype_name(import_types.data[0]);
  assert(name->size == 4 && memcmp(name->data, "same", 4) == 0);
  wasm_importtype_vec_delete(&import_types);

  struct callback_env env = {0};
  wasm_functype_t* type = wasm_functype_new_1_1(wasm_valtype_new_i32(), wasm_valtype_new_i32());
  wasm_func_t* a = wasm_func_new_with_env(store, type, callback_a, &env, finalize_env);
  wasm_func_t* b = wasm_func_new_with_env(store, type, callback_b, &env, NULL);
  assert(a && b);
  assert(wasm_func_param_arity(a) == 1 && wasm_func_result_arity(a) == 1);
  assert(!wasm_extern_as_memory(wasm_func_as_extern(a)));
  wasm_functype_delete(type);

  wasm_extern_t* bindings[] = { wasm_func_as_extern(a), wasm_func_as_extern(b) };
  wasm_extern_vec_t imports = WASM_ARRAY_VEC(bindings);
  wasm_trap_t* trap = NULL;
  wasm_instance_t* instance = wasm_instance_new(store, module, &imports, &trap);
  assert(instance && !trap);
  wasm_extern_vec_t exports;
  wasm_instance_exports(instance, &exports);
  assert(exports.size == 8);
  env.memory = wasm_extern_as_memory(exports.data[0]);
  wasm_global_t* global = wasm_extern_as_global(exports.data[1]);
  wasm_table_t* table = wasm_extern_as_table(exports.data[2]);
  wasm_func_t* inc = wasm_extern_as_func(exports.data[3]);
  wasm_func_t* run = wasm_extern_as_func(exports.data[4]);
  env.nested = wasm_extern_as_func(exports.data[5]);

  wasm_name_t message;
  wasm_name_new_from_string_nt(&message, "callback failed");
  env.trap = wasm_trap_new(store, &message);
  wasm_name_delete(&message);
  unsigned trap_finalized = 0;
  wasm_trap_set_host_info_with_finalizer(env.trap, &trap_finalized, finalize);

  wasm_val_t arg[] = { WASM_I32_VAL(40) };
  wasm_val_t output[1]; /* Deliberately uninitialized result storage. */
  wasm_val_vec_t args = WASM_ARRAY_VEC(arg);
  wasm_val_vec_t results = WASM_ARRAY_VEC(output);
  trap = wasm_func_call(run, &args, &results);
  assert(!trap && output[0].kind == WASM_I32 && output[0].of.i32 == 83);
  assert(env.calls == 3 && wasm_memory_data(env.memory)[0] == 3);

  arg[0].of.i32 = -1;
  trap = wasm_func_call(wasm_extern_as_func(exports.data[6]), &args, &results);
  assert(trap && wasm_trap_same(trap, env.trap));
  assert(wasm_trap_get_host_info(trap) == &trap_finalized);
  wasm_trap_message(trap, &message);
  assert(strcmp(message.data, "callback failed") == 0);
  wasm_name_delete(&message);
  wasm_trap_delete(trap);
  assert(trap_finalized == 0);
  wasm_trap_delete(env.trap);
  assert(trap_finalized == 1);

  wasm_val_vec_t empty = WASM_EMPTY_VEC;
  trap = wasm_func_call(wasm_extern_as_func(exports.data[7]), &empty, &empty);
  assert(trap);
  wasm_trap_delete(trap);
  arg[0].of.i32 = 41;
  assert(!wasm_func_call(inc, &args, &results) && output[0].of.i32 == 42);

  wasm_global_get(global, &output[0]);
  assert(output[0].of.i32 == 7);
  output[0] = (wasm_val_t)WASM_I32_VAL(12);
  wasm_global_set(global, &output[0]);
  wasm_global_get(global, &output[0]);
  assert(output[0].of.i32 == 12);
  assert(wasm_memory_grow(env.memory, 1));
  assert(wasm_memory_size(env.memory) == 2 && wasm_memory_data_size(env.memory) == 131072);
  assert(wasm_memory_data(env.memory)[0] == 3);
  assert(!wasm_memory_grow(env.memory, 1));

  wasm_ref_t* reference = wasm_table_get(table, 0);
  wasm_func_t* from_table = wasm_ref_as_func(reference);
  assert(from_table && wasm_func_same(from_table, inc));
  assert(!wasm_func_call(from_table, &args, &results) && output[0].of.i32 == 42);
  assert(wasm_table_set(table, 1, reference));
  assert(wasm_table_grow(table, 2, reference));
  assert(!wasm_table_grow(table, 1, reference));
  assert(wasm_table_size(table) == 4);
  wasm_ref_delete(reference);

  unsigned host_finalized = 0;
  wasm_func_set_host_info_with_finalizer(inc, &host_finalized, finalize);
  wasm_extern_vec_t other_exports;
  wasm_instance_exports(instance, &other_exports);
  assert(wasm_func_get_host_info(wasm_extern_as_func(other_exports.data[3])) == &host_finalized);
  wasm_extern_vec_delete(&other_exports);
  assert(host_finalized == 0);

  unsigned foreign_finalized = 0;
  wasm_foreign_t* foreign = wasm_foreign_new(store);
  wasm_foreign_set_host_info_with_finalizer(foreign, &foreign_finalized, finalize);
  wasm_val_t external = WASM_REF_VAL(wasm_foreign_as_ref(foreign));
  wasm_globaltype_t* external_type = wasm_globaltype_new(wasm_valtype_new_externref(), WASM_VAR);
  wasm_global_t* external_global = wasm_global_new(store, external_type, &external);
  wasm_globaltype_delete(external_type);
  assert(external_global);
  wasm_global_get(external_global, &output[0]);
  assert(wasm_ref_same(output[0].of.ref, external.of.ref));
  wasm_val_t copy;
  wasm_val_copy(&copy, &output[0]);
  wasm_val_delete(&output[0]);
  assert(wasm_ref_same(copy.of.ref, external.of.ref));
  wasm_val_delete(&copy);
  wasm_foreign_delete(foreign);
  wasm_global_delete(external_global);

  wasm_store_t* other_store = wasm_store_new(engine);
  wasm_shared_module_t* shared = wasm_module_share(module);
  wasm_module_t* obtained = wasm_module_obtain(other_store, shared);
  assert(obtained);
  wasm_shared_module_delete(shared);
  assert(!wasm_instance_new(other_store, obtained, &imports, &trap));
  assert(trap);
  wasm_trap_delete(trap);
  wasm_module_delete(obtained);
  wasm_store_delete(other_store);

  wasm_byte_vec_t archive;
  wasm_module_serialize(module, &archive);
  wasm_module_t* restored = wasm_module_deserialize(store, &archive);
  assert(restored);
  wasm_module_delete(restored);
  wasm_byte_vec_delete(&archive);

  for (unsigned i = 0; i < 2; ++i) {
    bytes = read_binary(argv[1], i == 0 ? "simd" : "memory64");
    assert(!wasm_module_validate(store, &bytes));
    assert(!wasm_module_new(store, &bytes));
    wasm_byte_vec_delete(&bytes);
  }
  bytes = read_binary(argv[1], "start");
  wasm_module_t* start_module = wasm_module_new(store, &bytes);
  wasm_byte_vec_delete(&bytes);
  wasm_extern_vec_t no_imports = WASM_EMPTY_VEC;
  assert(!wasm_instance_new(store, start_module, &no_imports, &trap));
  assert(trap);
  wasm_trap_delete(trap);
  wasm_module_delete(start_module);

  wasm_extern_vec_delete(&exports);
  wasm_instance_delete(instance);
  wasm_func_delete(a);
  wasm_func_delete(b);
  wasm_module_delete(module);
  wasm_store_delete(store);
  wasm_engine_delete(engine);
  assert(env.finalized == 1 && host_finalized == 1 && foreign_finalized == 1);
  puts("C API integration tests passed");
  return 0;
}
