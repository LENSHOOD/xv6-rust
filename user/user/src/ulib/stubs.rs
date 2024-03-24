extern {
    // system calls
    // int fork(void);
    // int exit(int) __attribute__((noreturn));
    // int wait(int*);
    // int pipe(int*);
    pub fn write(fd: i32, data: *const u8, sz: i32) -> i32;
    // int read(int, void*, int);
    // int close(int);
    // int kill(int);
    // int exec(const char*, char**);
    // int open(const char*, int);
    // int mknod(const char*, short, short);
    // int unlink(const char*);
    // int fstat(int fd, struct stat*);
    // int link(const char*, const char*);
    // int mkdir(const char*);
    // int chdir(const char*);
    // int dup(int);
    // int getpid(void);
    // char* sbrk(int);
    // int sleep(int);
    // int uptime(void);
}