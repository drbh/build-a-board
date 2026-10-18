.DEFAULT_GOAL := bb

PROFILE ?= release
CARGO ?= cargo
NPM ?= npm

.PHONY: help bb test playground book site deploy

help:
	@printf '%s\n' 'make [bb]      Build the CLI (target/release/bb)' 'make test      Run the workspace tests' 'make playground Build the browser playground into playground/dist' 'make book      Render book/ to playground/dist/book' 'make site      Stage the hosted site (playground, book, footprints) in playground/cloud' 'make deploy    Upload and deploy bb.dholtz.com (needs CLOUDFLARE_API_TOKEN)' 'Options: PROFILE=dev, KICAD_FOOTPRINT_DIR=<dir>'

bb:
	$(CARGO) build --locked -p build-a-board --profile $(PROFILE)

test:
	$(CARGO) test --locked --workspace

playground:
	playground/build.sh

# Rendered after the playground, since Vite empties dist/ when it builds.
book: playground
	$(NPM) --prefix playground run book

site: book
	playground/cloud/stage.sh

deploy: site
	playground/cloud/deploy.sh
