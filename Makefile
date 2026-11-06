SHELL := bash
.SHELLFLAGS := -eu -o pipefail -c

PYTHON_BIN := $(shell if command -v python3 >/dev/null 2>&1 && python3 -c "import sys" >/dev/null 2>&1; then echo python3; elif command -v python >/dev/null 2>&1 && python -c "import sys" >/dev/null 2>&1; then echo python; elif command -v py >/dev/null 2>&1 && py -3 -c "import sys" >/dev/null 2>&1; then echo "py -3"; else echo python3; fi)

.PHONY: all check check-no-network check-errors check-contrast check-i18n check-migrations fmt fmt-check lint test build codeburn security

all: check

check: check-no-network check-errors check-contrast check-i18n check-migrations fmt-check lint test build codeburn security
	@echo "[make check] ALL GATES PASSED SUCCESSFULLY (SafiData v5.0 Gold Master -- T1.12)."

check-no-network:
	bash tools/check-no-network.sh
	bash tools/check-no-network.sh --self-test

check-errors:
	$(PYTHON_BIN) tools/gen-errors.py --check
	$(PYTHON_BIN) tools/gen-errors.py --self-test

check-contrast:
	pnpm run check:contrast

check-i18n:
	$(PYTHON_BIN) tools/verify-i18n.py
	$(PYTHON_BIN) tools/verify-i18n.py --self-test

check-migrations:
	$(PYTHON_BIN) tools/verify-migrations.py
	$(PYTHON_BIN) tools/verify-migrations.py --self-test

fmt:
	cargo fmt --all
	pnpm run format

fmt-check:
	cargo fmt --all -- --check
	pnpm run format:check

lint:
	cargo clippy --workspace --all-targets -- -D warnings
	pnpm run typecheck
	pnpm run lint

test:
	cargo test --workspace
	pnpm run test

build:
	pnpm run build

codeburn:
	bash tools/codeburn/burn.sh

security:
	$(PYTHON_BIN) tools/verify-security.py
	$(PYTHON_BIN) tools/seed-data/no-cost-gen.py --verify
	pnpm audit
