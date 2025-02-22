section .text
    global _start

_start:
  mov rdi, 1
  mov rsi, 2
  mov rdx, 3
  mov r10, 4
  mov r9, 5
  mov r8, 6
  mov rax, 1 ; EXIT
  syscall

_loop:
  jmp _loop
