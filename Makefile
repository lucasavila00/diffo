.PHONY: diffo install diffo-mock e2e e2e-review

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
