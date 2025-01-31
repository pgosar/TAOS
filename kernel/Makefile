# Nuke built-in rules and variables.
MAKEFLAGS += -rR
.SUFFIXES:

# This is the name that our final executable will have.
override OUTPUT := taos

override RUST_TARGET := x86_64-unknown-none

ifeq ($(RUST_PROFILE),)
    override RUST_PROFILE := dev
endif

override RUST_PROFILE_SUBDIR := $(RUST_PROFILE)
ifeq ($(RUST_PROFILE),dev)
    override RUST_PROFILE_SUBDIR := debug
endif

UNAME_S := $(shell uname -s)
ifeq ($(UNAME_S),Linux)
FIND_CMD = find
else
FIND_CMD = gfind
endif

# Default target.
all:
	RUSTFLAGS="-C relocation-model=static" cargo build --target $(RUST_TARGET) --profile $(RUST_PROFILE)
	cp target/$(RUST_TARGET)/$(RUST_PROFILE_SUBDIR)/$$(cd target/$(RUST_TARGET)/$(RUST_PROFILE_SUBDIR) && $(FIND_CMD) -maxdepth 1 -perm -111 -type f) kernel

.PHONY: clean
clean:
	cargo clean
	rm -rf kernel

.PHONY: distclean
distclean: clean