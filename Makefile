.PHONY: all check-e2e-binary check-file-lines diffo install diffo-mock e2e e2e-review measure-cpu measure-startup measure-text-readiness

# Run every automated repository check once. Workspace tests include the black-box
# diffo-e2e package and the diffo integration tests.
all:
	cargo fmt --all --check
	cargo test --workspace
	cargo clippy --workspace --all-targets --all-features -- -D warnings
	cargo doc --workspace --no-deps
	$(MAKE) check-e2e-binary
	$(MAKE) check-file-lines

check-e2e-binary:
	@unexpected=$$(rg -l 'CARGO_BIN_EXE_diffo' crates/diffo/tests | \
		rg -v 'crates/diffo/tests/(launcher.rs|live_refresh.rs|git_operations/support.rs)' || true); \
	test -z "$$unexpected" || { printf 'black-box tests bypass DIFFO_E2E_BINARY:\n%s\n' "$$unexpected"; exit 1; }

check-file-lines:
	@rg --files -g '*.rs' | { failed=; while IFS= read -r file; do \
		lines=$$(wc -l < "$$file"); \
		if [ "$$lines" -gt 700 ]; then \
			printf '%s has %s lines (maximum 700)\n' "$$file" "$$lines"; \
			failed=1; \
		fi; \
	done; \
	test -z "$$failed"; }

# Build and run the diff viewer using Cargo's debug profile.
diffo:
	cargo run --package diffo

# Build and install the diff viewer into Cargo's binary directory.
install:
	cargo install --path crates/diffo --locked

# Run the viewer with a mutable deterministic repository-state fixture.
diffo-mock:
	DIFFO_MOCK_FILE=crates/diffo-core/fixtures/repository-state.ron cargo run --package diffo

# Run only the compiled-binary black-box suites during focused E2E development.
e2e:
	cargo test --package diffo-e2e
	cargo test --package diffo --test git_operations

e2e-review:
	cargo insta test --package diffo-e2e

# Measure release-build CPU use in deterministic idle and scrolling workloads.
measure-cpu:
	cargo build --release --package diffo
	cargo run --release --package diffo-measure

# Measure time to first terminal output and usable repository state at startup.
measure-startup:
	cargo build --release --package diffo
	cargo run --release --package diffo-measure -- --startup

# Measure deterministic 100x30 Diff and Explorer text-readiness workloads.
measure-text-readiness:
	cargo build --release --package diffo
	cargo run --release --package diffo-measure -- --text-readiness
