PACKAGE := tinywasm-c-api
LIB_BASENAME := tinywasm_c_api
RUST_TARGET ?= $(shell rustc -vV | sed -n 's/^host: //p')

DIST_DIR := dist/$(RUST_TARGET)
DIST_INCLUDE_DIR := $(DIST_DIR)/include
DIST_LIB_DIR := $(DIST_DIR)/lib
CARGO_BUILD_ARGS := -p $(PACKAGE) --release --target $(RUST_TARGET)
TARGET_DIR := ../../target/$(RUST_TARGET)/release

ifneq (,$(findstring windows,$(RUST_TARGET)))
STATIC_SRC := $(LIB_BASENAME).lib
STATIC_DST := tinywasm.lib
DYNAMIC_SRC := $(LIB_BASENAME).dll
DYNAMIC_DST := tinywasm.dll
else ifneq (,$(findstring apple,$(RUST_TARGET)))
STATIC_SRC := lib$(LIB_BASENAME).a
STATIC_DST := libtinywasm.a
DYNAMIC_SRC := lib$(LIB_BASENAME).dylib
DYNAMIC_DST := libtinywasm.dylib
else
STATIC_SRC := lib$(LIB_BASENAME).a
STATIC_DST := libtinywasm.a
DYNAMIC_SRC := lib$(LIB_BASENAME).so
DYNAMIC_DST := libtinywasm.so
endif

.PHONY: all header static dynamic clean update-header

all: header static dynamic

header:
	@mkdir -p "$(DIST_INCLUDE_DIR)"
	cp include/wasm.h "$(DIST_INCLUDE_DIR)/wasm.h"

static:
	cargo build $(CARGO_BUILD_ARGS)
	@mkdir -p "$(DIST_LIB_DIR)"
	cp "$(TARGET_DIR)/$(STATIC_SRC)" "$(DIST_LIB_DIR)/$(STATIC_DST)"

dynamic:
	cargo build $(CARGO_BUILD_ARGS)
	@mkdir -p "$(DIST_LIB_DIR)"
	cp "$(TARGET_DIR)/$(DYNAMIC_SRC)" "$(DIST_LIB_DIR)/$(DYNAMIC_DST)"

clean:
	rm -rf "$(DIST_DIR)"
