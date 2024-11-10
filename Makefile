PACKAGES := kernel mkfs user

.PHONY: all build-all clean kernel mkfs user

all: build-all

kernel:
	cd kernel && cargo build

mkfs:
	cd mkfs && cargo build

user:
	cd user && cargo build

build-all: kernel mkfs user

build-fs: build-all
	mkdir -p mkfs/user
	find user/target/riscv64gc-unknown-none-elf/debug -type f -regex '.*/_[a-zA-Z0-9_]*' -exec cp {} mkfs/user \;
	$(MAKE) -C mkfs run

run-debug: build-fs
	cd kernel && cargo run

clean:
	for pkg in $(PACKAGES); do \
		(cd $$pkg && cargo clean); \
	done
	$(MAKE) -C mkfs clean
	$(MAKE) -C user/initcode clean