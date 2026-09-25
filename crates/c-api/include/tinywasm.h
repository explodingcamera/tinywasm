#ifndef TINYWASM_H
#define TINYWASM_H

#ifdef TINYWASM_C_API_PREFIX
#include "tinywasm-prefix.h"
#endif
#include "wasm.h"

#ifdef __cplusplus
extern "C" {
#endif

/* Copies this thread's most recent API error into an owned, nul-terminated
 * vector. Successful operations do not clear it. Delete with wasm_byte_vec_delete.
 * An empty diagnostic is returned as a single nul byte. */
WASM_API_EXTERN void tinywasm_last_error_message(wasm_message_t* out);

#ifdef __cplusplus
}
#endif
#endif
