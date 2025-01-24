set architecture i386:x86-64

target remote localhost:1234

# Disable paging for long output
set pagination off

add-auto-load-safe-path /home/anooprac/cs378/Multicore-OS/.gdbinit
set auto-load safe-path