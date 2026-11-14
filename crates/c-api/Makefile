# Optional convenience targets. Cargo remains the library build system.
CARGO ?= cargo
RUSTC ?= rustc
CC ?= cc
CXX ?= c++
PREFIX ?= /usr/local
DESTDIR ?=
LIBDIR ?= $(PREFIX)/lib
PROFILE ?= release
TARGET_DIR ?= $(if $(CARGO_TARGET_DIR),$(CARGO_TARGET_DIR),../../target)
TARGET ?= $(CARGO_BUILD_TARGET)
# BUILD_STD=auto rebuilds std when nightly and rust-src are available.
BUILD_STD ?= 0
NIGHTLY := $(and $(findstring nightly,$(shell $(CARGO) --version)),$(findstring nightly,$(shell $(RUSTC) --version)))
RUST_SRC := $(wildcard $(shell $(RUSTC) --print sysroot)/lib/rustlib/src/rust/library/std/Cargo.toml)
USE_BUILD_STD := $(if $(filter auto,$(BUILD_STD)),$(if $(and $(NIGHTLY),$(RUST_SRC)),1,0),$(BUILD_STD))
BUILD_TARGET := $(if $(TARGET),$(TARGET),$(if $(filter 1,$(USE_BUILD_STD)),$(shell $(RUSTC) -vV | sed -n 's/^host: //p')))
BUILD_LIBDIR := $(abspath $(TARGET_DIR)/$(if $(BUILD_TARGET),$(BUILD_TARGET)/)$(if $(filter dev,$(PROFILE)),debug,$(PROFILE)))
BUILDDIR := $(abspath $(TARGET_DIR)/c-api)
CARGO_FLAGS := --profile $(PROFILE)
ifeq ($(USE_BUILD_STD),1)
CARGO_FLAGS += -Z build-std=std,panic_unwind
endif
ifneq ($(BUILD_TARGET),)
CARGO_FLAGS += --target $(BUILD_TARGET)
endif
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
	$(CC) $(CPPFLAGS) $(CFLAGS) -std=c11 -Iinclude tests/api.c -L"$(BUILD_LIBDIR)" -Wl,-rpath,"$(BUILD_LIBDIR)" -ltinywasm $(LDFLAGS) -o "$(BUILDDIR)/api"
	"$(BUILDDIR)/api" tests/fixtures
	$(CC) $(CPPFLAGS) $(CFLAGS) -std=c11 -Iinclude examples/add.c "$(BUILD_LIBDIR)/libtinywasm.a" $(NATIVE_LIBS) $(LDFLAGS) -o "$(BUILDDIR)/add-static"
	"$(BUILDDIR)/add-static"
	$(CC) $(CPPFLAGS) $(CFLAGS) -std=c11 -Iinclude tests/api.c "$(BUILD_LIBDIR)/libtinywasm.a" $(NATIVE_LIBS) $(LDFLAGS) -o "$(BUILDDIR)/api-static"
	"$(BUILDDIR)/api-static" tests/fixtures

install: build
	install -d "$(DESTDIR)$(PREFIX)/include" "$(DESTDIR)$(LIBDIR)/pkgconfig" "$(DESTDIR)$(PREFIX)/share/licenses/tinywasm"
	install -m644 include/wasm.h include/tinywasm.h include/tinywasm-prefix.h "$(DESTDIR)$(PREFIX)/include/"
	install -m644 "$(BUILD_LIBDIR)/libtinywasm.a" "$(BUILD_LIBDIR)/libtinywasm.$(SHARED_EXT)" "$(DESTDIR)$(LIBDIR)/"
	install -m644 ../../LICENSE-APACHE ../../LICENSE-MIT "$(DESTDIR)$(PREFIX)/share/licenses/tinywasm/"
	sed -e 's|@PREFIX@|$(PREFIX)|g' -e 's|@LIBDIR@|$(LIBDIR)|g' -e 's|@VERSION@|$(VERSION)|g' -e 's|@NATIVE_LIBS@|$(NATIVE_LIBS)|g' -e 's|@PREFIX_FLAG@|$(if $(TINYWASM_C_API_PREFIX),-DTINYWASM_C_API_PREFIX=$(TINYWASM_C_API_PREFIX))|g' tinywasm.pc.in > "$(DESTDIR)$(LIBDIR)/pkgconfig/tinywasm.pc"
