FILE_LIST=$(wildcard user/*)

run:
	cargo run -- --output-name=fs.img $(FILE_LIST)

clean:
	rm -f fs.img & rm -rf user