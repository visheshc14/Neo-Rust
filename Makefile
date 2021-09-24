.PHONY: all clean \
				get-version get-current-sha get-project-name \
				check-tool-cargo check-tool-cargo-watch \
				setup run run-tls watch cargo-clean \
				build build-watch build-release build-test build-test-watch \
				test test-unit test-unit-debug test-int test-int-debug test-e2e test-e2e-debug \
				release-prep release-publish \
				image image-publish image-release registry-login \
				builer-image builer-image-publish \
				tls-clean tls-credentials tls-run \
				example example-tls

all: build

clean: cargo-clean tls-clean

PROJECT_NAME ?= Neo-Rust

VERSION=$(shell perl -ne '/version\s+=\s+"([0-9|\.]+)"$$/ && print $$1 and last' Cargo.toml)
CURRENT_SHA ?= $(shell $(GIT) rev-parse --short HEAD)

BUILD_TARGET ?= x86_64-unknown-linux-musl
CARGO_BUILD_FLAGS ?= --target $(BUILD_TARGET)
GIT_REMOTE ?= origin

DOCKER ?= docker
CARGO ?= cargo
CARGO_WATCH ?= cargo-watch
SHA256SUM ?= sha256sum
SHA1SUM ?= sha1sum
GIT ?= git

RELEASES_DIR ?= releases
RELEASE_BINARY_NAME ?= Neo
CARGO_TARGET_DIR ?= target
RELEASE_BUILT_BIN_PATH = $(CARGO_TARGET_DIR)/$(BUILD_TARGET)/release/$(RELEASE_BINARY_NAME)

check-tool-docker:
ifeq (, $(shell which $(DOCKER)))
	$(error "`docker` is not available please install docker (https://docker.com)")
endif

check-tool-cargo:
ifeq (, $(shell which $(CARGO)))
	$(error "`cargo` is not available please install cargo (https://github.com/rust-lang/cargo/)")
endif

check-tool-cargo-watch:
ifeq (, $(shell which $(CARGO_WATCH)))
	$(error "`cargo-watch` is not available please install cargo-watch (https://github.com/passcod/cargo-watch)")
endif

check-tool-sha256sum:
ifeq (, $(shell which $(SHA256SUM)))
	$(error "`sha256sum` is not available please install sha256sum")
endif

check-tool-sha1sum:
ifeq (, $(shell which $(SHA1SUM)))
	$(error "`sha1sum` is not available please install sha1sum")
endif

get-version:
	@echo -e -n ${VERSION}

get-current-sha:
	@echo -e -n ${CURRENT_SHA}

get-project-name:
	@echo -e -n ${PROJECT_NAME}

setup:
	@echo "[info] installing rust utilities..."
	$(CARGO) install cargo-watch

fmt:
	$(CARGO) fmt

cargo-clean: check-tool-cargo
	$(CARGO) clean

build: check-tool-cargo
	$(CARGO) build $(CARGO_BUILD_FLAGS)

build-release: check-tool-cargo check-tool-sha1sum check-tool-sha256sum
	$(CARGO) build $(CARGO_BUILD_FLAGS) --release
	cp $(RELEASE_BUILT_BIN_PATH) $(RELEASES_DIR)/$(RELEASE_BINARY_NAME)

release-prep: build-release check-tool-sha1sum check-tool-sha256sum
	$(SHA256SUM) $(RELEASES_DIR)/${RELEASE_BINARY_NAME} > $(RELEASES_DIR)/${RELEASE_BINARY_NAME}.sha256sum
	$(SHA1SUM) $(RELEASES_DIR)/${RELEASE_BINARY_NAME} > $(RELEASES_DIR)/${RELEASE_BINARY_NAME}.sha1sum
	$(GIT) add .
	$(GIT) commit -am "Release v$(VERSION)"
	$(GIT) tag v$(VERSION) HEAD

release-publish:
	$(GIT) push $(GIT_REMOTE) v$(VERSION)

build-test: check-tool-cargo
	$(CARGO) test $(CARGO_BUILD_FLAGS) --no-run

install: check-tool-cargo
	$(CARGO) install --path . $(RELEASE_BINARY_NAME)

run:
	$(CARGO) run

test: test-unit test-int test-e2e

test-unit:
	$(CARGO) test $(CARGO_BUILD_FLAGS)

test-unit-debug:
	$(CARGO) test $(CARGO_BUILD_FLAGS) -- --nocapture

test-int:
	$(CARGO) test $(CARGO_BUILD_FLAGS) -- --ignored _int

test-int-debug:
	$(CARGO) test $(CARGO_BUILD_FLAGS) -- --ignored _int --nocapture

test-e2e: build-release
	$(CARGO) test $(CARGO_BUILD_FLAGS) -- --ignored _e2e

test-e2e-debug: build-release
	$(CARGO) test $(CARGO_BUILD_FLAGS) -- --ignored _e2e --nocapture

build-watch: check-tool-cargo check-tool-cargo-watch
	$(CARGO_WATCH) -x "build $(CARGO_BUILD_FLAGS)" --watch src

build-test-watch: check-tool-cargo check-tool-cargo-watch
	$(CARGO_WATCH) -x "test $(CARGO_BUILD_FLAGS)" --watch src --watch tests

#############
# Packaging #
#############

REGISTRY_PATH = registry.gitlab.com/mrman/$(PROJECT_NAME)

IMAGE_NAME ?= cli

IMAGE_FULL_NAME=${REGISTRY_PATH}/${IMAGE_NAME}:v${VERSION}
IMAGE_FULL_NAME_SHA=${REGISTRY_PATH}/${IMAGE_NAME}:v${VERSION}-${CURRENT_SHA}

BUILDER_IMAGE_NAME ?= builder
BUILDER_FULL_NAME = ${REGISTRY_PATH}/${BUILDER_IMAGE_NAME}:v${VERSION}

registry-login:
		cat infra/secrets/ci-deploy-token-password.secret | \
		docker login \
			-u $(shell cat infra/secrets/ci-deploy-token-username.secret) \
			--password-stdin \
			registry.gitlab.com

image:
		docker build -f infra/docker/Dockerfile -t $(IMAGE_FULL_NAME_SHA) .

image-publish: check-tool-docker
	$(DOCKER) push ${IMAGE_FULL_NAME_SHA}

image-release:
	$(DOCKER) tag $(IMAGE_FULL_NAME_SHA) $(IMAGE_FULL_NAME)
	$(DOCKER) push $(IMAGE_FULL_NAME)

builder-image:
		docker build -f infra/docker/builder.Dockerfile -t $(BUILDER_FULL_NAME) .

builder-image-publish:
		docker push $(BUILDER_FULL_NAME)


###########
# Testing #
###########

OPENSSL ?= openssl
TLS_KEY_PATH ?= $(realpath ./tls.key)
TLS_CERT_PATH ?= $(realpath ./tls.crt)

tls-clean:
	rm -f $(TLS_KEY_PATH)
	rm -f $(TLS_CERT_PATH)

tls-credentials:
	@if [ ! -f "$(TLS_KEY_PATH)" ] && [ ! -f "$(TLS_CERT_PATH)" ] ; then \
		echo "[info] neither TLS key (@ $(TLS_KEY_PATH)) or TLS cert (@ $(TLS_CERT_PATH)) exist, creating..."; \
		$(OPENSSL) req -nodes -new -x509 \
			-keyout $(TLS_KEY_PATH) \
			-out $(TLS_CERT_PATH) \
			-subj '/CN=Neo.localhost' \
			-days 3650; \
	else \
		echo "[info] TLS key (@ $(TLS_KEY_PATH)) and TLS cert (@ $(TLS_CERT_PATH)) already exist"; \
	fi

tls-run:
	TLS_KEY=$(TLS_KEY_PATH) TLS_CERT=$(TLS_CERT_PATH) $(CARGO) run

###########
# Example #
###########

EXAMPLE_FILE_PATH= ./example.html

example:
	FILE=$(EXAMPLE_FILE_PATH) $(CARGO) run

example-tls:
	FILE=$(EXAMPLE_FILE_PATH) $(MAKE) tls-run
