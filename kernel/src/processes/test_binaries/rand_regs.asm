section .text
    global _start

_start:
    mov rax, 0x42
    mov r8, 0xbeef
    mov r9, 0x69
    mov r10, 0x420
    mov r11, 0xdeadbeef
    jmp _start
