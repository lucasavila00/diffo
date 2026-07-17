.PHONY: diffo diffo-mock

# Build and run the diff viewer using Cargo's debug profile.
diffo:
	cargo run --package git-diff-tui --bin diffo

# Run the viewer with a deterministic repository-state fixture.
diffo-mock:
	DIFFO_MOCK_FILE=crates/git-diff-tui/fixtures/repository-state.ron cargo run --package git-diff-tui --bin diffo
