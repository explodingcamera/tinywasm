# Optional convenience targets. Cargo remains the library build system.
CARGO ?= cargo
CC ?= cc
CXX ?= c++
PREFIX ?= /usr/local
DESTDIR ?=
LIBDIR ?= $(PREFIX)/lib
PROFILE ?= release
TARGET_DIR ?= $(if $(CARGO_TARGET_DIR),$(CARGO_TARGET_DIR),../../target)
BUILD_LIBDIR := $(abspath $(TARGET_DIR)/$(if $(filter dev,$(PROFILE)),debug,$(PROFILE)))
BUILDDIR := $(abspath $(TARGET_DIR)/c-api)
CARGO_FLAGS := --profile $(PROFILE)
ifneq ($(TINYWASM_C_API_PREFIX),)
export TINYWASM_C_API_PREFIX
CARGO_FLAGS += --features custom-prefix
CPPFLAGS += -DTINYWASM_C_API_PREFIX=$(TINYWASM_C_API_PREFIX)
endif
CFLAGS ?= -O2 -Wall -Wextra -Werror
UNAME := $(shell uname -s)
SHARED_EXT := $(if $(filter Darwin,$(UNAME)),dylib,so)
# Native libraries reported by `cargo rustc -- --print native-static-libs`.
NATIVE_LIBS ?= $(if $(filter Darwin,$(UNAME)),-framework Security -framework CoreFoundation -liconv -lSystem,-lgcc_s -lutil -lrt -lpthread -lm -ldl -lc)
VERSION := $(shell sed -n '/^\[workspace.package\]/,/^\[/s/^version = "\(.*\)"/\1/p' ../../Cargo.toml)

.PHONY: all build example test install
all: build

build:
	$(CARGO) build -p tinywasm-c-api $(CARGO_FLAGS) --target-dir "$(abspath $(TARGET_DIR))"

example: build
	mkdir -p "$(BUILDDIR)"
	$(CC) $(CPPFLAGS) $(CFLAGS) -std=c11 -Iinclude examples/add.c -L"$(BUILD_LIBDIR)" -Wl,-rpath,"$(BUILD_LIBDIR)" -ltinywasm $(LDFLAGS) -o "$(BUILDDIR)/add"
	"$(BUILDDIR)/add"

test: example
	$(CXX) $(CPPFLAGS) -std=c++17 -Wall -Wextra -Werror -fsyntax-only -x c++ -Iinclude include/tinywasm.h
	$(CARGO) run -p tinywasm-c-api --example fixtures --target-dir "$(abspath $(TARGET_DIR))" -- "$(BUILDDIR)"
	$(CC) $(CPPFLAGS) $(CFLAGS) -std=c11 -Iinclude tests/api.c -L"$(BUILD_LIBDIR)" -Wl,-rpath,"$(BUILD_LIBDIR)" -ltinywasm $(LDFLAGS) -o "$(BUILDDIR)/api"
	"$(BUILDDIR)/api" "$(BUILDDIR)"
	$(CC) $(CPPFLAGS) $(CFLAGS) -std=c11 -Iinclude examples/add.c "$(BUILD_LIBDIR)/libtinywasm.a" $(NATIVE_LIBS) $(LDFLAGS) -o "$(BUILDDIR)/add-static"
	"$(BUILDDIR)/add-static"
	$(CC) $(CPPFLAGS) $(CFLAGS) -std=c11 -Iinclude tests/api.c "$(BUILD_LIBDIR)/libtinywasm.a" $(NATIVE_LIBS) $(LDFLAGS) -o "$(BUILDDIR)/api-static"
	"$(BUILDDIR)/api-static" "$(BUILDDIR)"
	python3 tests/symbols.py "$(BUILD_LIBDIR)/libtinywasm.$(SHARED_EXT)" "$(CC)" "$(TINYWASM_C_API_PREFIX)"

install: build
	install -d "$(DESTDIR)$(PREFIX)/include" "$(DESTDIR)$(LIBDIR)/pkgconfig" "$(DESTDIR)$(PREFIX)/share/licenses/tinywasm"
	install -m644 include/wasm.h include/tinywasm.h include/tinywasm-prefix.h "$(DESTDIR)$(PREFIX)/include/"
	install -m644 "$(BUILD_LIBDIR)/libtinywasm.a" "$(BUILD_LIBDIR)/libtinywasm.$(SHARED_EXT)" "$(DESTDIR)$(LIBDIR)/"
	install -m644 include/LICENSE-wasm-c-api ../../LICENSE-APACHE ../../LICENSE-MIT "$(DESTDIR)$(PREFIX)/share/licenses/tinywasm/"
	sed -e 's|@PREFIX@|$(PREFIX)|g' -e 's|@LIBDIR@|$(LIBDIR)|g' -e 's|@VERSION@|$(VERSION)|g' -e 's|@NATIVE_LIBS@|$(NATIVE_LIBS)|g' -e 's|@PREFIX_FLAG@|$(if $(TINYWASM_C_API_PREFIX),-DTINYWASM_C_API_PREFIX=$(TINYWASM_C_API_PREFIX))|g' tinywasm.pc.in > "$(DESTDIR)$(LIBDIR)/pkgconfig/tinywasm.pc"
