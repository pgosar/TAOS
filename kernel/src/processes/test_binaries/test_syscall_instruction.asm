section .text
    global _start

_start:
    mov rax, 1
    syscall ; exit