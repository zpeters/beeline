# beeline dev tasks. Run `just` to list them.

version := `grep -m1 '^version' Cargo.toml | cut -d'"' -f2`

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
    # Leftovers from an older bundle can stop cargo-bundle making the .dmg.
    rm -rf target/release/bundle
    cargo bundle --release

# Build beeline.app and copy it into /Applications, replacing any older copy.
install-mac: bundle-mac
    rm -rf /Applications/beeline.app
    cp -R target/release/bundle/osx/beeline.app /Applications/

# Build a drag-to-install disk image (beeline.app + an Applications link)
# at dist/beeline-<version>.dmg.
dmg: bundle-mac
    mkdir -p dist
    cp target/release/bundle/dmg/beeline.dmg dist/beeline-{{version}}.dmg
    @echo "dist/beeline-{{version}}.dmg"
