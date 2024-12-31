extern "C" {
    // system calls

    // Create a process, return child’s PID.
    pub fn fork() -> i32;

    // Terminate the current process; status reported to wait(). No return.
    pub fn exit(status: i32) -> !;

    // Wait for a child to exit; exit status in *status; returns child PID.
    pub fn wait(addr: *const u8) -> i32;

    // Create a pipe, put read/write file descriptors in p[0] and p[1].
    pub fn pipe(fdarray: *const i32) -> i32;

    // Write n bytes from buf to file descriptor fd; returns n.
    pub fn write(fd: i32, addr: *const u8, n: i32) -> i32;

    // Read n bytes into buf; returns number read; or 0 if end of file.
    pub fn read(fd: i32, addr: *mut u8, n: i32) -> i32;

    // Release open file fd.
    pub fn close(fd: i32);

    // Terminate process PID. Returns 0, or -1 for error.
    // int kill(int);

    // Load a file and execute it with arguments; only returns if error.
    pub fn exec(path: *const u8, argv: *const *const u8) -> i32;

    // Open a file; flags indicate read/write; returns an fd (file descriptor).
    pub fn open(path: *const u8, omode: u64) -> i32;

    // Create a device file.
    pub fn mknod(path: *const u8, major: u16, minior: u16) -> i32;

    // Remove a file.
    // int unlink(const char*);

    // Place info about an open file into *st.
    // int fstat(int fd, struct stat*);

    // Create another name (file2) for the file file1.
    // int link(const char*, const char*);

    // Create a new directory.
    // int mkdir(const char*);

    // Change the current directory.
    pub fn chdir(path: *const u8) -> i32;

    // Return a new file descriptor referring to the same file as fd.
    pub fn dup(fd: i32) -> i32;

    // Return the current process’s PID.
    // int getpid(void);

    // Grow process’s memory by n zero bytes. Returns start of new memory.
    pub fn sbrk(n: u32) -> *mut u8;

    // Pause for n clock ticks.
    // int sleep(int);

    // int uptime(void);
}
