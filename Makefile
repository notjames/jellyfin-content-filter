.PHONY: all rust web desktop test lint package install uninstall clean

all: rust web desktop

rust:
	cargo build --release

web:
	npm run build

desktop:
	npm run build:desktop

test:
	cargo test --workspace
	npm test

lint:
	npm run lint

package:
	cargo build --release
	npm run build:all
	VERSION="$${VERSION:?Set VERSION}" packaging/scripts/package-native.sh
	VERSION="$${VERSION:?Set VERSION}" packaging/scripts/package-plugin.sh

install:
	packaging/scripts/install.sh

uninstall:
	packaging/scripts/uninstall.sh

clean:
	rm -rf web/dist desktop/dist playwright-report test-results
