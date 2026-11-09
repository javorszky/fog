CARGO ?= mise exec -- cargo

.PHONY: build release test lint fmt install clean

build:
	$(CARGO) build

release:
	$(CARGO) build --release

test:
	$(CARGO) test

lint:
	$(CARGO) fmt --check
	$(CARGO) clippy --all-targets -- -D warnings

fmt:
	$(CARGO) fmt

install:
	$(CARGO) install --path .

clean:
	$(CARGO) clean
