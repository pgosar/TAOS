# Nuke built-in rules and variables.
override MAKEFLAGS += -rR

override IMAGE_NAME := taos

# QEMU configuration
QEMU := qemu-system-x86_64
QEMU_MACHINE := -M q35
QEMU_MEMORY := -m 2G
QEMU_CPU := -smp 2
QEMU_NET := -netdev user,id=net0 -device virtio-net-pci,netdev=net0
QEMU_AUDIO := -device intel-hda -device hda-duplex
QEMU_GDB := -s -S
QEMU_DEBUG := -d cpu_reset,guest_errors -D qemu.log

# Common QEMU flags groups
QEMU_COMMON := $(QEMU_MACHINE) $(QEMU_MEMORY) $(QEMU_CPU) $(QEMU_NET) $(QEMU_AUDIO)
QEMU_UEFI := $(QEMU_COMMON) -bios ovmf-x86_64/OVMF.fd
QEMU_BIOS := $(QEMU_COMMON)

# Boot media flags
QEMU_BOOT_ISO := -cdrom $(IMAGE_NAME).iso -boot d
QEMU_BOOT_HDD := -hda $(IMAGE_NAME).hdd

# Display flags
QEMU_DISPLAY_GUI :=
QEMU_DISPLAY_TERM := -nographic

# Convenience macro to reliably declare user overridable variables.
define DEFAULT_VAR =
    ifeq ($(origin $1),default)
        override $(1) := $(2)
    endif
    ifeq ($(origin $1),undefined)
        override $(1) := $(2)
    endif
endef

define get_zigflags
$(if $(filter debug,$(MODE)),-Doptimize=Debug,$(if $(filter release,$(MODE)),-Doptimize=ReleaseFast,-Doptimize=ReleaseSafe))
endef

# Default to ReleaseSafe mode
override DEFAULT_MODE := release-safe
$(eval $(call DEFAULT_VAR,MODE,$(DEFAULT_MODE)))

$(eval $(call DEFAULT_VAR,KZIGFLAGS,$(DEFAULT_KZIGFLAGS)))

# Debug targets
.PHONY: run-gui-debug
run-gui-debug: MODE=debug
run-gui-debug: run-gui

.PHONY: run-term-debug
run-term-debug: MODE=debug
run-term-debug: run-term

.PHONY: run-bios-debug
run-bios-debug: MODE=debug
run-bios-debug: run-bios

.PHONY: run-hdd-debug
run-hdd-debug: MODE=debug
run-hdd-debug: run-hdd

.PHONY: gdb-gui
gdb-gui: MODE=debug
gdb-gui: ovmf $(IMAGE_NAME).iso
	$(QEMU) $(QEMU_UEFI) $(QEMU_BOOT_ISO) $(QEMU_DISPLAY_GUI) $(QEMU_GDB) $(QEMU_DEBUG)

.PHONY: gdb-term
gdb-term: MODE=debug
gdb-term: ovmf $(IMAGE_NAME).iso
	$(QEMU) $(QEMU_UEFI) $(QEMU_BOOT_ISO) $(QEMU_DISPLAY_TERM) $(QEMU_GDB) $(QEMU_DEBUG)

.PHONY: gdb-hdd-gui
gdb-hdd-gui: MODE=debug
gdb-hdd-gui: ovmf $(IMAGE_NAME).hdd
	$(QEMU) $(QEMU_UEFI) $(QEMU_BOOT_HDD) $(QEMU_DISPLAY_GUI) $(QEMU_GDB) $(QEMU_DEBUG)

.PHONY: gdb-hdd-term
gdb-hdd-term: MODE=debug
gdb-hdd-term: ovmf $(IMAGE_NAME).hdd
	$(QEMU) $(QEMU_UEFI) $(QEMU_BOOT_HDD) $(QEMU_DISPLAY_TERM) $(QEMU_GDB) $(QEMU_DEBUG)

.PHONY: gdb-bios-gui
gdb-bios-gui: MODE=debug
gdb-bios-gui: $(IMAGE_NAME).iso
	$(QEMU) $(QEMU_BIOS) $(QEMU_BOOT_ISO) $(QEMU_DISPLAY_GUI) $(QEMU_GDB) $(QEMU_DEBUG)

.PHONY: gdb-bios-term
gdb-bios-term: MODE=debug
gdb-bios-term: $(IMAGE_NAME).iso
	$(QEMU) $(QEMU_BIOS) $(QEMU_BOOT_ISO) $(QEMU_DISPLAY_TERM) $(QEMU_GDB) $(QEMU_DEBUG)


.PHONY: gdb-hdd-bios-gui
gdb-hdd-bios-gui: MODE=debug
gdb-hdd-bios-gui: $(IMAGE_NAME).hdd
	$(QEMU) $(QEMU_BIOS) $(QEMU_BOOT_HDD) $(QEMU_DISPLAY_GUI) $(QEMU_GDB) $(QEMU_DEBUG)


.PHONY: gdb-hdd-bios-term
gdb-hdd-bios-term: MODE=debug
gdb-hdd-bios-term: $(IMAGE_NAME).hdd
	$(QEMU) $(QEMU_BIOS) $(QEMU_BOOT_HDD) $(QEMU_DISPLAY_TERM) $(QEMU_GDB) $(QEMU_DEBUG)

.PHONY: run-gui-release
run-gui-release: MODE=release
run-gui-release: run-gui

.PHONY: run-term-release
run-term-release: MODE=release
run-term-release: run-term

.PHONY: run-bios-release
run-bios-release: MODE=release
run-bios-release: run-bios

.PHONY: run-hdd-release
run-hdd-release: MODE=release
run-hdd-release: run-hdd

.PHONY: all
all: $(IMAGE_NAME).iso

.PHONY: all-hdd
all-hdd: $(IMAGE_NAME).hdd

# Terminal-based run targets
.PHONY: run-term
run-term: ovmf $(IMAGE_NAME).iso
	$(QEMU) $(QEMU_UEFI) $(QEMU_BOOT_ISO) $(QEMU_DISPLAY_TERM)

.PHONY: run-hdd-term
run-hdd-term: ovmf $(IMAGE_NAME).hdd
	$(QEMU) $(QEMU_UEFI) $(QEMU_BOOT_HDD) $(QEMU_DISPLAY_TERM)

.PHONY: run-bios-term
run-bios-term: $(IMAGE_NAME).iso
	$(QEMU) $(QEMU_BIOS) $(QEMU_BOOT_ISO) $(QEMU_DISPLAY_TERM)

.PHONY: run-hdd-bios-term
run-hdd-bios-term: $(IMAGE_NAME).hdd
	$(QEMU) $(QEMU_BIOS) $(QEMU_BOOT_HDD) $(QEMU_DISPLAY_TERM)

# Graphical run targets
.PHONY: run-gui
run-gui: ovmf $(IMAGE_NAME).iso
	$(QEMU) $(QEMU_UEFI) $(QEMU_BOOT_ISO) $(QEMU_DISPLAY_GUI)

.PHONY: run-hdd-gui
run-hdd-gui: ovmf $(IMAGE_NAME).hdd
	$(QEMU) $(QEMU_UEFI) $(QEMU_BOOT_HDD) $(QEMU_DISPLAY_GUI)

.PHONY: run-bios-gui
run-bios-gui: $(IMAGE_NAME).iso
	$(QEMU) $(QEMU_BIOS) $(QEMU_BOOT_ISO) $(QEMU_DISPLAY_GUI)

.PHONY: run-hdd-bios-gui
run-hdd-bios-gui: $(IMAGE_NAME).hdd
	$(QEMU) $(QEMU_BIOS) $(QEMU_BOOT_HDD) $(QEMU_DISPLAY_GUI)

# Backward compatibility - make the original 'run' targets point to GUI versions
.PHONY: run
run: run-gui

.PHONY: run-hdd
run-hdd: run-hdd-gui

.PHONY: run-bios
run-bios: run-bios-gui

.PHONY: run-hdd-bios
run-hdd-bios: run-hdd-bios-gui

.PHONY: ovmf
ovmf:
	mkdir -p ovmf-x86_64
	cd ovmf-x86_64 && curl -o OVMF.fd https://retrage.github.io/edk2-nightly/bin/RELEASEX64_OVMF.fd

limine/limine:
	rm -rf limine
	git clone https://github.com/limine-bootloader/limine.git --branch=v8.x-binary --depth=1
	$(MAKE) -C limine

.PHONY: kernel
kernel:
	cd kernel && zig build $(call get_zigflags)

$(IMAGE_NAME).iso: limine/limine kernel
	rm -rf iso_root
	mkdir -p iso_root/boot
	cp -v kernel/zig-out/bin/kernel iso_root/boot/
	mkdir -p iso_root/boot/limine
	cp -v limine.conf iso_root/boot/limine/
	mkdir -p iso_root/EFI/BOOT
	cp -v limine/limine-bios.sys limine/limine-bios-cd.bin limine/limine-uefi-cd.bin iso_root/boot/limine/
	cp -v limine/BOOTX64.EFI iso_root/EFI/BOOT/
	cp -v limine/BOOTIA32.EFI iso_root/EFI/BOOT/
	xorriso -as mkisofs -b boot/limine/limine-bios-cd.bin \
		-no-emul-boot -boot-load-size 4 -boot-info-table \
		--efi-boot boot/limine/limine-uefi-cd.bin \
		-efi-boot-part --efi-boot-image --protective-msdos-label \
		iso_root -o $(IMAGE_NAME).iso
	./limine/limine bios-install $(IMAGE_NAME).iso
	rm -rf iso_root

$(IMAGE_NAME).hdd: limine/limine kernel
	rm -f $(IMAGE_NAME).hdd
	dd if=/dev/zero bs=1M count=0 seek=64 of=$(IMAGE_NAME).hdd
	sgdisk $(IMAGE_NAME).hdd -n 1:2048 -t 1:ef00
	./limine/limine bios-install $(IMAGE_NAME).hdd
	mformat -i $(IMAGE_NAME).hdd@@1M
	mmd -i $(IMAGE_NAME).hdd@@1M ::/EFI ::/EFI/BOOT ::/boot ::/boot/limine
	mcopy -i $(IMAGE_NAME).hdd@@1M kernel/zig-out/bin/kernel ::/boot
	mcopy -i $(IMAGE_NAME).hdd@@1M limine.conf ::/boot/limine
	mcopy -i $(IMAGE_NAME).hdd@@1M limine/limine-bios.sys ::/boot/limine
	mcopy -i $(IMAGE_NAME).hdd@@1M limine/BOOTX64.EFI ::/EFI/BOOT
	mcopy -i $(IMAGE_NAME).hdd@@1M limine/BOOTIA32.EFI ::/EFI/BOOT

.PHONY: clean
clean:
	rm -rf iso_root $(IMAGE_NAME).iso $(IMAGE_NAME).hdd
	rm -rf kernel/.zig-cache kernel/zig-cache kernel/zig-out

.PHONY: distclean
distclean: clean
	rm -rf limine ovmf
