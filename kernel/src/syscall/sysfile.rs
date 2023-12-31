use crate::param::{MAXARG, MAXPATH};
use crate::syscall::syscall::argaddr;

fn sys_exec() -> u64 {
    let path: [u8; MAXPATH] = [0; MAXPATH];
    let path: [Option<&u8>; MAXARG] = [None; MAXARG];
    int i;

    let uarg: u64 = 0;
    let uargv = argaddr(1);
    if(argstr(0, path, MAXPATH) < 0) {
        return -1;
    }
    memset(argv, 0, sizeof(argv));
    for(i=0;; i++){
        if(i >= NELEM(argv)){
            goto bad;
        }

        if(fetchaddr(uargv+sizeof(uint64)*i, (uint64*)&uarg) < 0){
            goto bad;
        }

        if(uarg == 0){
            argv[i] = 0;
            break;
        }
        argv[i] = kalloc();
        if(argv[i] == 0)
            goto bad;
        if(fetchstr(uarg, argv[i], PGSIZE) < 0)
            goto bad;
    }

    int ret = exec(path, argv);

    for(i = 0; i < NELEM(argv) && argv[i] != 0; i++)
        kfree(argv[i]);

    return ret;

    bad:
        for(i = 0; i < NELEM(argv) && argv[i] != 0; i++)
            kfree(argv[i]);
        return -1;
}
