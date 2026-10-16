# beeline dev tasks. Run `just` to list them.

default:
    @just --list

# Build everything.
build:
    cargo build

# Launch the app.
run:
    cargo run

# Run the test suite.
test:
    cargo test

# Format + lint gate.
check:
    cargo fmt --check
    cargo clippy -- -D warnings
    cargo test

# Build beeline.app (macOS) into target/release/bundle/osx/.
bundle-mac:
    cargo bundle --release

# Build beeline.app and copy it into /Applications, replacing any older copy.
install-mac: bundle-mac
    rm -rf /Applications/beeline.app
    cp -R target/release/bundle/osx/beeline.app /Applications/
