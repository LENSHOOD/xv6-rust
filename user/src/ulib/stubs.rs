extern "C" {
    // system calls
    pub fn fork() -> i32;
    pub fn exit(status: i32) -> !;
    pub fn wait(addr: *const u8) -> i32;
    pub fn pipe(fdarray: *const i32) -> i32;
    pub fn write(fd: i32, addr: *const u8, n: i32) -> i32;
    pub fn read(fd: i32, addr: *mut u8, n: i32) -> i32;
    pub fn close(fd: i32);
    // int kill(int);
    pub fn exec(path: *const u8, argv: *const *const u8) -> i32;
    pub fn open(path: *const u8, omode: u64) -> i32;
    pub fn mknod(path: *const u8, major: u16, minior: u16) -> i32;
    // int unlink(const char*);
    // int fstat(int fd, struct stat*);
    // int link(const char*, const char*);
    // int mkdir(const char*);
    pub fn chdir(path: *const u8) -> i32;
    pub fn dup(fd: i32) -> i32;
    // int getpid(void);
    pub fn sbrk(n: u32) -> *mut u8;
    // int sleep(int);
    // int uptime(void);
}
