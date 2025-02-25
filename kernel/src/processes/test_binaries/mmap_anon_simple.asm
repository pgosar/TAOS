section .text
    global _start

_start:
    ; set up initial mmap call
    mov rbx, 0x0
    mov rcx, 0x1000
    mov rdx, 0x6
    mov rsi, 0x10
    mov rdi, -1
    mov rbp, 0x0
    mov rax, 0x4

    int 0x80

    ; write value to memory addr returned by mmap call
    mov byte [rax], 0x42

    ; read back value to see if we can. if it's what we expect,
    ; return with code 1. Else, return w code 0.
    mov bl, byte [rax] 
    cmp bl, 0x42
    je _finish

    mov rbx, 0x0
    mov rax, 0x1
    int 0x80

_finish: 
    mov rbx, 0x1
    mov rax, 0x1
    int 0x80