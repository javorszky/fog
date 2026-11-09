CARGO ?= mise exec -- cargo

.PHONY: build release test lint fmt install hooks clean

build:
	$(CARGO) build

release:
	$(CARGO) build --release

test:
	$(CARGO) test --locked

lint:
	$(CARGO) fmt --check
	$(CARGO) clippy --locked --all-targets -- -D warnings

fmt:
	$(CARGO) fmt

install:
	$(CARGO) install --path .

hooks:
	mise exec -- lefthook install

clean:
	$(CARGO) clean
