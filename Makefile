.PHONY: check
check:
	@cd kernel && \
	echo "Checking clippy" && \
	cargo clippy -- -D warnings && \
	echo "Checking formatting" && \
	cargo fmt --check

.PHONY: build
build:
	@cd kernel && cargo build --features "strict"

.PHONY: run
run:
	@cd kernel && cargo run

.PHONY: debug-sd 
debug-sd: ovmf/ovmf-code-x86_64.fd ovmf/ovmf-vars-x86_64.fd $(IMAGE_NAME).iso blank_drive
	$(QEMU) $(QEMU_UEFI) $(QEMU_BOOT_ISO) $(QEMU_DISPLAY_TERM) --trace "sd*"

.PHONY: run-term
run-term:
	@cd kernel && cargo run mode terminal

.PHONY: gdb-term
gdb-term:
	@cd kernel && cargo run mode gdb-terminal

.PHONY: gdb-gui
gdb-gui:
	@cd kernel && cargo run mode gdb-gui

.PHONY: test
test:
	@cd kernel && cargo test

.PHONY: fmt
fmt:
	@cd kernel && cargo fmt

.PHONY: blank_drive
blank_drive:
	dd if=/dev/zero of=$(STORAGE_NAME).img bs=1M count=4k

.PHONY: clean
clean:
	@cd kernel && cargo clean
