.PHONY: diffo install diffo-mock e2e e2e-review measure-cpu measure-text-readiness

# Build and run the diff viewer using Cargo's debug profile.
diffo:
	cargo run --package diffo

# Build and install the diff viewer into Cargo's binary directory.
install:
	cargo install --path crates/diffo --locked

# Run the viewer with a mutable deterministic repository-state fixture.
diffo-mock:
	DIFFO_MOCK_FILE=crates/diffo-core/fixtures/repository-state.ron cargo run --package diffo

e2e:
	cargo test --package diffo-e2e

e2e-review:
	cargo insta test --package diffo-e2e

# Measure release-build CPU use in deterministic idle and scrolling workloads.
measure-cpu:
	cargo build --release --package diffo
	cargo run --release --package diffo-measure

# Measure deterministic 100x30 Diff and Explorer text-readiness workloads.
measure-text-readiness:
	cargo build --release --package diffo
	cargo run --release --package diffo-measure -- --text-readiness
