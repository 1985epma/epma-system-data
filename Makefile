# Atalhos: o mesmo que o CI roda. `make check` antes de abrir um PR.
.PHONY: help fmt fmt-check lint test test-linux deny check

help: ## Lista os alvos
	@grep -E '^[a-z-]+:.*##' $(MAKEFILE_LIST) | awk -F':.*## ' '{printf "  %-12s %s\n", $$1, $$2}'

fmt: ## Formata o código
	cargo fmt --all

fmt-check: ## Falha se o código não estiver formatado
	cargo fmt --all -- --check

lint: ## Clippy sem avisos
	RUSTFLAGS="-D warnings" cargo clippy --locked --all-targets

test: ## Testes na máquina atual
	RUSTFLAGS="-D warnings" cargo test --locked

# Linux de verdade num contêiner (como root: o teste de permissão negada é pulado).
# O código entra somente leitura; build e cache ficam em volumes do Docker.
IMAGE ?= epma-system-data-test
test-linux: ## Testes em Linux (Docker)
	docker build -q -f Dockerfile.test -t $(IMAGE) .
	docker run --rm \
		-v "$(CURDIR)":/src:ro \
		-v epma-cargo-registry:/usr/local/cargo/registry \
		-v epma-target-linux:/tmp/target \
		-e CARGO_TARGET_DIR=/tmp/target \
		-e RUSTFLAGS="-D warnings" \
		$(IMAGE) cargo test --locked

deny: ## Licenças e avisos de segurança das dependências (cargo install cargo-deny)
	cargo deny check

check: fmt-check lint test deny ## Tudo que o CI verifica (menos o Linux)
