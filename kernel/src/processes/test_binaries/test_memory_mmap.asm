section .text
    global _start

_start:
    mov rax, 1 ; EXIT
    int 0x80 ; do exit
