.globl reload_segments
.type reload_segments, @function
.globl load_tss
.type load_tss, @function
.section .text
reload_segments:
    mov $0x10, %ax          # Puts 0x10 for DS, ES, FS, GS, SS
    mov %ax, %ds            
    mov %ax, %es            
    mov %ax, %fs            
    mov %ax, %gs            
    mov %ax, %ss            
    pushq $0x08             # Puts 0x08 for CS
    pushq $reload_CS
    lretq                    # I'm not sure if this far jump is resistant to position independent code
reload_CS:
    ret                     
load_tss:
    mov $0x28, %ax
    ltr %ax
    ret