# xv6-rust
This repo belongs to a learning project, which port the [xv6](https://github.com/mit-pdos/xv6-public) operating system from C to Rust.

## How to run?
### Preconditions
Please make sure you have QEMU installed in your local machine, my QEMU version is:
```shell
$ qemu-system-riscv64 --version
QEMU emulator version 9.1.0
Copyright (c) 2003-2024 Fabrice Bellard and the QEMU Project developers
```

Some of the features rely on nightly build of rust, my Cargo version is:
```shell
$ cargo --version
cargo 1.75.0-nightly (b4d18d4bd 2023-10-31)
```

### Run with debugger
```shell
### In terminal 1, build and start running.
### The program will be stopped at the beginning, wait for a debugger to connect.
$ make run-debug
... ...

### In terminal 2, connect the remote gdb server via a gdb client
$ gdb -ex "target remote localhost:1234" {project_root}/kernel/target/riscv64gc-unknown-none-elf/debug/kernel
... ...
### At gdb:
###   execute "c" to let the xv6 continue running
###   execute "CTRL+C" to stop
###   execute "kill" to terminate the xv6
```

## Project Structure
```shell
{project_root}
├── kernel       ## os kernel
├── user         ## user programs, such as init, sh, echo...
├── mkfs         ## make the file system image
└── Makefile     ## root path makefile
```

## For more details
I'm writing a series of articles, please check [here](https://www.lenshood.dev/2024/10/23/xv6-rust-1/) to get more details.